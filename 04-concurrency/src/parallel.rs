//! Hand-rolled map-reduce with scoped threads — the core of this module.
//!
//! Split the buffer into `n` newline-aligned chunks, give each chunk its own
//! thread and its own local `Stats` (no sharing, no `Mutex`), then merge the
//! locals once at the end. This is Pill 7 written out by hand so you see what
//! `rayon` does for you in `rayon_impl.rs`.

#[allow(unused_imports)]
use crate::parser::parse_line;
#[allow(unused_imports)]
use crate::stats::{Merge, Stats};

/// Split `data` into at most `n` sub-slices, each starting and ending on a
/// record boundary (just past a `\n`), so no line is split across chunks.
///
/// This is the classic parallel-file bug if you get it wrong: a raw byte
/// split lands mid-line and that line is double-counted or dropped *silently*.
//
// TODO (step 5):
//   - raw split for chunk i ≈ i * data.len() / n
//   - from each raw offset, advance while data[end] != b'\n' (and end <
//     data.len()), then step past the '\n' — that index is this chunk's end
//     and the next chunk's start
//   - chunk 0 starts at 0; the final chunk ends at data.len()
//   - return Vec<&[u8]> sub-slices (zero-copy)
// Unit-test this BEFORE writing analyze_parallel. Edge cases: no trailing
// newline, split landing exactly on '\n', n > line count, empty input.
pub fn split_into_chunks(data: &[u8], n: usize) -> Vec<&[u8]> {
    let _ = (data, n);
    todo!("step 5: newline-aligned chunk boundaries")
}

/// Map-reduce the buffer across `n_threads` scoped threads.
//
// TODO (step 6):
//   1. `let chunks = split_into_chunks(data, n_threads);`
//   2. `std::thread::scope(|s| { ... })`: spawn one closure per chunk that
//      builds a LOCAL `Stats` (same line loop as sequential) and RETURNS it;
//      collect the `ScopedJoinHandle`s.
//   3. Fold: `let mut acc = Stats::default(); for h in handles {
//      acc.merge(h.join().unwrap()) }` — no `Mutex` anywhere.
//   4. Return `acc`.
pub fn analyze_parallel(data: &[u8], n_threads: usize) -> Stats {
    let _ = (data, n_threads);
    todo!("step 6: scoped-thread map-reduce")
}
