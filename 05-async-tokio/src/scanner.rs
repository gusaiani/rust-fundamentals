//! The concurrent TCP port scanner — the async heart of this module.
//!
//! Shape: spawn one lightweight future per `(host, port)`, but cap how many
//! run at once with a [`Semaphore`] (Pill 11). Each probe is a `connect`
//! wrapped in [`tokio::time::timeout`] — when the timeout wins the race the
//! connect future is *dropped*, which **is** the cancellation (Pill 10).
//! Results funnel back through an [`mpsc`] channel (Pill 12) or a
//! [`JoinSet`] (Pill 13) — you'll build one of each and feel the difference.
//!
//! Crucially this is *concurrency, not parallelism*: 256 in-flight connects
//! are 256 sockets parked at `.await`, interleaved by a couple of OS threads.
//! No thread-per-connection. The benchmark proves the win comes from
//! overlapping I/O wait, not from using more cores.

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use tokio::{net::TcpStream, sync::Semaphore, task::JoinSet};

use crate::target::{PortState, ScanOutcome, Target};

/// Tunables for a scan.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Max probes in flight at once (the semaphore's permit count). This is
    /// the knob the benchmark sweeps: 1 ≈ sequential, 256 ≈ saturated.
    pub concurrency: usize,
    /// Per-port deadline. A `connect` that doesn't resolve in this window is
    /// `Filtered`. Too short → false `Filtered`; too long → slow scans.
    pub timeout: Duration,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            concurrency: 256,
            timeout: Duration::from_millis(500),
        }
    }
}

/// Probe one `host:port`. Returns the [`ScanOutcome`] — never errors, because
/// "couldn't connect" is data here, not failure.
///
/// Logic:
///   1. Record a start instant (for `rtt`).
///   2. `tokio::time::timeout(cfg.timeout, TcpStream::connect((host, port)))`.
///   3. Map the nested result:
///        - `Ok(Ok(_stream))`  → `PortState::Open`   (drop the stream — we
///          only wanted to know it opened; a real scanner might banner-grab)
///        - `Ok(Err(_))`       → `PortState::Closed`  (connection refused = RST)
///        - `Err(_elapsed)`    → `PortState::Filtered` (the timeout fired;
///          the connect future is dropped here — that drop is the cancel)
///
/// Note `host` is `&str`: tokio's `connect` resolves it (DNS) for you. For a
/// big scan you'd resolve once up front — see the stretch goals.
pub async fn probe_port(host: &str, port: u16, cfg: &ScanConfig) -> ScanOutcome {
    let start = Instant::now();

    let connect = TcpStream::connect((host, port));
    let result = tokio::time::timeout(cfg.timeout, connect).await;

    let state = match result {
        Ok(Ok(_stream)) => PortState::Open,
        Ok(Err(_)) => PortState::Closed,
        Err(_elapsed) => PortState::Filtered,
    };

    ScanOutcome {
        port,
        state,
        rtt: start.elapsed(),
    }
}

/// Scan every port in `target`, at most `cfg.concurrency` at a time.
///
/// This is the orchestration. Build it with a [`Semaphore`] + [`JoinSet`]:
///   1. `let sem = Arc::new(Semaphore::new(cfg.concurrency));`
///   2. For each port, `let permit = sem.clone().acquire_owned().await.unwrap();`
///      — this line is the backpressure: it *blocks the spawn loop* once
///      `concurrency` probes are in flight, so you never have a million tasks
///      queued at once.
///   3. `set.spawn(async move { let out = probe_port(&host, port, &cfg).await;
///      drop(permit); out });` — the permit drops with the task, freeing a slot.
///   4. Drain the `JoinSet` with `while let Some(res) = set.join_next().await`,
///      collecting outcomes.
///   5. Sort outcomes by port before returning (task completion order is
///      nondeterministic — the caller wants stable output).
///
/// Everything `spawn`ed must be `Send + 'static` — that's why `host`/`cfg`
/// are cloned into each task and the `Arc<Semaphore>` is shared by clone.
pub async fn scan(target: &Target, cfg: &ScanConfig) -> Vec<ScanOutcome> {
    let sem = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
    let mut set = JoinSet::new();

    let host = target.host.clone();

    for &port in &target.ports {
        let host = host.clone();
        let cfg = cfg.clone();
        let sem = sem.clone();

        let permit = sem.acquire_owned().await.unwrap();

        set.spawn(async move {
            let outcome = probe_port(&host, port, &cfg).await;
            drop(permit);
            outcome
        });
    }

    let mut outcomes = Vec::new();

    while let Some(joined) = set.join_next().await {
        outcomes.push(joined.unwrap());
    }

    outcomes.sort_by_key(|o| o.port);
    outcomes
}

/// The deliberately-slow baseline: probe ports strictly one at a time.
///
/// This is the speedup *denominator* and a correctness oracle (it must find
/// exactly the same open ports as [`scan`]). It's a one-liner sequential loop
/// over `probe_port` — no semaphore, no spawning, no overlap. On a range with
/// even a few `Filtered` ports it's brutal: every timeout is paid in series.
pub async fn scan_sequential(target: &Target, cfg: &ScanConfig) -> Vec<ScanOutcome> {
    let mut outcomes = Vec::new();

    for &port in &target.ports {
        outcomes.push(probe_port(&target.host, port, cfg).await);
    }

    outcomes
}
