#![forbid(unsafe_code)]

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Usage,
    Io,
    InvalidElf,
    Unsupported,
    Decode,
    Runtime,
    Host,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn io(context: &str, error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, format!("{context}: {error}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
