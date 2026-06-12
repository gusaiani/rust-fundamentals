//! A graceful TCP proxy.
//!
//! Accept connections on `listen`, dial `upstream` for each, and pump bytes
//! both directions until either side closes. The interesting parts are async
//! I/O ([`copy_bidirectional`], Pill 15), one **spawned task per connection**
//! (Pill 6 — but cancellation-aware, Pill 14), and *graceful* shutdown: when
//! the [`Shutdown`] flag flips, stop accepting new connections but let the
//! in-flight ones drain.
//!
//! This is the canonical "C10k on a laptop" demo: thousands of concurrent
//! proxied connections on a handful of threads, because every task spends
//! almost all its time parked at an `.await` waiting for bytes.

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};

use crate::shutdown::Shutdown;

/// How many bytes flowed each way on one proxied connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transferred {
    pub client_to_upstream: u64,
    pub upstream_to_client: u64,
}

/// Run the proxy until `shutdown` triggers. Returns when the listener is
/// closed and all accepted connections have drained.
///
/// The accept loop is a `select!` (Pill 9) between two arms:
///   - `listener.accept()` → got a connection: clone `upstream` + `shutdown`,
///     `tokio::spawn(handle_conn(...))`. Do **not** `.await` the handle here —
///     that would serialize connections; spawning is what makes them concurrent.
///   - `shutdown.cancelled()` → stop looping. Break out, optionally join the
///     spawned tasks (a `JoinSet` makes "wait for in-flight to drain" clean).
///
/// ```text
/// loop {
///     select! {
///         res = listener.accept() => { let (sock, _) = res?; spawn(handle_conn(sock, up, sd)); }
///         _   = shutdown.cancelled() => break,
///     }
/// }
/// ```
pub async fn run_proxy(
    listen: &str,
    upstream: &str,
    shutdown: Shutdown,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    eprintln!("aprobe proxy: {listen} -> {upstream} (Ctrl-C to drain & exit)");

    // TODO (step 8): the select! accept loop described above. Spawn one task
    // per connection; break on shutdown; let in-flight connections finish.
    let _ = (listener, upstream, shutdown);
    todo!("graceful accept loop")
}

/// Proxy one client connection to `upstream`.
///
/// 1. `let mut upstream = TcpStream::connect(upstream).await?;` — dial out.
/// 2. `copy_bidirectional(&mut inbound, &mut upstream).await` — tokio pumps
///    both halves concurrently and returns `(a_to_b, b_to_a)` byte counts when
///    either side hits EOF. One call, both directions, full backpressure.
/// 3. Map the byte tuple into [`Transferred`].
///
/// Errors (upstream refused, connection reset mid-stream) are returned, not
/// panicked — one bad connection must never take the proxy down. The caller
/// logs and moves on.
pub async fn handle_conn(
    mut inbound: TcpStream,
    upstream: &str,
) -> std::io::Result<Transferred> {
    // TODO (step 8): connect upstream, copy_bidirectional, return the counts.
    // (`copy_bidirectional` is imported above and unused until you wire it —
    // that's the one warning you'll see against the stubbed tree.)
    let _ = &mut inbound;
    let _ = upstream;
    todo!("dial upstream and pump bytes both ways")
}
