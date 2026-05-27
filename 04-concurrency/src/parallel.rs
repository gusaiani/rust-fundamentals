//! Hand-rolled map-reduce with scoped threads — the core of this module.
//!
//! Split the buffer into `n` newline-aligned chunks, give each chunk its own
//! thread and its own local `Stats` (no sharing, no `Mutex`), then merge the
//! locals once at the end. This is Pill 7 written out by hand so you see what
//! `rayon` does for you in `rayon_impl.rs`.

use crate::sequential::analyze_sequential;
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
    let n = n.max(1); // n == 0 would divide by zero below; treat it as "1 chunk"
    if data.is_empty() {
        return Vec::new();
    }

    // Byte offsets where chunks split. Always begins at 0 and will end at
    // data.len(); the interior boundaries get snapped to newline edges next.
    let mut bounds = vec![0usize];

    for i in 1..n {
        // Rough split point — almost certainly mid-line.
        let raw = i * data.len() / n;

        // Walk forward to the next newline so we cut on a record boundary.
        let mut end = raw;
        while end < data.len() && data[end] != b'\n' {
            end += 1;
        }
        // Step past the '\n' itself so the newline stays with the line it ends.
        if end < data.len() {
            end += 1;
        }
        bounds.push(end);
    }
    bounds.push(data.len()); // last chunk always runs to the end

    bounds
        .windows(2)
        .map(|pair| &data[pair[0]..pair[1]])
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

/// Map-reduce the buffer across `n_threads` scoped threads.
pub fn analyze_parallel(data: &[u8], n_threads: usize) -> Stats {
    let chunks = split_into_chunks(data, n_threads);

    std::thread::scope(|s| {
        // map: spawn one thread per chunk, each returns its own local Stats
        let handles: Vec<_> = chunks
            .iter()
            .map(|chunk| s.spawn(|| analyze_sequential(chunk)))
            .collect();

        // reduce: associative merge of the locals — no Mutex anywhere
        let mut acc = Stats::default();
        for h in handles {
            acc.merge(h.join().unwrap());
        }
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*; // pulls split_into_chunks into scope

    /// Concatenate the chunks back into one buffer.
    fn concat(chunks: &[&[u8]]) -> Vec<u8> {
        chunks.iter().flat_map(|c| c.iter().copied()).collect()
    }

    #[test]
    fn chunks_cover_every_byte() {
        let data = b"aaa\nbbb\nccc\n".as_slice();
        let chunks = split_into_chunks(data, 3);
        // Round-trip: no byte lost, none double-counted.
        assert_eq!(concat(&chunks), data);
    }
}
