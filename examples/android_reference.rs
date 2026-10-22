//! Explicit Windows SDK reference probe. Merely building this example starts nothing.
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use emucore::android_reference::{FixtureAbi, ReferenceConfig, ReferenceSession};
    use std::{fs::OpenOptions, io::Write, path::PathBuf};
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("expected <SDK root> <new capture directory> <x86_64|arm64-v8a>".into());
    }
    let abi = match arguments[2].to_str() {
        Some("x86_64") => FixtureAbi::X86_64,
        Some("arm64-v8a") => FixtureAbi::Arm64,
        _ => return Err("fixture ABI must be x86_64 or arm64-v8a".into()),
    };
    let workspace = std::env::current_dir()?;
    let apk = workspace.join(format!(
        "artifacts/android-fixture/probe-{}.apk",
        abi.name()
    ));
    let config = ReferenceConfig::new(
        PathBuf::from(&arguments[0]),
        workspace,
        PathBuf::from(&arguments[1]),
    );
    println!("Starting the explicitly requested Android reference...");
    let mut session = ReferenceSession::start(config)?;
    let ready = session.wait_ready()?;
    println!(
        "Android ready in {:.2} s; configured RAM {} MB",
        ready.package_ready.as_secs_f64(),
        ready.configured_guest_memory_mb
    );
    let fixture = session.verify_fixture(&apk, abi)?;
    println!("Owned {} native computation and touch verified", abi.name());
    let shutdown = session.stop()?;
    let report = session.output_directory().join("result.txt");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report)?;
    writeln!(
        file,
        "Ready:\n{ready:#?}\nFixture:\n{fixture:#?}\nShutdown:\n{shutdown:#?}"
    )?;
    println!("Reference stopped; evidence: {}", report.display());
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The external Android reference currently requires Windows.");
    std::process::exit(2);
}
