//! The single-threaded baseline.
//!
//! This is two things at once: the **correctness oracle** (the parallel
//! variants must produce an identical report to this) and the **speedup
//! denominator** (every benchmark number is "× faster than this"). Keep it
//! boring and obviously correct.

use crate::parser::parse_line;
use crate::stats::Stats;

/// Analyze an in-memory log buffer on one thread.
///
/// Takes `&[u8]` (not a path) so the benchmark can measure pure CPU work
/// without re-reading the file each iteration.
pub fn analyze_sequential(data: &[u8]) -> Stats {
    // from_utf8_lossy never fails — invalid bytes become the replacement char,
    // which is fine for a log analyzer (a corrupt byte ≈ a malformed line)
    let text = String::from_utf8_lossy(data);
    let mut stats = Stats::default();

    for line in text.lines() {
        // .lines() already strips the trailing \n; skip blank lines so a
        // trailing newline at EOF doesn't count as a malformed record.
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok(entry) => stats.record(&entry),
            Err(_) => stats.record_malformed(),
        }
    }

    stats
}
