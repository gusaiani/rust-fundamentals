//! Reader → bounded channel → worker pool pipeline.
//!
//! Decouples the I/O-ish "produce lines" stage from the CPU-bound "parse +
//! aggregate" stage. The channel is **bounded** on purpose (Pill 10): an
//! unbounded queue on a huge log is an OOM waiting to happen — a bounded one
//! makes the producer block when workers fall behind (free backpressure).

#[allow(unused_imports)]
use crossbeam_channel::bounded;

#[allow(unused_imports)]
use crate::parser::parse_line;
#[allow(unused_imports)]
use crate::stats::{Merge, Stats};

/// Lines per channel message. One line per message makes channel overhead
/// dominate; batch so the synchronization cost amortizes.
#[allow(dead_code)]
const BATCH: usize = 1024;

/// Analyze via a producer thread + `n_workers` consumer threads.
//
// TODO (step 7):
//   1. `let (tx, rx) = bounded::<Vec<String>>(64);`  // bounded, not unbounded
//   2. `std::thread::scope(|s| { ... })`:
//      - PRODUCER: walk `data` lines, fill a `Vec<String>` of `BATCH`,
//        `tx.send(batch)` when full; send the remainder; then DROP `tx`
//        (let it fall out of scope) so workers observe disconnect.
//      - `n_workers` WORKERS: each `let rx = rx.clone()`, loop
//        `for batch in rx { ... }`, parse into its OWN local `Stats`,
//        return that `Stats`.
//   3. Merge all worker `Stats` into one and return it.
// Shutdown gotcha: workers exit only when the channel is empty AND every
// Sender is dropped. Keep a stray `tx` alive and they hang forever.
pub fn analyze_pipeline(data: &[u8], n_workers: usize) -> Stats {
    let _ = (data, n_workers);
    todo!("step 7: bounded-channel reader/worker pipeline")
}
