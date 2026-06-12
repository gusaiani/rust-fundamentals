//! `aprobe` — an async network probe built on tokio.
//!
//! Two tools that share the same async machinery (bounded concurrency,
//! timeouts-as-cancellation, graceful shutdown):
//!
//! - [`scanner::scan`] — a concurrent TCP port scanner. Thousands of
//!   `connect` attempts in flight at once, capped by a [`tokio::sync::Semaphore`],
//!   each with a per-port timeout. The whole thing runs on *one* task per
//!   probe but a handful of OS threads — that's the async payoff.
//! - [`proxy::run_proxy`] — a TCP proxy. Accepts connections, dials an
//!   upstream, and shuttles bytes both ways with `copy_bidirectional`, then
//!   drains in-flight connections on a [`shutdown::Shutdown`] signal.
//!
//! The point of the module is *concurrency without parallelism*: one thread
//! interleaving thousands of sockets at their `.await` points, not one thread
//! per socket. Prove it with the benchmark — concurrency 256 should crush
//! sequential even though it isn't using more cores.

pub mod proxy;
pub mod scanner;
pub mod shutdown;
pub mod target;

pub use scanner::{scan, ScanConfig};
pub use shutdown::Shutdown;
pub use target::{parse_target, PortState, ScanOutcome, Target, TargetError};
