//! Full-stack tests: a **real client over a real TCP socket** driving the
//! **real event loop**. No mocks — this is the same path `redis-cli` takes.
//!
//! Each test binds a fresh listener on port 0 (the OS picks a free port), starts
//! the server on a background thread, and connects to it. They go green as you
//! implement the protocol (step 1–2), store (step 3), command layer (step 4–5),
//! and connection I/O (step 6).
//!
//! What we prove:
//!   1. PING / ECHO round-trip (the protocol frames correctly)
//!   2. SET then GET returns the value; GET of a missing key is the null bulk
//!   3. DEL / EXISTS / INCR integer replies
//!   4. TTL semantics: -2 missing, -1 no-expiry
//!   5. pipelining: two commands in one write get two replies in order

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::Duration;

use mio::net::TcpListener;
use rudis::server;
use rudis::store::Store;

/// Start a server on an ephemeral port; return the address to connect to.
fn start_server() -> SocketAddr {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(addr).expect("bind");
    let local = listener.local_addr().expect("local_addr");
    thread::spawn(move || {
        // No persistence in tests. Ignore the result — the thread lives until
        // the test process exits.
        let _ = server::run(listener, Store::new(None));
    });
    local
}

/// A tiny blocking RESP client.
struct Client {
    stream: TcpStream,
}

impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        Client { stream }
    }

    /// Send a command as a RESP array of bulk strings (what redis-cli sends).
    fn send(&mut self, parts: &[&str]) {
        self.stream.write_all(&encode_command(parts)).expect("write");
    }

    /// Send raw bytes (for the pipelining test).
    fn send_raw(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).expect("write");
    }

    /// Read exactly the bytes of `expected` and assert they match.
    fn expect(&mut self, expected: &str) {
        let mut buf = vec![0u8; expected.len()];
        self.stream.read_exact(&mut buf).expect("read reply");
        assert_eq!(
            String::from_utf8_lossy(&buf),
            expected,
            "unexpected reply bytes"
        );
    }
}

/// Encode `parts` as `*N\r\n$len\r\n<part>\r\n...`.
fn encode_command(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("*{}\r\n", parts.len()).as_bytes());
    for p in parts {
        out.extend_from_slice(format!("${}\r\n", p.len()).as_bytes());
        out.extend_from_slice(p.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

#[test]
fn ping_and_echo() {
    let mut c = Client::connect(start_server());
    c.send(&["PING"]);
    c.expect("+PONG\r\n");
    c.send(&["PING", "hello"]);
    c.expect("$5\r\nhello\r\n");
    c.send(&["ECHO", "world"]);
    c.expect("$5\r\nworld\r\n");
}

#[test]
fn set_get_and_missing() {
    let mut c = Client::connect(start_server());
    c.send(&["SET", "foo", "bar"]);
    c.expect("+OK\r\n");
    c.send(&["GET", "foo"]);
    c.expect("$3\r\nbar\r\n");
    c.send(&["GET", "nope"]);
    c.expect("$-1\r\n"); // null bulk
}

#[test]
fn del_exists_incr() {
    let mut c = Client::connect(start_server());
    c.send(&["SET", "k", "v"]);
    c.expect("+OK\r\n");
    c.send(&["EXISTS", "k"]);
    c.expect(":1\r\n");
    c.send(&["DEL", "k"]);
    c.expect(":1\r\n");
    c.send(&["DEL", "k"]);
    c.expect(":0\r\n");

    c.send(&["INCR", "n"]);
    c.expect(":1\r\n");
    c.send(&["INCR", "n"]);
    c.expect(":2\r\n");
}

#[test]
fn ttl_semantics() {
    let mut c = Client::connect(start_server());
    c.send(&["TTL", "ghost"]);
    c.expect(":-2\r\n"); // missing key
    c.send(&["SET", "k", "v"]);
    c.expect("+OK\r\n");
    c.send(&["TTL", "k"]);
    c.expect(":-1\r\n"); // exists, no expiry
}

#[test]
fn pipelining() {
    let mut c = Client::connect(start_server());
    // Two commands in a single write — one read on the server side must produce
    // two replies, in order (Pill 13).
    let mut batch = encode_command(&["SET", "a", "1"]);
    batch.extend_from_slice(&encode_command(&["GET", "a"]));
    c.send_raw(&batch);
    c.expect("+OK\r\n");
    c.expect("$1\r\n1\r\n");
}
