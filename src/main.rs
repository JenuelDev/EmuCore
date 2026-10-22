#![forbid(unsafe_code)]

use emucore::{
    Error, ErrorKind, Result,
    decode::Program,
    elf::ElfImage,
    fixture,
    process::ProcessOptions,
    runtime::{self, Backend},
};
use std::{
    env,
    ffi::OsString,
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const WORKER_TIMEOUT: Duration = Duration::from_secs(5);
const USAGE: &str = concat!(
    "EmuCore ELF prototype\n\n",
    "  emucore fixture <new-file.elf>         Generate the hello fixture\n",
    "  emucore memory-fixture <new-file.elf>  Generate the writable-memory fixture\n",
    "  emucore args-fixture <new-file.elf>    Generate the argument echo fixture\n",
    "  emucore inspect <file.elf>             Validate ELF and supported instructions\n",
    "  emucore run <file.elf> [-- arguments]  Run in a worker (5-second timeout)\n",
    "  emucore profile <file> [-- arguments] Report stage timings as JSON\n\n",
    "Execution supports only the documented static x86-64 subset, not Android apps.\n",
);

fn main() {
    let code = match execute() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("emucore: {error}");
            if error.kind == ErrorKind::Usage {
                2
            } else {
                125
            }
        }
    };
    std::process::exit(code);
}

fn execute() -> Result<i32> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.is_empty()
        || (arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h")))
    {
        print!("{USAGE}");
        return Ok(0);
    }
    if arguments.len() < 2 {
        return Err(Error::new(
            ErrorKind::Usage,
            "expected a command and a file; use --help",
        ));
    }
    let command = arguments[0].to_str();
    let can_forward = matches!(
        command,
        Some("run" | "profile" | "__worker" | "__profile_worker")
    );
    if arguments.len() > 2 && (!can_forward || arguments[2] != "--") {
        return Err(Error::new(
            ErrorKind::Usage,
            "guest arguments must follow run/profile <file> --; use --help",
        ));
    }
    let guest_arguments = if arguments.len() > 2 {
        &arguments[3..]
    } else {
        &[]
    };
    let path = Path::new(&arguments[1]);
    match arguments[0].to_str() {
        Some("fixture" | "memory-fixture" | "args-fixture") => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| {
                    Error::io("create fixture (existing files are never overwritten)", e)
                })?;
            let fixture = match command {
                Some("memory-fixture") => fixture::memory_demo(),
                Some("args-fixture") => fixture::arguments_demo(),
                _ => fixture::hello(),
            };
            file.write_all(&fixture)
                .map_err(|e| Error::io("write fixture", e))?;
            println!("Created {}", path.display());
            Ok(0)
        }
        Some("inspect") => {
            let image = ElfImage::from_path(path)?;
            let program = Program::decode(&image)?;
            println!(
                "ELF64 x86-64 ET_EXEC: entry=0x{:x}, image={} bytes, instructions={}",
                image.entry(),
                image.image_size(),
                program.instructions().len()
            );
            for segment in image.segments() {
                println!(
                    "  LOAD address=0x{:x} file={} memory={} flags={:#x}",
                    segment.address, segment.file_size, segment.memory_size, segment.flags
                );
            }
            Ok(0)
        }
        Some("run" | "profile") => supervise(path, guest_arguments, command == Some("profile")),
        Some("__worker" | "__profile_worker") => {
            let worker_started = Instant::now();
            let load_started = Instant::now();
            let image = ElfImage::from_path(path)?;
            let load = load_started.elapsed();
            let mut options = ProcessOptions::default();
            for argument in guest_arguments {
                options.arguments.push(
                    argument
                        .to_str()
                        .ok_or_else(|| {
                            Error::new(ErrorKind::Usage, "guest arguments must be valid Unicode")
                        })?
                        .to_owned(),
                );
            }
            if command == Some("__profile_worker") {
                let profile = runtime::run_profiled(&image, Backend::Native, &options)?;
                let worker_elapsed = worker_started.elapsed();
                let metrics = &profile.metrics;
                let result = &profile.result;
                println!(
                    concat!(
                        "{{\"schema_version\":1,\"backend\":\"native\",",
                        "\"load_ns\":{},\"decode_ns\":{},\"process_setup_ns\":{},",
                        "\"translation_ns\":{},\"executable_setup_ns\":{},\"execution_ns\":{},",
                        "\"runtime_total_ns\":{},\"worker_total_ns\":{},",
                        "\"decoded_instructions\":{},\"executed_instructions\":{},",
                        "\"native_code_bytes\":{},\"guest_mapped_bytes_final\":{},",
                        "\"guest_mapped_bytes_peak\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},",
                        "\"exit_code\":{}}}"
                    ),
                    load.as_nanos(),
                    metrics.decode.as_nanos(),
                    metrics.process_setup.as_nanos(),
                    metrics.translation.as_nanos(),
                    metrics.executable_setup.as_nanos(),
                    metrics.execution.as_nanos(),
                    metrics.total.as_nanos(),
                    worker_elapsed.as_nanos(),
                    metrics.decoded_instructions,
                    result.instructions_executed,
                    metrics.native_code_bytes,
                    metrics.guest_mapped_bytes_final,
                    metrics.guest_mapped_bytes_peak,
                    result.stdout.len(),
                    result.stderr.len(),
                    result.exit_code,
                );
                return Ok(i32::from(result.exit_code));
            }
            let result = runtime::run_with_options(&image, Backend::Native, &options)?;
            std::io::stdout()
                .lock()
                .write_all(&result.stdout)
                .map_err(|e| Error::io("write stdout", e))?;
            std::io::stderr()
                .lock()
                .write_all(&result.stderr)
                .map_err(|e| Error::io("write stderr", e))?;
            Ok(i32::from(result.exit_code))
        }
        _ => Err(Error::new(ErrorKind::Usage, "unknown command; use --help")),
    }
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        // Also clean up on timeout or a failed wait; a reaped worker makes these no-ops/errors.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn supervise(path: &Path, guest_arguments: &[OsString], profile: bool) -> Result<i32> {
    let executable = env::current_exe().map_err(|e| Error::io("locate worker executable", e))?;
    let mut command = Command::new(executable);
    command
        .arg(if profile {
            "__profile_worker"
        } else {
            "__worker"
        })
        .arg(path)
        .arg("--")
        .args(guest_arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut worker = Worker(
        command
            .spawn()
            .map_err(|e| Error::io("start execution worker", e))?,
    );
    let started = Instant::now();
    loop {
        if let Some(status) = worker
            .0
            .try_wait()
            .map_err(|e| Error::io("wait for execution worker", e))?
        {
            return status.code().ok_or_else(|| {
                Error::new(
                    ErrorKind::Host,
                    "execution worker terminated without an exit code",
                )
            });
        }
        if started.elapsed() >= WORKER_TIMEOUT {
            return Err(Error::new(
                ErrorKind::Host,
                "execution worker exceeded its 5-second deadline",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
