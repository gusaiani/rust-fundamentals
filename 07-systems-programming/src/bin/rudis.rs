//! `rudis` server entrypoint.
//!
//! This file is **given** — the worked `main`. Read it as the boot order:
//! resolve config → build the store → load a snapshot if one exists → bind the
//! listener → run the event loop. It compiles and boots against the stubbed
//! library; commands will `todo!()`-panic the connection until you implement the
//! protocol, store, and connection methods.
//!
//! ```text
//! BIND_ADDR=127.0.0.1:6379 SNAPSHOT_PATH=rudis.rdb cargo run
//! # then, in another terminal:
//! redis-cli -p 6379 ping
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use mio::net::TcpListener;

use rudis::server;
use rudis::store::Store;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rudis: fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> rudis::Result<()> {
    // Twelve-factor config: env first, sane defaults so `cargo run` just works.
    let bind_addr = env_or("BIND_ADDR", "127.0.0.1:6379");
    let snapshot_path = std::env::var("SNAPSHOT_PATH").ok().map(PathBuf::from);

    let mut store = Store::new(snapshot_path.clone());

    // Warm start: if a snapshot exists, load it before accepting traffic.
    if let Some(path) = &snapshot_path {
        if path.exists() {
            rudis::persistence::load(path, &mut store)?;
            println!("rudis: loaded snapshot from {} ({} keys)", path.display(), store.len());
        }
    }

    // Bind the non-blocking listener. mio wants a parsed SocketAddr.
    let addr = bind_addr
        .parse()
        .map_err(|_| rudis::Error::protocol(format!("invalid BIND_ADDR: {bind_addr}")))?;
    let listener = TcpListener::bind(addr)?;
    println!("rudis: listening on {bind_addr}");

    server::run(listener, store)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
