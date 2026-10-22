#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| workspace.join(".tools/client/library.json"));
    let result = if args.next().is_some() {
        Err(emucore::Error::new(
            emucore::ErrorKind::Usage,
            "usage: emucore-client [library.json]",
        ))
    } else {
        emucore::client::run_desktop(workspace.clone(), path)
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // GUI-subsystem programs do not have a console. Preserve startup errors
            // without requiring another process or silently replacing the library.
            let parent = workspace.join(".tools/client");
            let _ = std::fs::create_dir_all(&parent);
            let _ = std::fs::write(parent.join("startup-error.txt"), format!("{error}\n"));
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The desktop prototype currently requires Windows.");
}
