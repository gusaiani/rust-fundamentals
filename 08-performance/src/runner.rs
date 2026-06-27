//! The drivers: single-core and parallel aggregation over a mapped buffer.
//!
//! Both return the same thing — a sorted `BTreeMap<Vec<u8>, Stats>` ready for
//! [`crate::aggregate::format_results`] — so the integration test can assert the
//! parallel path agrees with the sequential one byte-for-byte.

use std::collections::BTreeMap;

use crate::aggregate::{into_sorted, Stats};
use crate::hash::FastMap;
use crate::io::split_chunks;
use crate::parse::{parse_temp, split_line};

/// Aggregate one contiguous byte range into a borrowed-key map.
///
/// This is the hot loop (Pills 6–9): iterate `\n`-delimited lines with
/// `memchr::memchr_iter` (SIMD, Pill 11), [`split_line`] each, [`parse_temp`] the
/// temperature, and fold it in via the entry API so each row is **one** hash and
/// **one** probe with **no** allocation. Keys borrow `data`, so the returned map
/// is tied to `data`'s lifetime.
///
/// TODO (step 4): implement the loop. See the step-4 hint.
pub fn aggregate<'a>(data: &'a [u8]) -> FastMap<&'a [u8], Stats> {
    let _ = (data, split_line as fn(&[u8]) -> (&[u8], &[u8]), parse_temp as fn(&[u8]) -> i32);
    todo!("memchr_iter lines -> split -> parse -> entry().record() — see step 4 hint")
}

/// Single-core path: aggregate the whole buffer, then sort for output.
///
/// TODO (step 4): call [`aggregate`] over all of `data` and hand the result to
/// [`into_sorted`].
pub fn run_sequential(data: &[u8]) -> BTreeMap<Vec<u8>, Stats> {
    let _ = (data, into_sorted as fn(FastMap<&[u8], Stats>) -> BTreeMap<Vec<u8>, Stats>);
    todo!("aggregate(data) then into_sorted(..) — see step 4")
}

/// Parallel path (Pill 12): split `data` into `threads` newline-aligned chunks,
/// aggregate each on its own scoped thread into its own map, then merge the maps.
///
/// Use [`std::thread::scope`] so each worker can borrow `&[u8]` slices into `data`
/// with no `Arc` and no `'static` bound — the scope guarantees the threads finish
/// before `data` could be dropped. Merge with [`Stats::merge`].
///
/// TODO (step 8): implement split -> scoped fan-out -> merge -> sort. See the
/// step-4/8 hint. If `threads <= 1`, just defer to [`run_sequential`].
pub fn run_parallel(data: &[u8], threads: usize) -> BTreeMap<Vec<u8>, Stats> {
    let _ = (data, threads, split_chunks as fn(&[u8], usize) -> Vec<&[u8]>);
    todo!("thread::scope over split_chunks, merge per-thread maps — see step 8 hint")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::format_results;

    const SAMPLE: &[u8] = b"\
Hamburg;12.0
Bulawayo;8.9
Palembang;38.8
Hamburg;-3.4
Bulawayo;19.2
Palembang;-5.0
Hamburg;0.0
";

    #[test]
    fn sequential_aggregates_correctly() {
        let out = run_sequential(SAMPLE);
        let hamburg = out.get(b"Hamburg".as_slice()).unwrap();
        assert_eq!(hamburg.min, -34);
        assert_eq!(hamburg.max, 120);
        assert_eq!(hamburg.count, 3);
        assert_eq!(hamburg.sum, 120 - 34 + 0);
    }

    #[test]
    fn parallel_matches_sequential() {
        let seq = format_results(&run_sequential(SAMPLE));
        for threads in [1, 2, 3, 8] {
            let par = format_results(&run_parallel(SAMPLE, threads));
            assert_eq!(seq, par, "threads={threads}");
        }
    }
}
