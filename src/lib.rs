//! A bounded ELF execution experiment, not an Android runtime or general x86 emulator.

#[cfg(windows)]
pub mod android_reference;
#[cfg(windows)]
pub mod client;
pub mod cpu;
pub mod decode;
pub mod elf;
pub mod error;
pub mod fixture;
pub mod flags;
#[cfg(windows)]
pub mod game_library;
#[cfg(windows)]
pub mod host_process;
#[cfg(windows)]
pub mod launch_gate;
pub mod memory;
mod native;
pub mod process;
pub mod runtime;
pub mod vm;

pub use error::{Error, ErrorKind, Result};
