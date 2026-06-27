//! Getting the bytes in, and splitting them for parallelism (Pills 5 & 12).

use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;

/// Memory-map a file read-only and hand back the `Mmap` (deref to `&[u8]`).
///
/// **Given.** This is the Pill 5 move: no `read` syscall in the hot loop, no copy
/// into a userspace buffer — the kernel faults pages in from the page cache as
/// you touch the slice.
///
/// # Safety
///
/// `Mmap::map` is `unsafe` because the mapping aliases the file: if another
/// process truncates or rewrites it while the map is live, you get undefined
/// behavior. For this workload — a file you generated and only read — that can't
/// happen, but the contract is stated, not hidden (Module 7's discipline).
pub fn map_file(path: &Path) -> io::Result<Mmap> {
    let file = File::open(path)?;
    // SAFETY: the file is opened read-only and not modified for the map's lifetime.
    unsafe { Mmap::map(&file) }
}

/// Split `data` into (up to) `n` contiguous chunks, each ending on a `\n` so no
/// line is ever cut in half. The chunks tile `data` exactly: concatenated, they
/// reproduce the input.
///
/// Strategy: aim for `data.len() / n`-sized pieces, then walk each cut forward to
/// the next newline before slicing. The last chunk takes whatever remains.
///
/// TODO (step 7): implement the boundary-aligned split. See the step-7 hint for a
/// near-complete version. Edge cases to keep green: empty input -> no chunks;
/// `n == 1` -> the whole slice; `n` larger than the line count -> fewer, non-empty
/// chunks (never empty ones).
pub fn split_chunks(data: &[u8], n: usize) -> Vec<&[u8]> {
    let _ = (data, n);
    todo!("tile the buffer into n newline-aligned chunks — see step 7 hint")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(split_chunks(b"", 4).is_empty());
    }

    #[test]
    fn single_chunk_is_whole_input() {
        let data = b"a;1.0\nb;2.0\n";
        let chunks = split_chunks(data, 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data);
    }

    #[test]
    fn chunks_tile_the_input_exactly() {
        let data = b"alpha;1.0\nbravo;2.0\ncharlie;3.0\ndelta;4.0\necho;5.0\n";
        for n in 1..=8 {
            let chunks = split_chunks(data, n);
            // Reassembling the chunks reproduces the input.
            let rejoined: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
            assert_eq!(rejoined, data, "n={n}");
        }
    }

    #[test]
    fn every_chunk_ends_on_a_newline_and_is_nonempty() {
        let data = b"alpha;1.0\nbravo;2.0\ncharlie;3.0\ndelta;4.0\n";
        let chunks = split_chunks(data, 3);
        for c in &chunks {
            assert!(!c.is_empty());
            assert_eq!(*c.last().unwrap(), b'\n');
        }
    }

    #[test]
    fn never_splits_a_line() {
        let data = b"alpha;1.0\nbravo;2.0\ncharlie;3.0\n";
        // Each chunk must contain only complete lines: same number of ';' as '\n'.
        for c in split_chunks(data, 5) {
            let semis = c.iter().filter(|&&b| b == b';').count();
            let nls = c.iter().filter(|&&b| b == b'\n').count();
            assert_eq!(semis, nls);
        }
    }
}
