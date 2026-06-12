//! A tiny async echo server — a target to scan and proxy against.
//!
//! ```text
//! cargo run --example echo_server -- 127.0.0.1:9000
//! ```
//!
//! It binds the given address and echoes every byte back. Use it as the
//! `--upstream` for `aprobe proxy`, or as an open port for `aprobe scan`:
//!
//! ```text
//! # terminal 1
//! cargo run --example echo_server -- 127.0.0.1:9000
//! # terminal 2
//! cargo run -- proxy --listen 127.0.0.1:8080 --upstream 127.0.0.1:9000
//! # terminal 3
//! printf 'hello\n' | nc 127.0.0.1 8080      # round-trips through the proxy
//! cargo run -- scan 127.0.0.1:8990-9010     # 9000 shows up as open
//! ```
//!
//! This one is fully written — it's scaffolding, not the exercise. Read it as
//! a worked example of the accept-loop + spawn-per-connection shape you'll
//! build (with graceful shutdown added) in `proxy.rs`.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:9000".into());
    let listener = TcpListener::bind(&addr).await?;
    eprintln!("echo_server listening on {addr}");

    loop {
        let (mut socket, peer) = listener.accept().await?;
        // One task per connection — they all interleave on the runtime's threads.
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => break, // peer closed
                    Ok(n) => {
                        if socket.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = peer;
        });
    }
}
