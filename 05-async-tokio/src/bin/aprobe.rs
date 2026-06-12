//! `aprobe` CLI.
//!
//! ```text
//! aprobe scan  <host:ports> [--concurrency N] [--timeout-ms T]
//! aprobe proxy --listen <addr> --upstream <addr>
//! ```
//!
//! Examples:
//! ```text
//! aprobe scan 127.0.0.1:1-1024 --concurrency 512 --timeout-ms 300
//! aprobe proxy --listen 127.0.0.1:8080 --upstream 127.0.0.1:9000
//! ```
//!
//! No clap on purpose (Module 4's lesson carries over): hand-rolled arg
//! parsing keeps the dependency list honest and the async the star.
//! `#[tokio::main]` builds the multi-threaded runtime and blocks on `main`'s
//! future — that's the one place the sync world meets the async world.

use std::process::ExitCode;
use std::time::Duration;

use aprobe::scanner::{scan, ScanConfig};
use aprobe::shutdown::Shutdown;
use aprobe::target::{parse_target, PortState};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("scan") => run_scan(&args[1..]).await,
        Some("proxy") => run_proxy_cmd(&args[1..]).await,
        _ => {
            eprintln!("usage:");
            eprintln!("  aprobe scan  <host:ports> [--concurrency N] [--timeout-ms T]");
            eprintln!("  aprobe proxy --listen <addr> --upstream <addr>");
            ExitCode::FAILURE
        }
    }
}

/// Pull the value following `--flag` and parse it, or `None`.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).map(String::as_str)
}

async fn run_scan(args: &[String]) -> ExitCode {
    let spec = match args.iter().find(|a| !a.starts_with('-')) {
        Some(s) => s,
        None => {
            eprintln!("scan: need a target, e.g. 127.0.0.1:1-1024");
            return ExitCode::FAILURE;
        }
    };

    let target = match parse_target(spec) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("scan: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut cfg = ScanConfig::default();
    if let Some(v) = flag(args, "--concurrency") {
        match v.parse::<usize>() {
            Ok(n) if n > 0 => cfg.concurrency = n,
            _ => {
                eprintln!("--concurrency needs a positive integer");
                return ExitCode::FAILURE;
            }
        }
    }
    if let Some(v) = flag(args, "--timeout-ms") {
        match v.parse::<u64>() {
            Ok(ms) if ms > 0 => cfg.timeout = Duration::from_millis(ms),
            _ => {
                eprintln!("--timeout-ms needs a positive integer");
                return ExitCode::FAILURE;
            }
        }
    }

    eprintln!(
        "scanning {} port(s) on {} (concurrency {}, timeout {:?})",
        target.ports.len(),
        target.host,
        cfg.concurrency,
        cfg.timeout
    );

    let outcomes = scan(&target, &cfg).await;

    // Print only the interesting ports; closed/filtered are the boring majority.
    let mut open = 0u64;
    for o in &outcomes {
        if o.state == PortState::Open {
            println!("{:>6}/tcp  open      ({:?})", o.port, o.rtt);
            open += 1;
        }
    }
    let filtered = outcomes.iter().filter(|o| o.state == PortState::Filtered).count();
    eprintln!(
        "done: {} open, {} filtered, {} total",
        open,
        filtered,
        outcomes.len()
    );

    ExitCode::SUCCESS
}

async fn run_proxy_cmd(args: &[String]) -> ExitCode {
    let listen = match flag(args, "--listen") {
        Some(v) => v.to_string(),
        None => {
            eprintln!("proxy: --listen <addr> is required");
            return ExitCode::FAILURE;
        }
    };
    let upstream = match flag(args, "--upstream") {
        Some(v) => v.to_string(),
        None => {
            eprintln!("proxy: --upstream <addr> is required");
            return ExitCode::FAILURE;
        }
    };

    // Wire Ctrl-C to the shutdown handle so the proxy drains instead of dying.
    let shutdown = Shutdown::new();
    let sig = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\naprobe proxy: shutdown requested, draining…");
        sig.trigger();
    });

    match aprobe::proxy::run_proxy(&listen, &upstream, shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("proxy: {e}");
            ExitCode::FAILURE
        }
    }
}
