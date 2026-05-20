//! The same map-reduce, handed to `rayon`.
//!
//! Once you've hand-rolled `parallel.rs`, this should feel like cheating:
//! `fold` builds per-task local `Stats`, `reduce` combines them with the
//! exact same associative `Merge`. Rayon's work-stealing pool handles the
//! threading, splitting, and load balancing. Compare its benchmark number
//! to your hand-rolled version — sometimes you win, usually it's close.

#[allow(unused_imports)]
use rayon::prelude::*;

#[allow(unused_imports)]
use crate::parser::parse_line;
#[allow(unused_imports)]
use crate::stats::{Merge, Stats};

/// Analyze with a rayon parallel iterator.
//
// TODO (step 8):
//   - Get the lines parallelizable. Simplest: `data.lines().collect::<Vec<_>>()`
//     then `.par_iter()`. (Benchmarking `.lines().par_bridge()` against this
//     and noting which wins — and why — is worthwhile.)
//   - `.fold(Stats::default, |mut acc, line| { match parse_line(line) {
//        Ok(e) => acc.record(&e), Err(_) => acc.record_malformed() }; acc })`
//   - `.reduce(Stats::default, |mut a, b| { a.merge(b); a })`
// Note: rayon's pool size is global (`RAYON_NUM_THREADS` /
// `ThreadPoolBuilder`); this signature ignores `n_threads` on purpose — the
// benchmark sets the pool size around the call.
pub fn analyze_rayon(data: &[u8]) -> Stats {
    let _ = data;
    todo!("step 8: par_iter().fold().reduce()")
}
