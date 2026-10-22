//! Explicit live cancellation probe for the prepared Windows Android reference.
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use emucore::{
        ErrorKind,
        android_reference::{LifecycleState, ReferenceConfig, ReferenceSession},
    };
    use std::{fs::OpenOptions, io::Write, path::PathBuf};
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err("expected <SDK root> <new capture directory>".into());
    }
    let config = ReferenceConfig::new(
        PathBuf::from(&arguments[0]),
        std::env::current_dir()?,
        PathBuf::from(&arguments[1]),
    );
    let cancellation = config.cancellation.clone();
    let mut session = ReferenceSession::start(config)?;
    // Start returned only after owning both the adb server and emulator jobs.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let memory_before_cancel = session.memory_peaks()?;
    cancellation.cancel();
    let error = session
        .wait_ready()
        .expect_err("cancelled launch unexpectedly became ready");
    if error.kind != ErrorKind::Cancelled || session.state() != LifecycleState::Cancelled {
        return Err(format!(
            "unexpected cancellation result: {error}, {:?}",
            session.state()
        )
        .into());
    }
    let report = session.output_directory().join("result.txt");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report)?;
    writeln!(
        file,
        "State: {:?}\nResult: {error}\nMemory before cancellation: {memory_before_cancel:#?}\nOwned jobs released before returning.",
        session.state()
    )?;
    println!(
        "Live Android startup cancellation verified; evidence: {}",
        report.display()
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The external Android reference currently requires Windows.");
    std::process::exit(2);
}
