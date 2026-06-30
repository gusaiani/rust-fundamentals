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
pub fn aggregate<'a>(data: &'a [u8]) -> FastMap<&'a [u8], Stats> {
    let mut map: FastMap<&[u8], Stats> = FastMap::default();
    let mut start = 0; // index just past the previous '\n'

    for nl in memchr::memchr_iter(b'\n', data) {
        let line = &data[start..nl];
        start = nl + 1;

        let (name, temp) = split_line(line); // &[u8] name, &[u8] temp
        let temp = parse_temp(temp); // i32 tenths
        map.entry(name).or_default().record(temp); // one hash, one probe, no alloc
    }

    map
}

/// Single-core path: aggregate the whole buffer, then sort for output.
pub fn run_sequential(data: &[u8]) -> BTreeMap<Vec<u8>, Stats> {
    let map = aggregate(data);
    into_sorted(map)
}

/// Parallel path (Pill 12): split `data` into `threads` newline-aligned chunks,
/// aggregate each on its own scoped thread into its own map, then merge the maps.
///
/// Use [`std::thread::scope`] so each worker can borrow `&[u8]` slices into `data`
/// with no `Arc` and no `'static` bound — the scope guarantees the threads finish
/// before `data` could be dropped. Merge with [`Stats::merge`].
pub fn run_parallel(data: &[u8], threads: usize) -> BTreeMap<Vec<u8>, Stats> {
    if threads <= 1 {
        return run_sequential(data);
    }

    let chunks = split_chunks(data, threads);

    let partials = std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(|| aggregate(chunk)))
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });

    let mut global: FastMap<&[u8], Stats> = FastMap::default();
    for partial in partials {
        for (name, stats) in partial {
            global.entry(name).or_default().merge(&stats);
        }
    }

    into_sorted(global)
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
