//! Live restart regression: install once, then verify native code and touch without reinstalling.
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use emucore::android_reference::{FixtureAbi, ReferenceConfig, ReferenceSession};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        path::PathBuf,
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: android_persistence SDK_ROOT NEW_CAPTURE_DIRECTORY".into());
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sdk = PathBuf::from(&args[0]);
    let output = workspace.join(&args[1]);
    fs::create_dir(&output)?;
    let mut report = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("result.txt"))?;
    let apk = workspace.join("artifacts/android-fixture/probe-x86_64.apk");
    let result = (|| -> emucore::Result<()> {
        let mut first = ReferenceSession::start(ReferenceConfig::new(
            sdk.clone(),
            workspace.clone(),
            output.join("installation"),
        ))?;
        let ready = first.wait_ready()?;
        let probe = first.verify_fixture(&apk, FixtureAbi::X86_64)?;
        let stopped = first.stop()?;
        writeln!(
            report,
            "Install boot: {:?}; native {} / {}; touches {}; graceful stop {}.",
            ready.package_ready,
            probe.startup_checksum,
            probe.touch_checksum,
            probe.touches,
            stopped.graceful
        )
        .map_err(|e| emucore::Error::io("write persistence report", e))?;
        drop(first);
        let mut second =
            ReferenceSession::start(ReferenceConfig::new(sdk, workspace, output.join("restart")))?;
        let ready = second.wait_ready()?;
        let probe = second.verify_installed_fixture(FixtureAbi::X86_64)?;
        let stopped = second.stop()?;
        writeln!(
            report,
            "Restart without install: {:?}; native {} / {}; touches {}; graceful stop {}.",
            ready.package_ready,
            probe.startup_checksum,
            probe.touch_checksum,
            probe.touches,
            stopped.graceful
        )
        .map_err(|e| emucore::Error::io("write persistence report", e))?;
        Ok(())
    })();
    match &result {
        Ok(()) => writeln!(
            report,
            "PASS: owned native code and actual UI touch survived shutdown and cold restart without reinstall."
        )?,
        Err(error) => writeln!(report, "FAIL: {error}")?,
    }
    report.sync_all()?;
    result?;
    println!(
        "Owned fixture restart persistence passed. Evidence: {}",
        output.display()
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This persistence regression requires Windows.");
}
