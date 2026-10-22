use emucore::fixture;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "emucore-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(operation: &str, path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_emucore"))
        .arg(operation)
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn fixture_is_reproducible_and_existing_files_are_not_overwritten() {
    let directory = Directory::new();
    let path = directory.0.join("owned fixture.elf");
    assert!(command("fixture", &path).status.success());
    assert_eq!(fs::read(&path).unwrap(), fixture::hello());
    assert!(!command("fixture", &path).status.success());
    assert_eq!(fs::read(&path).unwrap(), fixture::hello());
    let inspection = command("inspect", &path);
    assert!(inspection.status.success());
    assert!(String::from_utf8_lossy(&inspection.stdout).contains("ELF64 x86-64 ET_EXEC"));
}

#[test]
fn invalid_input_fails_with_diagnostic() {
    let directory = Directory::new();
    let path = directory.0.join("invalid.elf");
    fs::write(&path, b"not an ELF").unwrap();
    let output = command("inspect", &path);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidElf"));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn forwards_guest_arguments_through_the_worker_without_splitting() {
    let directory = Directory::new();
    let path = directory.0.join("arguments fixture.elf");
    assert!(command("args-fixture", &path).status.success());
    for argument in ["", "Rust", "A B!", "🦀", "Variable-length argument"] {
        let output = Command::new(env!("CARGO_BIN_EXE_emucore"))
            .arg("run")
            .arg(&path)
            .arg("--")
            .arg(argument)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, argument.as_bytes());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_emucore"))
        .arg("run")
        .arg(&path)
        .arg("Rust")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn native_infinite_loop_reports_budget_exhaustion_before_worker_timeout() {
    let directory = Directory::new();
    let path = directory.0.join("bounded loop.elf");
    fs::write(&path, fixture::elf(&[0xeb, 0xfe], b"x")).unwrap();
    let output = command("run", &path);
    assert_eq!(output.status.code(), Some(125));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("execution budget exhausted after 100000")
    );
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn worker_executes_fixture_with_exact_output_and_exit_status() {
    let directory = Directory::new();
    let path = directory.0.join("owned fixture.elf");
    fs::write(&path, fixture::hello()).unwrap();
    let output = command("run", &path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, fixture::MESSAGE);
    assert!(output.stderr.is_empty());

    let code = [0xbf, 37, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05];
    fs::write(&path, fixture::elf(&code, b"x")).unwrap();
    assert_eq!(command("run", &path).status.code(), Some(37));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn profile_reports_metrics_instead_of_guest_output_and_preserves_exit_status() {
    let directory = Directory::new();
    let path = directory.0.join("profile fixture.elf");
    fs::write(&path, fixture::hello()).unwrap();
    let output = command("profile", &path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.starts_with("{\"schema_version\":1,\"backend\":\"native\","));
    assert!(report.contains(&format!("\"stdout_bytes\":{}", fixture::MESSAGE.len())));
    assert!(report.contains("\"exit_code\":0}"));
    assert!(!report.contains(std::str::from_utf8(fixture::MESSAGE).unwrap().trim()));
    let code = [0xbf, 37, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05];
    fs::write(&path, fixture::elf(&code, b"x")).unwrap();
    let output = command("profile", &path);
    assert_eq!(output.status.code(), Some(37));
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"exit_code\":37}"));
    fs::write(&path, fixture::elf(&[0xeb, 0xfe], b"x")).unwrap();
    let output = command("profile", &path);
    assert_eq!(output.status.code(), Some(125));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("execution budget exhausted"));
}
