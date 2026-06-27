//! The event loop — one thread, one `Poll`, every socket (Pill 4).
//!
//! This file is **given**: it's the worked machine you plug your protocol and
//! store into, the way `aprobe.rs` was the worked driver in Module 5. Read it as
//! the shape of what `tokio` does for you. The flow:
//!
//!   1. register the listener and the signal source with `Poll`;
//!   2. block in `poll()` until *something* is ready (a new connection, a
//!      readable client, a writable client, a signal, or the timer tick);
//!   3. for each ready event, do the non-blocking work and re-arm interest;
//!   4. on a tick, sweep expired keys; on SIGINT/SIGTERM, snapshot and exit.
//!
//! Everything it calls on a [`Connection`] — `read_into_buf`, `process`,
//! `flush_write_buf` — is yours (step 6). Until those are implemented the loop
//! boots and accepts, but a command will panic the connection.

use std::collections::HashMap;
use std::io;
use std::time::Duration;

use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook_mio::v1_0::Signals;

use crate::connection::Connection;
use crate::persistence;
use crate::store::Store;
use crate::Result;

/// The listening socket's token.
const LISTENER: Token = Token(0);
/// The signal source's token.
const SIGNALS: Token = Token(1);
/// First token handed to a client connection (0 and 1 are reserved above).
const FIRST_CONN: usize = 16;
/// How often we wake up with no I/O, to run the active-expiry sweep (Pill 9).
const TICK: Duration = Duration::from_millis(100);

/// A per-connection accept error that should cost us one connection, not the
/// whole server. The classic case is `ECONNABORTED`: the client sent a SYN and
/// then went away before we called `accept()`, so the half-formed connection is
/// already dead — but the listener is healthy. Treating this as fatal would let
/// any client kill the server by aborting at the right moment (a remote DoS).
///
/// `ConnectionReset` is included for the same reason. Resource-exhaustion errors
/// (`EMFILE`/`ENFILE`) and a broken listener fd (`EBADF`) are deliberately *not*
/// here — those are genuine, so they fall through to the fatal arm and stop us.
fn is_transient(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
    )
}

fn next_free_token(mut candidate: usize, conns: &HashMap<Token, Connection>) -> usize {
    loop {
        candidate = candidate.checked_add(1).unwrap_or(FIRST_CONN);
        if candidate >= FIRST_CONN && !conns.contains_key(&Token(candidate)) {
            return candidate;
        }
    }
}

/// Run the server on an already-bound listener until a shutdown signal arrives.
///
/// Taking a bound `TcpListener` (rather than an address) lets callers — notably
/// the tests — bind to port 0, read back the OS-chosen port, and *then* start
/// the loop.
pub fn run(mut listener: TcpListener, mut store: Store) -> Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(256);

    // Register the listener: we care about READABLE (a pending connection shows
    // up as the listener becoming readable).
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE)?;

    // Register signals as just another readable source in the same loop (Pill
    // 12). No async-signal-safety headaches: signal-hook does the unsafe part,
    // and we learn about it by polling, not in a handler.
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    poll.registry()
        .register(&mut signals, SIGNALS, Interest::READABLE)?;

    let mut conns: HashMap<Token, Connection> = HashMap::new();
    let mut next_token = FIRST_CONN;

    println!("rudis: event loop started");

    'event_loop: loop {
        // Block until something is ready, or the tick elapses. `Interrupted`
        // (EINTR) just means a signal landed mid-syscall — loop and re-poll.
        if let Err(e) = poll.poll(&mut events, Some(TICK)) {
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.into());
        }

        for event in events.iter() {
            match event.token() {
                LISTENER => {
                    // Accept every pending connection until the listener would
                    // block — one readiness event can mean several waiting.
                    loop {
                        match listener.accept() {
                            Ok((mut stream, _addr)) => {
                                let token = Token(next_token);
                                next_token = next_free_token(next_token, &conns);
                                poll.registry()
                                    .register(&mut stream, token, Interest::READABLE)?;
                                conns.insert(token, Connection::new(stream));
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                            Err(ref e) if is_transient(e) => {
                                eprintln!("rudis: accept failed, dropping connection: {e}");
                                continue;
                            }
                            Err(e) => return Err(e.into()),
                        }
                    }
                }

                SIGNALS => {
                    for signal in signals.pending() {
                        if signal == SIGINT || signal == SIGTERM {
                            println!("rudis: signal {signal} received, draining");
                            break 'event_loop;
                        }
                    }
                }

                token => {
                    let mut drop_conn = false;

                    if let Some(conn) = conns.get_mut(&token) {
                        // Readable: pull bytes in, then run whatever commands
                        // completed.
                        if event.is_readable() {
                            match conn.read_into_buf() {
                                Ok(peer_closed) => {
                                    if let Err(e) = conn.process(&mut store) {
                                        eprintln!("rudis: connection error: {e}");
                                        drop_conn = true;
                                    }
                                    if peer_closed {
                                        drop_conn = true;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("rudis: read error: {e}");
                                    drop_conn = true;
                                }
                            }
                        }

                        // Writable, or we have replies queued: try to flush.
                        if !drop_conn && (event.is_writable() || conn.wants_write()) {
                            if let Err(e) = conn.flush_write_buf() {
                                eprintln!("rudis: write error: {e}");
                                drop_conn = true;
                            }
                        }

                        // Re-arm interest: ask for WRITABLE only while output is
                        // still pending (Pill 7), otherwise just READABLE.
                        if !drop_conn {
                            let interest = if conn.wants_write() {
                                Interest::READABLE | Interest::WRITABLE
                            } else {
                                Interest::READABLE
                            };
                            poll.registry()
                                .reregister(&mut conn.stream, token, interest)?;
                        }
                    }

                    if drop_conn {
                        if let Some(mut conn) = conns.remove(&token) {
                            let _ = poll.registry().deregister(&mut conn.stream);
                        }
                    }
                }
            }
        }

        // Tick work: reclaim expired keys even if nobody touched them.
        store.purge_expired();
    }

    // Graceful shutdown: persist before we go, if persistence is configured.
    if let Some(path) = store.snapshot_path.clone() {
        match persistence::save(&store, &path) {
            Ok(()) => println!("rudis: snapshot written to {}", path.display()),
            Err(e) => eprintln!("rudis: snapshot on shutdown failed: {e}"),
        }
    }
    println!("rudis: bye");
    Ok(())
}
