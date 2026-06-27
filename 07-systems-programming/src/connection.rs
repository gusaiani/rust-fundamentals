//! Per-client connection state: the read buffer, the write buffer, and the
//! non-blocking I/O that fills and drains them (Pills 6 & 7).
//!
//! The struct is **given**; the three methods are **step 6** — and they are the
//! systems-programming core of the module. A `mio` socket is *non-blocking*:
//! `read`/`write` return immediately, and "there's nothing more right now" shows
//! up as `io::ErrorKind::WouldBlock`. You must loop until you see it.

use std::io;

use mio::net::TcpStream;

use crate::command::Command;
use crate::resp::Resp;
use crate::store::Store;
use crate::Result;

/// Everything we track for one connected client.
pub struct Connection {
    /// The non-blocking socket, registered with the event loop's `Poll`.
    pub stream: TcpStream,
    /// Bytes received but not yet parsed into a complete command.
    pub read_buf: Vec<u8>,
    /// Replies produced but not yet written to the socket.
    pub write_buf: Vec<u8>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream,
            read_buf: Vec::with_capacity(16 * 1024),
            write_buf: Vec::new(),
        }
    }

    /// Is there pending output? The event loop uses this to decide whether to
    /// ask `Poll` for writable readiness (Pill 7).
    pub fn wants_write(&self) -> bool {
        !self.write_buf.is_empty()
    }

    /// Drain the socket into `read_buf` until it would block.
    ///
    /// Returns `Ok(true)` if the peer closed the connection (a clean EOF),
    /// `Ok(false)` if we've read everything available for now.
    ///
    /// (Add `use std::io::Read;` to call `self.stream.read(...)`.)
    pub fn read_into_buf(&mut self) -> io::Result<bool> {
        use std::io::Read;

        let mut tmp = [0u8; 16 * 1024];

        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => return Ok(true),
                Ok(n) => self.read_buf.extend_from_slice(&tmp[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Parse every complete command currently in `read_buf`, execute it, and
    /// append each reply to `write_buf`.
    pub fn process(&mut self, store: &mut Store) -> Result<()> {
        loop {
            match Resp::parse(&self.read_buf)? {
                Some((value, consumed)) => {
                    self.read_buf.drain(..consumed);

                    match Command::parse(value) {
                        Ok(command) => command.execute(store).encode(&mut self.write_buf),
                        Err(e) => Resp::error(format!("ERR {e}")).encode(&mut self.write_buf),
                    }
                }
                None => return Ok(()),
            }
        }
    }

    /// Write as much of `write_buf` as the socket will take right now, then drop
    /// the bytes that made it out.
    ///
    /// (Add `use std::io::Write;` to call `self.stream.write(...)`.)

    pub fn flush_write_buf(&mut self) -> io::Result<()> {
        use std::io::Write;

        let mut sent = 0;
        while sent < self.write_buf.len() {
            match self.stream.write(&self.write_buf[sent..]) {
                Ok(0) => break,
                Ok(n) => sent += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.write_buf.drain(..sent);
                    return Err(e);
                }
            }
        }
        self.write_buf.drain(..sent); // unsent bytes stay queued for the next WRITABLE event
        Ok(())
    }
}
