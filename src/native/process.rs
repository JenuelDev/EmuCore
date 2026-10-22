//! Audited Windows job/thread boundary. The public host API contains no unsafe code.

use crate::{Error, ErrorKind, Result};
use std::{
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Child, Command, ExitStatus},
    ptr,
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        },
    },
};

pub(crate) struct JobProcess {
    pub child: Child,
    job: OwnedHandle,
}

fn host_error(operation: &str) -> Error {
    Error::new(
        ErrorKind::Host,
        format!("{operation}: {}", std::io::Error::last_os_error()),
    )
}

fn create_job() -> Result<OwnedHandle> {
    // SAFETY: null security attributes produce a non-inheritable handle, and null
    // name creates a new unnamed job rather than opening another process owner's job.
    let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if raw.is_null() {
        return Err(host_error("CreateJobObjectW"));
    }
    // SAFETY: a successful CreateJobObjectW returns a unique owned real handle.
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: this C data structure contains only integers and integer-only structs.
    // All-zero values are valid; only the explicitly enabled limit is interpreted.
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: the handle is live and the input pointer has the exact ABI structure
    // and byte size for JobObjectExtendedLimitInformation for the duration of the call.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(host_error("SetInformationJobObject"));
    }
    Ok(job)
}

fn resume_primary_thread(process_id: u32) -> Result<()> {
    // SAFETY: TH32CS_SNAPTHREAD needs no input pointers; zero PID means all threads.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(host_error("CreateToolhelp32Snapshot"));
    }
    // SAFETY: this is a newly returned live snapshot handle, owned exactly once.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: THREADENTRY32 consists entirely of integer fields, and zero is valid.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of_val(&entry) as u32;
    // SAFETY: snapshot is a live thread snapshot and entry is a valid writable
    // THREADENTRY32 initialized with its required byte size.
    if unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } == 0 {
        return Err(host_error("Thread32First"));
    }
    loop {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: the snapshot supplies this thread ID; only suspend/resume
            // access is requested, and its owner is our live suspended child.
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if raw.is_null() {
                return Err(host_error("OpenThread"));
            }
            // SAFETY: a successful OpenThread returns a newly owned real handle.
            let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
            // SAFETY: the owned process was created CREATE_SUSPENDED and has not
            // run user code. Its primary thread is resumed only after job assignment.
            let previous = unsafe { ResumeThread(thread.as_raw_handle()) };
            if previous == u32::MAX {
                return Err(host_error("ResumeThread"));
            }
            if previous != 1 {
                return Err(Error::new(
                    ErrorKind::Host,
                    "owned primary thread had an unexpected suspend count",
                ));
            }
            return Ok(());
        }
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        // SAFETY: the same live snapshot and correctly sized writable entry are reused.
        if unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } == 0 {
            return Err(Error::new(
                ErrorKind::Host,
                "no primary thread found for owned suspended process",
            ));
        }
    }
}

impl JobProcess {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        let job = create_job()?;
        command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let child = command
            .spawn()
            .map_err(|error| Error::io("start owned suspended process", error))?;
        // Construct the owner before any fallible assignment/resume step, so every
        // error path terminates and reaps the still-suspended child as well as the job.
        let process = Self { child, job };
        // SAFETY: both handles remain live; this is our newly created suspended child,
        // so it cannot spawn descendants before joining our non-breakaway job.
        if unsafe {
            AssignProcessToJobObject(process.job.as_raw_handle(), process.child.as_raw_handle())
        } == 0
        {
            return Err(host_error("AssignProcessToJobObject"));
        }
        resume_primary_thread(process.child.id())?;
        Ok(process)
    }

    pub fn terminate(&mut self) -> Result<ExitStatus> {
        // SAFETY: this owner holds a live private job handle containing only its
        // child tree; termination does not target processes by an externally supplied PID.
        if unsafe { TerminateJobObject(self.job.as_raw_handle(), 125) } == 0 {
            return Err(host_error("TerminateJobObject"));
        }
        self.child
            .wait()
            .map_err(|error| Error::io("reap terminated owned process", error))
    }

    pub fn memory_peaks(&self) -> Result<(usize, usize)> {
        // SAFETY: the output structure has only integer fields and permits zero initialization.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the private job is live and limits is writable for exactly the
        // documented output structure's size; the optional returned size is unused.
        if unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(host_error("QueryInformationJobObject"));
        }
        Ok((limits.PeakJobMemoryUsed, limits.PeakProcessMemoryUsed))
    }
}

impl Drop for JobProcess {
    fn drop(&mut self) {
        // SAFETY: terminate the owned job even after the root process has exited.
        // The non-inheritable job handle also enforces kill-on-close after a host crash.
        unsafe { TerminateJobObject(self.job.as_raw_handle(), 125) };
        // Assignment can fail while the child is suspended and not yet in the job.
        // Directly killing that exact owned child covers that error path too.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_process::ManagedProcess;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Stdio,
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
    const HELPER: &str = "native::process::tests::helper_process";

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "emucore-job-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn helper(mode: &str, marker: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", HELPER, "--nocapture", "--test-threads=1"])
            .env("EMUCORE_TEST_JOB_MODE", mode)
            .env("EMUCORE_TEST_JOB_MARKER", marker)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn marker_pid(path: &Path) -> u32 {
        let started = Instant::now();
        loop {
            if let Ok(text) = fs::read_to_string(path)
                && let Ok(pid) = text.parse()
            {
                return pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "owned helper did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn observe_process(pid: u32) -> OwnedHandle {
        // SAFETY: the PID comes from our live owned child or its private marker;
        // only synchronization rights are requested, with no mutation by PID.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!raw.is_null(), "could not observe owned process");
        // SAFETY: OpenProcess succeeded and returned a newly owned real handle.
        unsafe { OwnedHandle::from_raw_handle(raw) }
    }

    fn wait_state(handle: &OwnedHandle, milliseconds: u32) -> u32 {
        // SAFETY: the owned process handle remains live and has SYNCHRONIZE rights.
        unsafe { WaitForSingleObject(handle.as_raw_handle(), milliseconds) }
    }

    #[test]
    fn helper_process() {
        let Ok(mode) = std::env::var("EMUCORE_TEST_JOB_MODE") else {
            return;
        };
        let marker = PathBuf::from(std::env::var_os("EMUCORE_TEST_JOB_MARKER").unwrap());
        match mode.as_str() {
            "loop" => {
                let backing = vec![7_u8; 16 * 1024 * 1024];
                fs::write(marker, std::process::id().to_string()).unwrap();
                loop {
                    std::hint::black_box(&backing);
                    thread::park_timeout(Duration::from_secs(30));
                }
            }
            "descendant" => {
                let mut command = helper("loop", &marker);
                command.creation_flags(CREATE_NO_WINDOW);
                // The OS must inherit our job without assistance from ManagedProcess.
                let descendant = command.spawn().unwrap();
                drop(descendant);
            }
            "arguments" => {
                let expected = std::env::var_os("EMUCORE_TEST_JOB_ARGUMENT").unwrap();
                assert!(std::env::args_os().any(|argument| argument == expected));
                fs::write(marker, std::process::id().to_string()).unwrap();
            }
            _ => panic!("unknown owned helper mode"),
        }
    }

    #[test]
    fn normal_exit_preserves_quoted_unicode_arguments() {
        let directory = Directory::new();
        let marker = directory.0.join("ready");
        let expected = "space 🦀 with \"quotes\" and a trailing slash \\";
        let mut command = helper("arguments", &marker);
        command
            .args(["--skip", expected])
            .env("EMUCORE_TEST_JOB_ARGUMENT", expected);
        let mut process = ManagedProcess::spawn(&mut command).unwrap();
        let status = process.wait_for(Duration::from_secs(5)).unwrap().unwrap();
        assert!(status.success());
        assert_eq!(marker_pid(&marker), process.id());
    }

    #[test]
    fn timeout_then_drop_stops_the_owned_process_and_tracks_memory() {
        let directory = Directory::new();
        let marker = directory.0.join("ready");
        let mut process = ManagedProcess::spawn(&mut helper("loop", &marker)).unwrap();
        assert_eq!(marker_pid(&marker), process.id());
        let observed = observe_process(process.id());
        assert!(process.wait_for(Duration::ZERO).unwrap().is_none());
        assert_eq!(wait_state(&observed, 0), WAIT_TIMEOUT);
        let memory = process.memory_peaks().unwrap();
        assert!(memory.peak_process_bytes >= 16 * 1024 * 1024);
        assert!(memory.peak_job_bytes >= memory.peak_process_bytes);
        drop(process);
        assert_eq!(wait_state(&observed, 5000), WAIT_OBJECT_0);
    }

    #[test]
    fn dropping_owner_stops_descendants_even_after_root_normal_exit() {
        let directory = Directory::new();
        let marker = directory.0.join("descendant");
        let mut process = ManagedProcess::spawn(&mut helper("descendant", &marker)).unwrap();
        assert!(
            process
                .wait_for(Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .success()
        );
        let descendant_pid = marker_pid(&marker);
        assert_ne!(descendant_pid, process.id());
        let observed = observe_process(descendant_pid);
        assert_eq!(wait_state(&observed, 0), WAIT_TIMEOUT);
        drop(process);
        assert_eq!(wait_state(&observed, 5000), WAIT_OBJECT_0);
    }

    #[test]
    fn explicit_termination_returns_the_owned_job_exit_status() {
        let directory = Directory::new();
        let marker = directory.0.join("ready");
        let mut process = ManagedProcess::spawn(&mut helper("loop", &marker)).unwrap();
        marker_pid(&marker);
        assert_eq!(process.terminate().unwrap().code(), Some(125));
        assert_eq!(process.try_wait().unwrap().unwrap().code(), Some(125));
    }

    #[test]
    fn missing_executable_returns_an_io_error_without_a_running_child() {
        let directory = Directory::new();
        let error = match ManagedProcess::spawn(&mut Command::new(directory.0.join("missing.exe")))
        {
            Ok(_) => panic!("unexpected executable in fresh test directory"),
            Err(error) => error,
        };
        assert_eq!(error.kind, ErrorKind::Io);
    }
}
