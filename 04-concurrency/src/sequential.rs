//! The single-threaded baseline.
//!
//! This is two things at once: the **correctness oracle** (the parallel
//! variants must produce an identical report to this) and the **speedup
//! denominator** (every benchmark number is "× faster than this"). Keep it
//! boring and obviously correct.

#[allow(unused_imports)]
use crate::parser::parse_line;
#[allow(unused_imports)]
use crate::stats::Stats;

/// Analyze an in-memory log buffer on one thread.
///
/// Takes `&[u8]` (not a path) so the benchmark can measure pure CPU work
/// without re-reading the file each iteration.
//
// TODO (step 3):
//   1. `std::str::from_utf8(data)` (or `from_utf8_lossy`).
//   2. `let mut stats = Stats::default();`
//   3. For each non-empty `line` in `.lines()`: `parse_line(line)` →
//      `stats.record(&e)` on Ok, `stats.record_malformed()` on Err.
//   4. Return `stats`.
pub fn analyze_sequential(data: &[u8]) -> Stats {
    let _ = data;
    todo!("step 3: line loop into one Stats")
}
