//! `rudis` — a minimal Redis-compatible server, built on a hand-written event
//! loop instead of an async runtime.
//!
//! The point of the module is to do by hand the things Modules 4 and 5 let a
//! library do for you, and see the machine underneath:
//!
//! - [`server::run`] is a **single-threaded `mio` event loop** — it owns one
//!   `Poll`, registers the listener and every client socket, and reacts to
//!   *readiness* events. This is the shape of `tokio`'s scheduler with the lid
//!   off (Pill 4).
//! - [`resp`] is the **wire protocol**: an incremental parser that turns a byte
//!   buffer into [`resp::Resp`] values, and an encoder that turns them back into
//!   bytes. Framing — finding where one message ends — is the whole job (Pill 5).
//! - [`connection::Connection`] holds the **read and write buffers** for one
//!   client and does the non-blocking I/O dance: drain the socket until it would
//!   block, parse whatever complete commands arrived, queue the replies, flush
//!   until the kernel's buffer is full (Pills 6 & 7).
//! - [`command::Command`] is the **command set** (`GET`/`SET`/`DEL`/…); it
//!   parses a RESP array and executes against the [`store::Store`] keyspace.
//! - [`persistence`] gives the in-memory store **durability**: a snapshot file
//!   it writes on `SAVE`/shutdown and memory-maps back in on boot (Pills 10 & 11).
//!
//! The two skills that make this "systems programming" rather than "a hash map
//! behind a socket": non-blocking readiness I/O multiplexed over one thread, and
//! a byte-stream protocol you frame yourself.

pub mod command;
pub mod connection;
pub mod error;
pub mod persistence;
pub mod resp;
pub mod server;
pub mod store;

pub use error::{Error, Result};
pub use resp::Resp;
pub use store::Store;
