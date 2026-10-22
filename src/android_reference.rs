//! Explicit external-SDK lifecycle experiment, separate from the direct ELF runtime.
#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    host_process::{JobMemory, ManagedProcess},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const MARKER: &str = "EmuCore Android reference v1\n";
const MAX_COMMAND_OUTPUT: u64 = 256 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Starting,
    AndroidReady,
    LaunchingApplication,
    ApplicationRunning,
    Stopping,
    Stopped,
    Failed,
}

/// Configuration construction is inert. Only ReferenceSession::start launches processes.
#[derive(Clone, Debug)]
pub struct ReferenceConfig {
    pub sdk_root: PathBuf,
    pub workspace_root: PathBuf,
    pub output_directory: PathBuf,
    pub boot_timeout: Duration,
}

impl ReferenceConfig {
    pub fn new(sdk_root: PathBuf, workspace_root: PathBuf, output_directory: PathBuf) -> Self {
        Self {
            sdk_root,
            workspace_root,
            output_directory,
            boot_timeout: Duration::from_secs(120),
        }
    }

    fn validate(&mut self) -> Result<()> {
        if !(Duration::from_secs(30)..=Duration::from_secs(300)).contains(&self.boot_timeout) {
            return Err(usage(
                "Android reference boot timeout must be 30 to 300 seconds",
            ));
        }
        self.workspace_root = fs::canonicalize(&self.workspace_root)
            .map_err(|e| Error::io("resolve workspace", e))?;
        self.sdk_root =
            fs::canonicalize(&self.sdk_root).map_err(|e| Error::io("resolve SDK", e))?;
        for relative in ["emulator/emulator.exe", "platform-tools/adb.exe"] {
            if !self.sdk_root.join(relative).is_file() {
                return Err(usage(format!("missing installed SDK tool: {relative}")));
            }
        }
        let state = fs::canonicalize(self.workspace_root.join(".tools/android"))
            .map_err(|e| Error::io("resolve prepared Android state", e))?;
        if !state.starts_with(&self.workspace_root) {
            return Err(usage(
                "prepared Android state must resolve inside the workspace",
            ));
        }
        let marker = read_bounded(&state.join("avd/EmuCoreReference35.avd/.emucore-reference"))?;
        if marker != MARKER {
            return Err(usage(
                "expected the prepared project-owned Android reference marker",
            ));
        }
        let metadata = read_bounded(
            &self
                .sdk_root
                .join("system-images/android-35/google_apis_playstore/x86_64/source.properties"),
        )?;
        if property(&metadata, "Pkg.Revision") != Some("9") {
            return Err(usage(
                "Android reference requires the installed image revision 9",
            ));
        }
        let output = if self.output_directory.is_absolute() {
            self.output_directory.clone()
        } else {
            self.workspace_root.join(&self.output_directory)
        };
        let filename = output
            .file_name()
            .ok_or_else(|| usage("expected a new output directory name"))?;
        let parent = fs::canonicalize(
            output
                .parent()
                .ok_or_else(|| usage("output has no parent"))?,
        )
        .map_err(|e| Error::io("resolve output parent", e))?;
        if !parent.starts_with(&self.workspace_root) {
            return Err(usage("reference output must resolve inside the workspace"));
        }
        self.output_directory = parent.join(filename);
        if self.output_directory.exists() {
            return Err(usage(
                "use a new output directory; existing reference captures are preserved",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReadyInfo {
    pub boot_completed: Duration,
    pub package_ready: Duration,
    pub build_fingerprint: String,
    pub abis: Vec<String>,
    pub native_bridge: String,
    pub renderer: String,
    pub configured_guest_memory_mb: u32,
    pub guest_memtotal_kib: u64,
    pub memory_peaks: JobMemory,
}

#[derive(Clone, Copy, Debug)]
pub struct ShutdownInfo {
    pub graceful: bool,
    pub memory_peaks: JobMemory,
}

struct CommandOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    fn require_success(self, operation: &str) -> Result<Self> {
        if self.status.success() {
            Ok(self)
        } else {
            Err(host(format!(
                "{operation} failed with {}; inspect command logs",
                self.status
            )))
        }
    }
}

pub struct ReferenceSession {
    config: ReferenceConfig,
    state: LifecycleState,
    adb_server: Option<ManagedProcess>,
    emulator: Option<ManagedProcess>,
    clock: Instant,
    guest_started: Option<Instant>,
    events: File,
    command_index: u64,
    application: Option<String>,
    shutdown_info: Option<ShutdownInfo>,
}

impl ReferenceSession {
    pub fn start(mut config: ReferenceConfig) -> Result<Self> {
        config.validate()?;
        // Hold unused ports until immediately before launching their respective owner.
        let mut reserved = Vec::new();
        for port in [5039, 5580, 5581] {
            reserved.push(
                TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
                    .map_err(|e| Error::io("reserve private Android reference ports", e))?,
            );
        }
        fs::create_dir(&config.output_directory)
            .map_err(|e| Error::io("create reference capture directory", e))?;
        let events = create_file(&config.output_directory.join("events.txt"))?;
        let mut session = Self {
            config,
            state: LifecycleState::Starting,
            adb_server: None,
            emulator: None,
            clock: Instant::now(),
            guest_started: None,
            events,
            command_index: 0,
            application: None,
            shutdown_info: None,
        };
        let result = (|| {
            session.event("validating external engine acceleration")?;
            let mut version = session.tool("emulator/emulator.exe");
            version.arg("-version");
            session
                .run_command(&mut version, COMMAND_TIMEOUT)?
                .require_success("emulator version")?;
            let mut check = session.tool("emulator/emulator.exe");
            check.arg("-accel-check");
            let acceleration = session
                .run_command(&mut check, COMMAND_TIMEOUT)?
                .require_success("acceleration check")?;
            if !acceleration.stdout.contains("WHPX")
                || !acceleration.stdout.contains("installed and usable")
            {
                return Err(host("external emulator did not confirm usable WHPX"));
            }
            let config_text = read_bounded(
                &session
                    .config
                    .workspace_root
                    .join(".tools/android/avd/EmuCoreReference35.avd/config.ini"),
            )?;
            write_new(
                &session.config.output_directory.join("avd-config.ini"),
                &config_text,
            )?;
            drop(reserved.remove(0));
            let mut server = session.tool("platform-tools/adb.exe");
            server.args(["-L", "tcp:5039", "nodaemon", "server"]);
            session.adb_server = Some(session.spawn_logged(&mut server, "adb-server")?);
            let started = Instant::now();
            loop {
                if session.adb_server.as_mut().unwrap().try_wait()?.is_some() {
                    return Err(host("private adb server exited during startup"));
                }
                if TcpStream::connect_timeout(
                    &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5039).into(),
                    Duration::from_millis(100),
                )
                .is_ok()
                {
                    break;
                }
                if started.elapsed() >= COMMAND_TIMEOUT {
                    return Err(host("private adb server startup timed out"));
                }
                thread::sleep(Duration::from_millis(100));
            }
            drop(reserved);
            let mut emulator = session.tool("emulator/emulator.exe");
            emulator.args([
                "-avd",
                "EmuCoreReference35",
                "-port",
                "5580",
                "-accel",
                "on",
                "-gpu",
                "host",
                "-memory",
                "2048",
                "-cores",
                "2",
                "-no-window",
                "-no-audio",
                "-no-snapshot",
                "-no-boot-anim",
                "-no-metrics",
                "-camera-back",
                "none",
                "-camera-front",
                "none",
            ]);
            session.guest_started = Some(Instant::now());
            session.emulator = Some(session.spawn_logged(&mut emulator, "emulator")?);
            session.event("external Android process started; waiting for readiness")
        })();
        if let Err(error) = result {
            session.fail(&error);
            return Err(error);
        }
        Ok(session)
    }

    pub fn state(&self) -> LifecycleState {
        self.state
    }
    pub fn output_directory(&self) -> &Path {
        &self.config.output_directory
    }

    fn tool(&self, relative: &str) -> Command {
        let mut command = Command::new(self.config.sdk_root.join(relative));
        let state = self.config.workspace_root.join(".tools/android");
        command
            .env("ANDROID_AVD_HOME", state.join("avd"))
            .env("ANDROID_USER_HOME", state.join("user"))
            .env("ANDROID_EMULATOR_HOME", state.join("user"))
            .env("ANDROID_ADB_SERVER_PORT", "5039")
            .env("ADB_SERVER_SOCKET", "tcp:127.0.0.1:5039");
        command
    }

    fn event(&mut self, message: &str) -> Result<()> {
        writeln!(
            self.events,
            "{:.3}ms {:?}: {message}",
            self.clock.elapsed().as_secs_f64() * 1000.0,
            self.state
        )
        .and_then(|_| self.events.flush())
        .map_err(|e| Error::io("write lifecycle event", e))
    }

    fn spawn_logged(&self, command: &mut Command, name: &str) -> Result<ManagedProcess> {
        let stdout = create_file(
            &self
                .config
                .output_directory
                .join(format!("{name}.stdout.log")),
        )?;
        let stderr = create_file(
            &self
                .config
                .output_directory
                .join(format!("{name}.stderr.log")),
        )?;
        command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
        ManagedProcess::spawn(command)
    }

    fn run_command(&mut self, command: &mut Command, timeout: Duration) -> Result<CommandOutput> {
        let name = format!("command-{:04}", self.command_index);
        self.command_index = self
            .command_index
            .checked_add(1)
            .ok_or_else(|| host("command index overflow"))?;
        self.event(&format!("{name}: {command:?}"))?;
        let mut process = self.spawn_logged(command, &name)?;
        let Some(status) = process.wait_for(timeout)? else {
            process.terminate()?;
            return Err(host(format!(
                "{name} exceeded {timeout:?}; inspect its retained logs"
            )));
        };
        // End the complete command job before reading its files, even if a helper outlived the root.
        drop(process);
        Ok(CommandOutput {
            status,
            stdout: read_bounded(
                &self
                    .config
                    .output_directory
                    .join(format!("{name}.stdout.log")),
            )?,
            stderr: read_bounded(
                &self
                    .config
                    .output_directory
                    .join(format!("{name}.stderr.log")),
            )?,
        })
    }

    fn adb(&mut self, arguments: &[&str], timeout: Duration) -> Result<CommandOutput> {
        let mut command = self.tool("platform-tools/adb.exe");
        command
            .args(["-P", "5039", "-s", "emulator-5580"])
            .args(arguments);
        self.run_command(&mut command, timeout)
    }

    fn fail(&mut self, error: &Error) {
        self.state = LifecycleState::Failed;
        let _ = self.event(&error.to_string());
        self.emulator.take();
        self.adb_server.take();
    }

    /// Releases both private process trees immediately, without a graceful guest request.
    pub fn cancel(&mut self) {
        self.emulator.take();
        self.adb_server.take();
        self.state = LifecycleState::Stopped;
        let _ = self.event("reference cancelled; owned jobs released");
    }

    pub fn wait_ready(&mut self) -> Result<ReadyInfo> {
        if self.state != LifecycleState::Starting {
            return Err(usage("reference is not starting"));
        }
        let result = self.wait_ready_inner();
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }

    fn wait_ready_inner(&mut self) -> Result<ReadyInfo> {
        let started = self
            .guest_started
            .ok_or_else(|| host("missing guest start time"))?;
        let boot_completed = loop {
            if self
                .emulator
                .as_mut()
                .ok_or_else(|| host("missing emulator process"))?
                .try_wait()?
                .is_some()
            {
                return Err(host("external emulator exited before Android readiness"));
            }
            if started.elapsed() >= self.config.boot_timeout {
                return Err(host("Android boot readiness deadline exceeded"));
            }
            let boot = self.adb(&["shell", "getprop", "sys.boot_completed"], COMMAND_TIMEOUT)?;
            if boot.status.success() && boot.stdout.trim() == "1" {
                break started.elapsed();
            }
            thread::sleep(Duration::from_millis(500));
        };
        let package = self
            .adb(&["shell", "pm", "path", "android"], COMMAND_TIMEOUT)?
            .require_success("package readiness")?;
        if !package.stdout.trim_start().starts_with("package:") {
            return Err(host(
                "package service did not return the Android package path",
            ));
        }
        let package_ready = started.elapsed();
        let build_fingerprint = self.get_property("ro.build.fingerprint")?;
        let abis = self
            .get_property("ro.product.cpu.abilist")?
            .split(',')
            .filter(|abi| !abi.is_empty())
            .map(str::to_owned)
            .collect();
        let native_bridge = self.get_property("ro.dalvik.vm.native.bridge")?;
        let meminfo = self
            .adb(&["shell", "cat", "/proc/meminfo"], COMMAND_TIMEOUT)?
            .require_success("guest memory")?;
        let guest_memtotal_kib = meminfo
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| host("guest memory total was not reported"))?;
        let hardware = read_bounded(
            &self
                .config
                .workspace_root
                .join(".tools/android/avd/EmuCoreReference35.avd/hardware-qemu.ini"),
        )?;
        write_new(
            &self.config.output_directory.join("hardware-qemu.ini"),
            &hardware,
        )?;
        let configured_guest_memory_mb = property(&hardware, "hw.ramSize")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| host("effective guest RAM was not reported"))?;
        let graphics = self
            .adb(&["shell", "dumpsys", "SurfaceFlinger"], COMMAND_TIMEOUT)?
            .require_success("graphics diagnostics")?;
        let renderer = graphics
            .stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix("GLES:"))
            .unwrap_or("not reported")
            .trim()
            .to_owned();
        write_new(
            &self.config.output_directory.join("surfaceflinger.txt"),
            &graphics.stdout,
        )?;
        let memory_peaks = self.memory_peaks()?;
        self.state = LifecycleState::AndroidReady;
        self.event("Android boot and package readiness observed")?;
        Ok(ReadyInfo {
            boot_completed,
            package_ready,
            build_fingerprint,
            abis,
            native_bridge,
            renderer,
            configured_guest_memory_mb,
            guest_memtotal_kib,
            memory_peaks,
        })
    }

    fn get_property(&mut self, name: &str) -> Result<String> {
        let output = self
            .adb(&["shell", "getprop", name], COMMAND_TIMEOUT)?
            .require_success("guest property")?;
        Ok(output.stdout.trim().to_owned())
    }

    pub fn memory_peaks(&self) -> Result<JobMemory> {
        self.emulator
            .as_ref()
            .ok_or_else(|| usage("reference emulator is not running"))?
            .memory_peaks()
    }

    pub fn launch_activity(&mut self, component: &str) -> Result<()> {
        if !valid_component(component) {
            return Err(usage("invalid Android package/activity component"));
        }
        if !matches!(
            self.state,
            LifecycleState::AndroidReady | LifecycleState::ApplicationRunning
        ) {
            return Err(usage(
                "Android must be ready before launching an application",
            ));
        }
        if self.application.as_deref() == Some(component) {
            return Ok(());
        }
        self.state = LifecycleState::LaunchingApplication;
        let result = (|| {
            self.event(&format!("launching {component}"))?;
            let launch = self
                .adb(
                    &["shell", "am", "start", "-W", "-n", component],
                    Duration::from_secs(15),
                )?
                .require_success("activity launch")?;
            if !launch
                .stdout
                .lines()
                .any(|line| line.trim() == "Status: ok")
                || launch.stderr.contains("Error:")
            {
                return Err(host("Android did not confirm activity launch"));
            }
            self.application = Some(component.to_owned());
            self.state = LifecycleState::ApplicationRunning;
            self.event("application launch acknowledged")
        })();
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }

    pub fn stop(&mut self) -> Result<ShutdownInfo> {
        if let Some(info) = self.shutdown_info {
            return Ok(info);
        }
        if self.emulator.is_none() {
            return Err(usage("reference is not running"));
        }
        self.state = LifecycleState::Stopping;
        let result = (|| {
            self.event("requesting graceful emulator shutdown")?;
            self.adb(&["emu", "kill"], COMMAND_TIMEOUT)?
                .require_success("emulator shutdown request")?;
            if self
                .emulator
                .as_mut()
                .unwrap()
                .wait_for(Duration::from_secs(10))?
                .is_none()
            {
                return Err(host("emulator did not exit within the shutdown deadline"));
            }
            Ok(ShutdownInfo {
                graceful: true,
                memory_peaks: self.memory_peaks()?,
            })
        })();
        self.emulator.take();
        self.adb_server.take();
        match result {
            Ok(info) => {
                self.state = LifecycleState::Stopped;
                self.shutdown_info = Some(info);
                self.event("owned jobs released after graceful shutdown")?;
                Ok(info)
            }
            Err(error) => {
                self.fail(&error);
                Err(error)
            }
        }
    }
}

impl Drop for ReferenceSession {
    fn drop(&mut self) {
        self.emulator.take();
        self.adb_server.take();
    }
}

fn valid_component(component: &str) -> bool {
    let Some((package, activity)) = component.split_once('/') else {
        return false;
    };
    if package.split('.').count() < 2 {
        return false;
    }
    let valid_name = |name: &str| {
        name.split('.').all(|part| {
            let mut bytes = part.bytes();
            matches!(bytes.next(), Some(first) if first.is_ascii_alphabetic() || first == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    };
    valid_name(package) && valid_name(activity.strip_prefix('.').unwrap_or(activity))
}

fn property<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(name, value)| (name.trim() == key).then_some(value.trim()))
}

fn create_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Error::io("create new reference log", e))
}

fn write_new(path: &Path, text: &str) -> Result<()> {
    create_file(path)?
        .write_all(text.as_bytes())
        .map_err(|e| Error::io("write reference evidence", e))
}

fn read_bounded(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_COMMAND_OUTPUT + 1).read_to_end(&mut bytes))
        .map_err(|e| Error::io("read bounded reference evidence", e))?;
    if bytes.len() as u64 > MAX_COMMAND_OUTPUT {
        return Err(host("reference command output exceeded 256 KiB"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn usage(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Usage, message)
}
fn host(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Host, message)
}
