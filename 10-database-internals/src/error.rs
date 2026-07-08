//! The crate's error type — **given**, you don't implement this.
//!
//! A storage engine touches the two things most likely to go wrong at runtime:
//! the filesystem (`io::Error`) and its own on-disk formats (a corrupt record, a
//! bad magic number, a truncated footer). We keep them as separate variants so a
//! caller can tell "the disk is full" apart from "this file isn't one of ours /
//! is damaged" — the second is what crash recovery has to reason about.

use std::fmt;
use std::io;

/// Anything that can go wrong opening, reading, or writing the store.
#[derive(Debug)]
pub enum Error {
    /// An underlying filesystem error (open/read/write/rename/fsync).
    Io(io::Error),
    /// An on-disk structure didn't match what we wrote: bad magic, an
    /// unparseable record, a footer that doesn't add up. The `String` says where.
    Corrupt(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Corrupt(what) => write!(f, "corrupt data: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Corrupt(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Convenience alias so signatures read `-> Result<T>`.
pub type Result<T> = std::result::Result<T, Error>;
