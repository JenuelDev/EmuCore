//! Exercise the persisted library and the same worker used by the desktop view.
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use emucore::{
        client::ClientController, game_library::LibraryStore, launch_gate::LibraryLaunchState,
    };
    use std::{
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: client_launch LIBRARY_PATH NEW_REPORT_PATH".into());
    }
    let report_path = PathBuf::from(&args[1]);
    let mut report = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(report_path)?;
    use std::io::Write;
    let library = LibraryStore::new(PathBuf::from(&args[0])).load()?;
    let entry = library
        .installed_games()
        .next()
        .ok_or("no installed entry")?;
    if entry.component != "dev.emucore.probe/.ProbeActivity" {
        return Err("this check launches only the owned fixture".into());
    }
    let mut client =
        ClientController::headless_reference(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    writeln!(
        report,
        "Loaded {} entries. Initial state {:?}. No engine requested.",
        library.games.len(),
        client.state()
    )?;
    let start = Instant::now();
    client.launch(&library, &entry.id)?;
    assert!(
        client.launch(&library, &entry.id).is_err(),
        "pending duplicate accepted"
    );
    wait(&mut client, true)?;
    writeln!(
        report,
        "Library-to-owned-app launch: {:?}. State {:?}.",
        start.elapsed(),
        client.state()
    )?;
    let reuse = Instant::now();
    client.launch(&library, &entry.id)?;
    wait(&mut client, true)?;
    writeln!(
        report,
        "Repeated launch through compatible instance: {:?}.",
        reuse.elapsed()
    )?;
    let close = Instant::now();
    drop(client);
    writeln!(
        report,
        "Running client closed and worker joined in {:?}.",
        close.elapsed()
    )?;
    let mut client =
        ClientController::headless_reference(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    client.launch(&library, &entry.id)?;
    wait(&mut client, true)?;
    writeln!(
        report,
        "Saved app launched after client close without reinstall: {:?}.",
        client.state()
    )?;
    client.stop()?;
    wait(&mut client, false)?;
    writeln!(report, "Stop: {:?}. {}", client.state(), client.message())?;
    client.launch(&library, &entry.id)?;
    thread::sleep(Duration::from_secs(2));
    client.stop()?;
    wait(&mut client, false)?;
    writeln!(
        report,
        "Cancel after two seconds: {:?}. {}",
        client.state(),
        client.message()
    )?;
    report.sync_all()?;
    println!(
        "Persisted-library launch, reuse, client close/restart, stop, and cancellation passed."
    );
    fn wait(
        client: &mut ClientController,
        running: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            client.poll();
            match client.state() {
                LibraryLaunchState::Running { .. } if running => return Ok(()),
                LibraryLaunchState::Idle if !running => {
                    if client.message().starts_with("Emulator stopped:") {
                        return Err(client.message().into());
                    }
                    return Ok(());
                }
                LibraryLaunchState::Idle if running => return Err(client.message().into()),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(format!("client wait timed out: {}", client.message()).into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This reference check requires Windows.");
}
