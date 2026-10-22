//! Explicit preparation command: verify the owned APK before adding it to a new library.
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use emucore::{
        android_reference::{FixtureAbi, ReferenceConfig, ReferenceSession},
        game_library::{
            FixtureSpec, GameEntry, Installation, LibraryAbi, LibraryStore, RuntimeEngine,
            RuntimeProfile,
        },
    };
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: client_fixture SDK_ROOT NEW_LIBRARY_PATH".into());
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let library_path = PathBuf::from(&args[1]);
    if library_path.exists() {
        return Err("use a new library path; existing metadata is preserved".into());
    }
    let sdk = std::fs::canonicalize(PathBuf::from(&args[0]))?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let parent = workspace.join("artifacts/client-preparation");
    std::fs::create_dir_all(&parent)?;
    let capture = parent.join(format!("{stamp}-{}", std::process::id()));
    let apk = workspace.join("artifacts/android-fixture/probe-x86_64.apk");
    let mut session = ReferenceSession::start(ReferenceConfig::new(
        sdk.clone(),
        workspace,
        capture.clone(),
    ))?;
    let ready = session.wait_ready()?;
    let fixture = session.verify_fixture(&apk, FixtureAbi::X86_64)?;
    let shutdown = session.stop()?;
    let verified_at_unix_ms =
        u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    LibraryStore::new(library_path).update(|library| {
        if !library.runtime_profiles.is_empty() || !library.games.is_empty() {
            return Err(emucore::Error::new(
                emucore::ErrorKind::Usage,
                "library changed during fixture verification; metadata preserved",
            ));
        }
        library.runtime_profiles.push(RuntimeProfile {
            id: "sdk-reference".into(),
            name: "Android SDK reference".into(),
            engine: RuntimeEngine::AndroidSdkReference,
            sdk_root: sdk,
        });
        library.games.push(GameEntry {
            id: "native-probe".into(),
            title: "EmuCore native probe (test fixture)".into(),
            component: "dev.emucore.probe/.ProbeActivity".into(),
            runtime_profile_id: "sdk-reference".into(),
            installation: Installation::Installed {
                verified_at_unix_ms,
            },
            icon_path: None,
            fixture: Some(FixtureSpec {
                apk_path: apk,
                abi: LibraryAbi::X86_64,
            }),
        });
        Ok(())
    })?;
    let result = format!(
        "Verified owned x86_64 fixture before library insertion.\nReadiness: {:?}\nChecksums: {} / {}, touches: {}\nGraceful stop: {}\nCapture: {}\n",
        ready.package_ready,
        fixture.startup_checksum,
        fixture.touch_checksum,
        fixture.touches,
        shutdown.graceful,
        capture.display()
    );
    std::fs::write(capture.join("result.txt"), &result)?;
    print!("{result}");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This reference preparation command requires Windows.");
}
