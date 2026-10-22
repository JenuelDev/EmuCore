//! Compare both backends on an owned fixture whose instructions are already supported.
#![forbid(unsafe_code)]
use emucore::{
    elf::ElfImage,
    runtime::{self, Backend},
};
use std::{io::Write, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("expected one owned ELF path".into());
    }
    let image = ElfImage::from_path(Path::new(&arguments[0]))?;
    let native = runtime::run_with_backend(&image, Backend::Native)?;
    let reference = runtime::run_with_backend(&image, Backend::Reference)?;
    if native != reference {
        return Err("native/reference observable state mismatch".into());
    }
    if native.exit_code != 0 {
        return Err(format!("guest exited with {}", native.exit_code).into());
    }
    std::io::stdout().lock().write_all(&native.stdout)?;
    Ok(())
}
