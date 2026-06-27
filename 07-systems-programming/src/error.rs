//! The one error type the protocol and store share.
//!
//! This file is **given** — it's the Module 3 discipline reused: a single enum,
//! `#[from]` conversions so `?` works across layers, and a `Result` alias. The
//! interesting distinction here is [`Error::Protocol`] (the client sent us bytes
//! that don't parse — recoverable, we drop the connection) versus
//! [`Error::Io`]/[`Error::Snapshot`] (our problem, logged).

use thiserror::Error;

/// Anything that can go wrong parsing a request or running the server.
#[derive(Debug, Error)]
pub enum Error {
    /// The bytes on the wire aren't valid RESP, or aren't a command we can run.
    /// The client's fault: we reply with a RESP error or close the connection.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// A snapshot file is truncated or has a bad magic/version header.
    #[error("corrupt snapshot: {0}")]
    Snapshot(String),

    /// Underlying I/O failure (socket, file). `?` on any `std::io` call lands here.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Convenience for the many `return Err(Error::Protocol(...))` sites.
    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
