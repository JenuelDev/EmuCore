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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    Cancelled,
    Failed,
}

/// A per-launch cancellation signal that a responsive client can retain on another thread.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::new(
                ErrorKind::Cancelled,
                "Android reference launch cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

/// Configuration construction is inert. Only ReferenceSession::start launches processes.
#[derive(Clone, Debug)]
pub struct ReferenceConfig {
    pub sdk_root: PathBuf,
    pub workspace_root: PathBuf,
    pub output_directory: PathBuf,
    pub boot_timeout: Duration,
    pub cancellation: CancellationToken,
    /// Reference captures are headless by default; the desktop client opts in on launch.
    pub visible: bool,
}

impl ReferenceConfig {
    pub fn new(sdk_root: PathBuf, workspace_root: PathBuf, output_directory: PathBuf) -> Self {
        Self {
            sdk_root,
            workspace_root,
            output_directory,
            boot_timeout: Duration::from_secs(120),
            cancellation: CancellationToken::default(),
            visible: false,
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

#[derive(Clone, Copy, Debug)]
pub enum FixtureAbi {
    X86_64,
    Arm64,
}

impl FixtureAbi {
    pub fn name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64-v8a",
        }
    }
}

#[derive(Debug)]
pub struct FixtureResult {
    pub abi: FixtureAbi,
    pub startup_checksum: u64,
    pub touch_checksum: u64,
    pub touches: u32,
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
        config.cancellation.check()?;
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
                session.config.cancellation.check()?;
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
            session.config.cancellation.check()?;
            let mut emulator = session.tool("emulator/emulator.exe");
            emulator.args([
                "-avd",
                "EmuCoreReference35",
                "-verbose",
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
                "-no-audio",
                "-no-snapshot",
                "-no-boot-anim",
                "-no-metrics",
                "-camera-back",
                "none",
                "-camera-front",
                "none",
            ]);
            if !session.config.visible {
                emulator.arg("-no-window");
            }
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

    /// Replaces the signal between app requests on an already ready instance.
    pub fn set_cancellation(&mut self, token: CancellationToken) -> Result<()> {
        if !matches!(
            self.state,
            LifecycleState::AndroidReady | LifecycleState::ApplicationRunning
        ) {
            return Err(usage(
                "only a ready instance can accept another launch signal",
            ));
        }
        self.config.cancellation = token;
        Ok(())
    }

    /// Nonblocking observation for a client whose user can close the guest window.
    pub fn poll_exited(&mut self) -> Result<bool> {
        let Some(emulator) = &mut self.emulator else {
            return Ok(true);
        };
        match emulator.try_wait() {
            Ok(None) => Ok(false),
            Ok(Some(status)) => {
                self.emulator.take();
                self.adb_server.take();
                self.state = LifecycleState::Stopped;
                self.event(&format!(
                    "emulator exited with {status}; owned jobs released"
                ))?;
                Ok(true)
            }
            Err(error) => {
                self.fail(&error);
                Err(error)
            }
        }
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
        self.config.cancellation.check()?;
        let name = format!("command-{:04}", self.command_index);
        self.command_index = self
            .command_index
            .checked_add(1)
            .ok_or_else(|| host("command index overflow"))?;
        self.event(&format!("{name}: {command:?}"))?;
        let mut process = self.spawn_logged(command, &name)?;
        let started = Instant::now();
        let status = loop {
            self.config.cancellation.check()?;
            let remaining = timeout.saturating_sub(started.elapsed());
            if let Some(status) = process.wait_for(remaining.min(Duration::from_millis(100)))? {
                break status;
            }
            if remaining.is_zero() {
                process.terminate()?;
                return Err(host(format!(
                    "{name} exceeded {timeout:?}; inspect its retained logs"
                )));
            }
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
        self.state = if error.kind == ErrorKind::Cancelled {
            LifecycleState::Cancelled
        } else {
            LifecycleState::Failed
        };
        let _ = self.event(&error.to_string());
        self.emulator.take();
        self.adb_server.take();
    }

    /// Releases both private process trees immediately, without a graceful guest request.
    pub fn cancel(&mut self) {
        self.config.cancellation.cancel();
        self.emulator.take();
        self.adb_server.take();
        self.state = LifecycleState::Cancelled;
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
            self.config.cancellation.check()?;
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
        let result = (|| {
            if self.application.as_deref() == Some(component) {
                let activities = self
                    .adb(
                        &["shell", "dumpsys", "activity", "activities"],
                        COMMAND_TIMEOUT,
                    )?
                    .require_success("current activity")?;
                if activity_resumed(&activities.stdout, component) {
                    return Ok(());
                }
            }
            self.state = LifecycleState::LaunchingApplication;
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
            if error.kind != ErrorKind::Cancelled {
                let _ = self.adb(&["shell", "pm", "list", "packages", "-3"], COMMAND_TIMEOUT);
                let _ = self.adb(
                    &[
                        "logcat",
                        "-d",
                        "-v",
                        "brief",
                        "-s",
                        "PackageManager:V",
                        "PackageInstaller:V",
                        "installd:V",
                        "vold:V",
                        "*:S",
                    ],
                    COMMAND_TIMEOUT,
                );
            }
            self.fail(error);
        }
        result
    }

    /// Runs only the owned probe package. Its UI hierarchy is a restricted test
    /// contract, not a general Android UI automation parser.
    pub fn verify_fixture(&mut self, apk: &Path, abi: FixtureAbi) -> Result<FixtureResult> {
        if !matches!(
            self.state,
            LifecycleState::AndroidReady | LifecycleState::ApplicationRunning
        ) {
            return Err(usage("Android must be ready before the owned fixture"));
        }
        let result = self.verify_fixture_inner(apk, abi);
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }

    /// Verifies native execution and touch on the already installed owned probe.
    /// Never installs an APK; intended to detect lost code after a device restart.
    pub fn verify_installed_fixture(&mut self, abi: FixtureAbi) -> Result<FixtureResult> {
        if !matches!(
            self.state,
            LifecycleState::AndroidReady | LifecycleState::ApplicationRunning
        ) {
            return Err(usage("Android must be ready before the owned fixture"));
        }
        let result = self.exercise_fixture(abi);
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }

    fn verify_fixture_inner(&mut self, apk: &Path, abi: FixtureAbi) -> Result<FixtureResult> {
        let apk = fs::canonicalize(apk).map_err(|e| Error::io("resolve owned fixture APK", e))?;
        let mut metadata = self.tool("build-tools/35.0.0/aapt2.exe");
        metadata.args(["dump", "badging"]).arg(&apk);
        let package = self
            .run_command(&mut metadata, COMMAND_TIMEOUT)?
            .require_success("owned APK metadata")?;
        if !package
            .stdout
            .lines()
            .any(|line| line.starts_with("package: name='dev.emucore.probe' "))
            || !package
                .stdout
                .lines()
                .any(|line| line.trim() == format!("native-code: '{}'", abi.name()))
        {
            return Err(usage(
                "expected the owned probe package with exactly the selected ABI",
            ));
        }
        self.event(&format!(
            "installing owned {} fixture from {}",
            abi.name(),
            apk.display()
        ))?;
        let mut install = self.tool("platform-tools/adb.exe");
        install
            .args([
                "-P",
                "5039",
                "-s",
                "emulator-5580",
                "install",
                "--no-incremental",
                "-r",
            ])
            .arg(&apk);
        let installed = self
            .run_command(&mut install, Duration::from_secs(15))?
            .require_success("owned fixture install")?;
        if !installed
            .stdout
            .lines()
            .any(|line| line.trim() == "Success")
        {
            return Err(host("owned fixture install was not acknowledged"));
        }
        self.exercise_fixture(abi)
    }

    fn exercise_fixture(&mut self, abi: FixtureAbi) -> Result<FixtureResult> {
        self.event("checking existing owned probe; no installation in this step")?;
        self.adb(
            &["shell", "am", "force-stop", "dev.emucore.probe"],
            COMMAND_TIMEOUT,
        )?
        .require_success("stop earlier owned probe activity")?;
        self.adb(&["logcat", "-c"], COMMAND_TIMEOUT)?
            .require_success("clear owned guest log")?;
        // Force-stop and fresh logs ensure this check observes a new native startup.
        self.application = None;
        self.launch_activity("dev.emucore.probe/.ProbeActivity")?;
        let ready = format!("READY abi={} checksum=2206585887 touches=0", abi.name());
        self.wait_fixture_log(&ready, "fixture-ready.log")?;
        self.adb(
            &[
                "shell",
                "uiautomator",
                "dump",
                "/data/local/tmp/emucore-rust-ui.xml",
            ],
            Duration::from_secs(10),
        )?
        .require_success("owned fixture hierarchy")?;
        let hierarchy = self
            .adb(
                &["shell", "cat", "/data/local/tmp/emucore-rust-ui.xml"],
                COMMAND_TIMEOUT,
            )?
            .require_success("read owned fixture hierarchy")?;
        write_new(
            &self.config.output_directory.join("fixture-ui.xml"),
            &hierarchy.stdout,
        )?;
        let (x, y) = fixture_button_center(&hierarchy.stdout)?;
        self.adb(
            &["shell", "input", "tap", &x.to_string(), &y.to_string()],
            COMMAND_TIMEOUT,
        )?
        .require_success("owned fixture touch")?;
        let touched = format!("TOUCH abi={} checksum=2873771296 touches=1", abi.name());
        self.wait_fixture_log(&touched, "fixture-touch.log")?;
        thread::sleep(Duration::from_millis(500));
        self.adb(
            &[
                "shell",
                "screencap",
                "-p",
                "/data/local/tmp/emucore-rust-screen.png",
            ],
            COMMAND_TIMEOUT,
        )?
        .require_success("owned fixture screenshot")?;
        let screenshot = self.config.output_directory.join("screen.png");
        if screenshot.exists() {
            return Err(usage("reference screenshot already exists"));
        }
        let mut pull = self.tool("platform-tools/adb.exe");
        pull.args([
            "-P",
            "5039",
            "-s",
            "emulator-5580",
            "pull",
            "/data/local/tmp/emucore-rust-screen.png",
        ])
        .arg(&screenshot);
        self.run_command(&mut pull, COMMAND_TIMEOUT)?
            .require_success("owned fixture screenshot transfer")?;
        self.event(&format!(
            "owned {} native startup and UI touch checks passed",
            abi.name()
        ))?;
        Ok(FixtureResult {
            abi,
            startup_checksum: 2_206_585_887,
            touch_checksum: 2_873_771_296,
            touches: 1,
        })
    }

    fn wait_fixture_log(&mut self, expected: &str, filename: &str) -> Result<()> {
        let started = Instant::now();
        loop {
            let log = self
                .adb(
                    &["logcat", "-d", "-v", "brief", "-s", "EmuCoreProbe:I", "*:S"],
                    COMMAND_TIMEOUT,
                )?
                .require_success("read owned fixture log")?;
            if log
                .stdout
                .lines()
                .any(|line| line.ends_with(&format!(": {expected}")))
            {
                write_new(&self.config.output_directory.join(filename), &log.stdout)?;
                return Ok(());
            }
            if log.stdout.contains("FAIL") || started.elapsed() >= Duration::from_secs(5) {
                write_new(&self.config.output_directory.join(filename), &log.stdout)?;
                return Err(host(format!("owned fixture did not report {expected}")));
            }
            thread::sleep(Duration::from_millis(100));
        }
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
            self.event("flushing guest filesystem before emulator shutdown")?;
            self.adb(&["shell", "sync"], COMMAND_TIMEOUT)?
                .require_success("guest filesystem sync")?;
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

pub(crate) fn valid_component(component: &str) -> bool {
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

fn normalized_component(component: &str) -> Option<String> {
    if !valid_component(component) {
        return None;
    }
    let (package, activity) = component.split_once('/')?;
    Some(if activity.starts_with('.') {
        format!("{package}/{package}{activity}")
    } else {
        component.to_owned()
    })
}

fn activity_resumed(dump: &str, component: &str) -> bool {
    let Some(expected) = normalized_component(component) else {
        return false;
    };
    dump.lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with("topResumedActivity=") || line.starts_with("mResumedActivity=")
        })
        .any(|line| {
            line.split_whitespace().any(|word| {
                normalized_component(word.trim_end_matches(['}', ','])).as_deref()
                    == Some(expected.as_str())
            })
        })
}

fn fixture_button_center(hierarchy: &str) -> Result<(u32, u32)> {
    let mut buttons = hierarchy.split("<node ").filter(|node| {
        node.split('>')
            .next()
            .is_some_and(|tag| tag.contains("content-desc=\"Run native check\""))
    });
    let button = buttons
        .next()
        .ok_or_else(|| host("owned fixture button was not found"))?;
    if buttons.next().is_some() {
        return Err(host("owned fixture button is ambiguous"));
    }
    let bounds = button
        .split('>')
        .next()
        .and_then(|tag| tag.split("bounds=\"").nth(1))
        .and_then(|value| value.split('"').next())
        .ok_or_else(|| host("owned fixture button bounds missing"))?;
    let parts: Vec<_> = bounds
        .split(['[', ']', ','])
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 4 {
        return Err(host("invalid owned fixture button bounds"));
    }
    let values: Vec<u32> = parts
        .iter()
        .map(|part| part.parse())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| host("invalid owned fixture button coordinate"))?;
    if bounds != format!("[{},{}][{},{}]", values[0], values[1], values[2], values[3])
        || values[0] >= values[2]
        || values[1] >= values[3]
        || values[2] > 720
        || values[3] > 1280
    {
        return Err(host("owned fixture button exceeds expected display bounds"));
    }
    Ok(((values[0] + values[2]) / 2, (values[1] + values[3]) / 2))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "emucore-reference-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn prepared(&self) -> ReferenceConfig {
            let workspace = self.0.join("workspace");
            let sdk = self.0.join("sdk");
            for path in [
                workspace.join("artifacts"),
                workspace.join(".tools/android/avd/EmuCoreReference35.avd"),
                sdk.join("emulator"),
                sdk.join("platform-tools"),
                sdk.join("system-images/android-35/google_apis_playstore/x86_64"),
            ] {
                fs::create_dir_all(path).unwrap();
            }
            fs::write(sdk.join("emulator/emulator.exe"), b"not executable").unwrap();
            fs::write(sdk.join("platform-tools/adb.exe"), b"not executable").unwrap();
            fs::write(
                sdk.join("system-images/android-35/google_apis_playstore/x86_64/source.properties"),
                "Pkg.Revision=9\n",
            )
            .unwrap();
            fs::write(
                workspace.join(".tools/android/avd/EmuCoreReference35.avd/.emucore-reference"),
                MARKER,
            )
            .unwrap();
            ReferenceConfig::new(sdk, workspace, PathBuf::from("artifacts/capture"))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn components_allow_named_activities_and_reject_shell_metacharacters() {
        for value in [
            "dev.emucore.probe/.ProbeActivity",
            "dev.emucore.probe/dev.emucore.probe.ProbeActivity",
        ] {
            assert!(valid_component(value));
        }
        for value in [
            "",
            "single/.A",
            "dev.probe/",
            "dev..probe/.A",
            "dev.probe/.A;echo",
            "dev.probe/.A extra",
            "dev.probe/$(x)",
            "dev.probe/.A/other",
            "dev.probe/.A\n",
            "dev.probe/.A$Inner",
        ] {
            assert!(!valid_component(value), "accepted {value:?}");
        }
    }

    #[test]
    fn resumed_activity_matches_normalized_complete_component_only() {
        let short = "dev.emucore.probe/.ProbeActivity";
        let full = "dev.emucore.probe/dev.emucore.probe.ProbeActivity";
        let dump = format!(" topResumedActivity=ActivityRecord{{123 u0 {short} t9}}\n");
        assert!(activity_resumed(&dump, full));
        assert!(!activity_resumed(&dump, "dev.emucore.probe/.Probe"));
        assert!(!activity_resumed(
            &format!("mLastPausedActivity=ActivityRecord{{123 u0 {short} t9}}"),
            short
        ));
        assert!(!activity_resumed("topResumedActivity=null", short));
    }

    #[test]
    fn button_bounds_are_unique_positive_and_inside_the_known_display() {
        let node = |bounds: &str| {
            format!(
                "<hierarchy><node bounds=\"{bounds}\" content-desc=\"Run native check\" /></hierarchy>"
            )
        };
        assert_eq!(
            fixture_button_center(&node("[56,501][664,573]")).unwrap(),
            (360, 537)
        );
        for bounds in [
            "[-1,0][50,20]",
            "[0,0][721,1280]",
            "[0,0][720,1281]",
            "[50,10][20,30]",
            "[0,0][0,5]",
            "[1,2,3]",
            "[4294967295,0][1,1]",
            "[1,2] [3,4]",
        ] {
            assert!(
                fixture_button_center(&node(bounds)).is_err(),
                "accepted {bounds}"
            );
        }
        assert!(fixture_button_center("<hierarchy />").is_err());
        assert!(fixture_button_center(&(node("[0,0][10,10]") + &node("[20,20][30,30]"))).is_err());
    }

    #[test]
    fn property_parser_matches_whitespace_but_never_a_key_prefix() {
        let properties = "#comment\nhw.ramSize.extra = 1\nhw.ramSize = 2560\n";
        assert_eq!(property(properties, "hw.ramSize"), Some("2560"));
        assert_eq!(property(properties, "hw.cpu.ncore"), None);
    }

    #[test]
    fn validation_is_inert_and_preserves_existing_or_external_output() {
        let directory = Directory::new();
        let original = directory.prepared();
        let mut config = original.clone();
        config.validate().unwrap();
        assert!(!config.output_directory.exists());
        fs::create_dir(&config.output_directory).unwrap();
        assert_eq!(
            original.clone().validate().unwrap_err().kind,
            ErrorKind::Usage
        );
        let outside = directory.0.join("outside");
        fs::create_dir(&outside).unwrap();
        let mut external = original.clone();
        external.output_directory = outside.join("capture");
        assert_eq!(external.validate().unwrap_err().kind, ErrorKind::Usage);
        assert!(!outside.join("capture").exists());
        fs::write(
            original
                .workspace_root
                .join(".tools/android/avd/EmuCoreReference35.avd/.emucore-reference"),
            "not owned",
        )
        .unwrap();
        assert_eq!(
            original.clone().validate().unwrap_err().kind,
            ErrorKind::Usage
        );
    }

    #[test]
    fn cancellation_before_start_never_resolves_or_launches_an_sdk() {
        let config = ReferenceConfig::new(
            "missing-sdk".into(),
            "missing-workspace".into(),
            "capture".into(),
        );
        let token = config.cancellation.clone();
        assert!(!token.is_cancelled());
        token.cancel();
        let error = match ReferenceSession::start(config) {
            Ok(_) => panic!("cancelled launch started"),
            Err(error) => error,
        };
        assert_eq!(error.kind, ErrorKind::Cancelled);
    }

    #[test]
    fn bounded_evidence_rejects_oversized_tool_output() {
        let directory = Directory::new();
        let file = directory.0.join("large.log");
        fs::write(&file, vec![b'x'; MAX_COMMAND_OUTPUT as usize + 1]).unwrap();
        assert_eq!(read_bounded(&file).unwrap_err().kind, ErrorKind::Host);
    }

    #[test]
    fn cancellation_interrupts_a_running_owned_command() {
        let directory = Directory::new();
        let config = directory.prepared();
        fs::create_dir(config.workspace_root.join("artifacts/capture")).unwrap();
        let token = config.cancellation.clone();
        let marker = directory.0.join("command-ready");
        let observer_marker = marker.clone();
        let cancelled = thread::spawn(move || {
            let started = Instant::now();
            while !observer_marker.exists() && started.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(10));
            }
            token.cancel();
            assert!(observer_marker.exists(), "owned helper never started");
        });
        let output = config.workspace_root.join("artifacts/capture");
        let mut session = ReferenceSession {
            config: ReferenceConfig {
                output_directory: output.clone(),
                ..config
            },
            state: LifecycleState::Starting,
            adb_server: None,
            emulator: None,
            clock: Instant::now(),
            guest_started: None,
            events: create_file(&output.join("events.txt")).unwrap(),
            command_index: 0,
            application: None,
            shutdown_info: None,
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "native::process::tests::helper_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("EMUCORE_TEST_JOB_MODE", "loop")
            .env("EMUCORE_TEST_JOB_MARKER", marker);
        let result = session.run_command(&mut command, Duration::from_secs(30));
        cancelled.join().unwrap();
        match result {
            Ok(_) => panic!("cancelled command succeeded"),
            Err(error) => assert_eq!(error.kind, ErrorKind::Cancelled),
        }
    }
}
