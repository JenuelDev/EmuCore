//! Safe desktop lifecycle model. Construction and library access never start an engine.
#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    android_reference::{CancellationToken, ReferenceConfig, ReferenceSession},
    game_library::{GameEntry, GameLibrary, RuntimeProfile},
    launch_gate::{LaunchGate, LaunchTicket, LibraryLaunchState},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

trait Engine: Send {
    fn launch(&mut self, component: &str, token: CancellationToken) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn exited(&mut self) -> Result<bool>;
}

trait Factory: Send {
    fn start(
        &mut self,
        profile: &RuntimeProfile,
        token: CancellationToken,
    ) -> Result<Box<dyn Engine>>;
}

struct ReferenceFactory {
    workspace: PathBuf,
    visible: bool,
}

impl Factory for ReferenceFactory {
    fn start(
        &mut self,
        profile: &RuntimeProfile,
        token: CancellationToken,
    ) -> Result<Box<dyn Engine>> {
        let parent = self.workspace.join(".tools/client/runs");
        std::fs::create_dir_all(&parent)
            .map_err(|e| Error::io("create client run directory", e))?;
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| host("system clock precedes Unix epoch"))?
            .as_millis();
        let capture = parent.join(format!(
            "{epoch}-{}-{}",
            std::process::id(),
            NEXT_RUN.fetch_add(1, Ordering::Relaxed)
        ));
        let mut config = ReferenceConfig::new(
            profile.sdk_root.clone(),
            self.workspace.clone(),
            capture.clone(),
        );
        config.cancellation = token;
        config.visible = self.visible;
        let mut session =
            ReferenceSession::start(config).map_err(|error| with_capture(error, &capture))?;
        session
            .wait_ready()
            .map_err(|error| with_capture(error, &capture))?;
        Ok(Box::new(session))
    }
}

impl Engine for ReferenceSession {
    fn launch(&mut self, component: &str, token: CancellationToken) -> Result<()> {
        self.set_cancellation(token)?;
        self.launch_activity(component)
            .map_err(|error| with_capture(error, self.output_directory()))
    }
    fn stop(&mut self) -> Result<()> {
        ReferenceSession::stop(self).map(|_| ())
    }
    fn exited(&mut self) -> Result<bool> {
        self.poll_exited()
    }
}

enum Request {
    Launch {
        ticket: LaunchTicket,
        component: String,
        token: CancellationToken,
    },
    Stop,
}

enum Event {
    Ready(LaunchTicket),
    Finished(LaunchTicket, Result<()>),
    Stopped(Result<()>),
    Exited(Result<()>),
}

struct Worker {
    requests: SyncSender<Request>,
    events: Receiver<Event>,
    join: JoinHandle<()>,
}

/// UI thread model. Polling drains messages only, without waiting on SDK processes.
pub struct ClientController {
    gate: LaunchGate,
    factory: Option<Box<dyn Factory>>,
    worker: Option<Worker>,
    pending_token: Option<CancellationToken>,
    message: String,
}

impl ClientController {
    pub fn new(workspace: PathBuf) -> Self {
        Self::with_factory(Box::new(ReferenceFactory {
            workspace,
            visible: true,
        }))
    }

    /// For reproducible owned-fixture captures without a guest window.
    pub fn headless_reference(workspace: PathBuf) -> Self {
        Self::with_factory(Box::new(ReferenceFactory {
            workspace,
            visible: false,
        }))
    }

    fn with_factory(factory: Box<dyn Factory>) -> Self {
        Self {
            gate: LaunchGate::default(),
            factory: Some(factory),
            worker: None,
            pending_token: None,
            message: "Choose an installed game to start Android.".into(),
        }
    }

    pub fn state(&self) -> LibraryLaunchState {
        self.gate.state()
    }
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn launch(&mut self, library: &GameLibrary, entry_id: &str) -> Result<()> {
        library.validate()?;
        let entry = library
            .games
            .iter()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| host("selected library entry no longer exists"))?;
        let profile = library
            .profile_for(entry)
            .ok_or_else(|| host("runtime profile is missing"))?;
        let ticket = self.gate.request(entry, profile)?;
        let token = CancellationToken::default();
        let result = self.send_launch(&ticket, entry, token.clone());
        if let Err(error) = result {
            self.gate.finish(&ticket, false);
            self.message = error.to_string();
            return Err(error);
        }
        self.pending_token = Some(token);
        self.message = if ticket.reuse_instance() {
            "Launching game in the running emulator..."
        } else {
            "Starting Android..."
        }
        .into();
        Ok(())
    }

    fn send_launch(
        &mut self,
        ticket: &LaunchTicket,
        entry: &GameEntry,
        token: CancellationToken,
    ) -> Result<()> {
        if self.worker.is_none() {
            let factory = self
                .factory
                .take()
                .ok_or_else(|| host("client worker is unavailable; reopen the client"))?;
            let (requests, input) = mpsc::sync_channel(2);
            let (output, events) = mpsc::channel();
            let join = thread::Builder::new()
                .name("emucore-engine".into())
                .spawn(move || worker_loop(factory, input, output))
                .map_err(|e| Error::io("start engine worker", e))?;
            self.worker = Some(Worker {
                requests,
                events,
                join,
            });
        }
        self.worker
            .as_ref()
            .unwrap()
            .requests
            .try_send(Request::Launch {
                ticket: ticket.clone(),
                component: entry.component.clone(),
                token,
            })
            .map_err(|_| host("engine worker is busy or unavailable"))
    }

    pub fn stop(&mut self) -> Result<()> {
        if matches!(
            self.state(),
            LibraryLaunchState::Idle | LibraryLaunchState::Stopping
        ) {
            return Ok(());
        }
        if self.gate.begin_stop()
            && let Some(token) = self.pending_token.take()
        {
            token.cancel();
        }
        self.message = "Stopping emulator...".into();
        self.worker
            .as_ref()
            .ok_or_else(|| host("engine worker is missing"))?
            .requests
            .try_send(Request::Stop)
            .map_err(|_| host("engine worker is unavailable"))
    }

    /// Call regularly from the UI thread; stale launch replies cannot revive a stopped session.
    pub fn poll(&mut self) {
        loop {
            let event = self.worker.as_ref().map(|worker| worker.events.try_recv());
            match event {
                Some(Ok(Event::Ready(ticket))) => {
                    if matches!(self.state(), LibraryLaunchState::Starting { .. })
                        && self.gate.pending_is(&ticket)
                    {
                        self.message = "Android ready. Launching game...".into();
                    }
                }
                Some(Ok(Event::Finished(ticket, result))) => {
                    if self.gate.finish(&ticket, result.is_ok()) {
                        self.pending_token = None;
                        self.message = match result {
                            Ok(()) => "Game is running.".into(),
                            Err(e) => format!("Launch failed. Select the game to retry. {e}"),
                        };
                    }
                }
                Some(Ok(Event::Stopped(result) | Event::Exited(result))) => {
                    // An idle-exit notice can be queued just before a new launch.
                    // The worker will start a replacement; keep its newer ticket.
                    if matches!(self.state(), LibraryLaunchState::Starting { .. }) {
                        continue;
                    }
                    self.gate.finish_stop();
                    self.pending_token = None;
                    self.message = match result {
                        Ok(()) => "Emulator stopped. Choose a game to start again.".into(),
                        Err(e) => format!("Emulator stopped: {e}"),
                    };
                }
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    self.gate.finish_stop();
                    self.pending_token = None;
                    self.message = "Engine worker exited. Close and reopen the client.".into();
                    break;
                }
                _ => break,
            }
        }
    }
}

impl Drop for ClientController {
    fn drop(&mut self) {
        if let Some(token) = self.pending_token.take() {
            token.cancel();
        }
        if let Some(Worker { requests, join, .. }) = self.worker.take() {
            // Disconnect after queued work. The worker attempts bounded graceful
            // stop for a running session before dropping its owned process jobs.
            drop(requests);
            let _ = join.join();
        }
    }
}

fn worker_loop(
    mut factory: Box<dyn Factory>,
    input: Receiver<Request>,
    output: mpsc::Sender<Event>,
) {
    let mut engine: Option<Box<dyn Engine>> = None;
    loop {
        match input.recv_timeout(Duration::from_millis(250)) {
            Ok(Request::Launch {
                ticket,
                component,
                token,
            }) => {
                let result = (|| {
                    if token.is_cancelled() {
                        return Err(host("launch cancelled before engine startup"));
                    }
                    if !ticket.reuse_instance() {
                        engine.take();
                    }
                    if engine.is_none() {
                        engine = Some(factory.start(ticket.profile(), token.clone())?);
                    }
                    let _ = output.send(Event::Ready(ticket.clone()));
                    engine.as_mut().unwrap().launch(&component, token.clone())?;
                    if token.is_cancelled() {
                        return Err(host("launch cancelled"));
                    }
                    Ok(())
                })();
                if result.is_err() {
                    engine.take();
                }
                if output.send(Event::Finished(ticket, result)).is_err() {
                    break;
                }
            }
            Ok(Request::Stop) => {
                let result = engine.take().map_or(Ok(()), |mut engine| engine.stop());
                if output.send(Event::Stopped(result)).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(active) = engine.as_mut() {
                    let result = active.exited();
                    if !matches!(result, Ok(false)) {
                        engine.take();
                        if output.send(Event::Exited(result.map(|_| ()))).is_err() {
                            break;
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // Normal client exit must flush guest storage too. Dropping a running engine
    // directly would bypass ReferenceSession::stop and risk losing app data.
    // A failed/timeout stop still drops the owner and releases its process tree.
    if let Some(mut engine) = engine {
        let _ = engine.stop();
    }
}

fn host(message: &str) -> Error {
    Error::new(ErrorKind::Host, message)
}

fn with_capture(error: Error, capture: &std::path::Path) -> Error {
    Error::new(
        error.kind,
        format!("{error}; capture: {}", capture.display()),
    )
}

/// Runs the Windows prototype UI with a validated, inert library load.
pub fn run_desktop(workspace: PathBuf, library_path: PathBuf) -> Result<()> {
    crate::native::client::run(workspace, library_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_library::{Installation, RuntimeEngine};
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::Instant;

    #[derive(Default)]
    struct Counts {
        starts: AtomicU64,
        launches: AtomicU64,
        drops: AtomicU64,
        stops: AtomicU64,
        block: AtomicBool,
        fail_once: AtomicBool,
        fail_stop: AtomicBool,
        exited: AtomicBool,
    }
    struct FakeFactory(Arc<Counts>);
    struct FakeEngine(Arc<Counts>);
    impl Factory for FakeFactory {
        fn start(
            &mut self,
            _: &RuntimeProfile,
            token: CancellationToken,
        ) -> Result<Box<dyn Engine>> {
            self.0.starts.fetch_add(1, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.0.block.load(Ordering::SeqCst) {
                if token.is_cancelled() {
                    return Err(host("cancelled fake startup"));
                }
                assert!(
                    Instant::now() < deadline,
                    "test did not cancel or release startup"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Box::new(FakeEngine(self.0.clone())))
        }
    }
    impl Engine for FakeEngine {
        fn launch(&mut self, _: &str, _: CancellationToken) -> Result<()> {
            self.0.launches.fetch_add(1, Ordering::SeqCst);
            if self.0.fail_once.swap(false, Ordering::SeqCst) {
                Err(host("test activity failure"))
            } else {
                Ok(())
            }
        }
        fn stop(&mut self) -> Result<()> {
            self.0.stops.fetch_add(1, Ordering::SeqCst);
            if self.0.fail_stop.swap(false, Ordering::SeqCst) {
                Err(host("test stop failure"))
            } else {
                Ok(())
            }
        }
        fn exited(&mut self) -> Result<bool> {
            Ok(self.0.exited.swap(false, Ordering::SeqCst))
        }
    }
    impl Drop for FakeEngine {
        fn drop(&mut self) {
            self.0.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn library() -> GameLibrary {
        GameLibrary {
            runtime_profiles: vec![RuntimeProfile {
                id: "ref".into(),
                name: "Reference".into(),
                engine: RuntimeEngine::AndroidSdkReference,
                sdk_root: "not-accessed-sdk".into(),
            }],
            games: vec![GameEntry {
                id: "probe".into(),
                title: "Owned probe".into(),
                component: "dev.emucore.probe/.ProbeActivity".into(),
                runtime_profile_id: "ref".into(),
                installation: Installation::Installed {
                    verified_at_unix_ms: 1,
                },
                icon_path: None,
                fixture: None,
            }],
            ..GameLibrary::default()
        }
    }
    fn setup() -> (ClientController, Arc<Counts>) {
        let counts = Arc::new(Counts::default());
        (
            ClientController::with_factory(Box::new(FakeFactory(counts.clone()))),
            counts,
        )
    }
    fn until(client: &mut ClientController, predicate: impl Fn(&ClientController) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            client.poll();
            if predicate(client) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timed out: {:?}, {}",
                client.state(),
                client.message()
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn running(client: &mut ClientController) {
        until(client, |client| {
            matches!(client.state(), LibraryLaunchState::Running { .. })
        });
    }

    #[test]
    fn construction_library_validation_and_polling_do_not_start_worker_or_engine() {
        let (mut client, counts) = setup();
        library().validate().unwrap();
        for _ in 0..10 {
            client.poll();
        }
        assert!(client.worker.is_none());
        assert_eq!(counts.starts.load(Ordering::SeqCst), 0);
        assert_eq!(client.state(), LibraryLaunchState::Idle);
    }

    #[test]
    fn asynchronous_launch_rejects_duplicates_and_reuses_the_compatible_instance() {
        let (mut client, counts) = setup();
        counts.block.store(true, Ordering::SeqCst);
        client.launch(&library(), "probe").unwrap();
        assert!(matches!(
            client.state(),
            LibraryLaunchState::Starting { .. }
        ));
        assert!(client.launch(&library(), "probe").is_err());
        until(&mut client, |_| counts.starts.load(Ordering::SeqCst) == 1);
        counts.block.store(false, Ordering::SeqCst);
        running(&mut client);
        let mut changed = library();
        changed.runtime_profiles[0].sdk_root = "changed-sdk".into();
        assert!(client.launch(&changed, "probe").is_err());
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
        assert_eq!(counts.launches.load(Ordering::SeqCst), 2);
        client.stop().unwrap();
        until(&mut client, |client| {
            client.state() == LibraryLaunchState::Idle
        });
        assert_eq!(counts.stops.load(Ordering::SeqCst), 1);
        assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancelling_blocked_start_does_not_revive_the_old_ticket_and_allows_retry() {
        let (mut client, counts) = setup();
        counts.block.store(true, Ordering::SeqCst);
        client.launch(&library(), "probe").unwrap();
        until(&mut client, |_| counts.starts.load(Ordering::SeqCst) == 1);
        client.stop().unwrap();
        assert!(client.launch(&library(), "probe").is_err());
        until(&mut client, |client| {
            client.state() == LibraryLaunchState::Idle
        });
        assert_eq!(counts.launches.load(Ordering::SeqCst), 0);
        counts.block.store(false, Ordering::SeqCst);
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn activity_failure_releases_instance_and_explicit_retry_creates_another() {
        let (mut client, counts) = setup();
        counts.fail_once.store(true, Ordering::SeqCst);
        client.launch(&library(), "probe").unwrap();
        until(&mut client, |client| {
            client.state() == LibraryLaunchState::Idle
        });
        assert!(client.message().contains("test activity failure"));
        assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn externally_closed_emulator_returns_to_idle_and_can_be_restarted() {
        let (mut client, counts) = setup();
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        counts.exited.store(true, Ordering::SeqCst);
        until(&mut client, |client| {
            client.state() == LibraryLaunchState::Idle
        });
        assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn dropping_client_cancels_pending_start_and_joins_worker() {
        let (mut client, counts) = setup();
        counts.block.store(true, Ordering::SeqCst);
        client.launch(&library(), "probe").unwrap();
        until(&mut client, |_| counts.starts.load(Ordering::SeqCst) == 1);
        drop(client);
        assert_eq!(counts.launches.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn closing_running_client_stops_engine_before_joining_worker() {
        let (mut client, counts) = setup();
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        assert_eq!(counts.stops.load(Ordering::SeqCst), 0);
        drop(client);
        assert_eq!(counts.stops.load(Ordering::SeqCst), 1);
        assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_stop_still_releases_engine_and_allows_another_launch() {
        let (mut client, counts) = setup();
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        counts.fail_stop.store(true, Ordering::SeqCst);
        client.stop().unwrap();
        until(&mut client, |client| {
            client.state() == LibraryLaunchState::Idle
        });
        assert!(client.message().contains("test stop failure"));
        assert_eq!(counts.stops.load(Ordering::SeqCst), 1);
        assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
        client.launch(&library(), "probe").unwrap();
        running(&mut client);
        drop(client);
        assert_eq!(counts.stops.load(Ordering::SeqCst), 2);
        assert_eq!(counts.drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn unavailable_game_never_creates_worker() {
        let (mut client, counts) = setup();
        let mut library = library();
        library.games[0].installation = Installation::NotInstalled;
        assert!(client.launch(&library, "probe").is_err());
        assert!(client.worker.is_none());
        assert_eq!(counts.starts.load(Ordering::SeqCst), 0);
    }
}
