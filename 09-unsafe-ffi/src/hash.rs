//! Hashing for the filter — safe, `unsafe`-free Rust (Pills 3 & 10).
//!
//! A Bloom filter needs `k` independent-looking bit positions per item. Hashing
//! the input `k` separate times is wasteful; the standard trick (Kirsch &
//! Mitzenmacher, 2006) is to compute **two** hashes once and combine them:
//!
//! ```text
//! index_i = (h1 + i * h2) mod m      for i in 0..k
//! ```
//!
//! That gives `k` well-distributed positions from two base hashes with no loss
//! in false-positive rate. We use FNV-1a — a tiny, fast, non-cryptographic hash
//! that's a few lines and needs no dependency. (DoS resistance is irrelevant
//! here: the filter is a local data structure, not exposed to adversarial keys.)

/// FNV-1a 64-bit offset basis and prime (the published constants).
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64-bit hash of `data`, starting from a caller-supplied basis.
///
/// Two calls with two different starting bases give us the two independent
/// hashes the combining trick needs.
fn fnv1a(data: &[u8], basis: u64) -> u64 {
    let mut hash = basis;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The two base hashes for an item: `(h1, h2)`.
///
/// `h1` is plain FNV-1a from the standard offset basis. `h2` is a second,
/// independent hash; force it odd so that `i * h2` keeps stepping across the
/// whole bit array instead of getting stuck on a small cycle.
pub fn hash_pair(item: &[u8]) -> (u64, u64) {
    let h1 = fnv1a(item, FNV_OFFSET);
    let h2 = fnv1a(item, FNV_PRIME) | 1;
    (h1, h2)
}

/// The `i`-th bit position for an item, from its two base hashes.
///
/// Implements `(h1 + i * h2) mod num_bits`. Use wrapping arithmetic on the `u64`
/// combination so a large `i * h2` can't overflow-panic in debug builds, then
/// reduce into `0..num_bits`.
pub fn bit_index(h1: u64, h2: u64, i: u32, num_bits: usize) -> usize {
    let combined = h1.wrapping_add((i as u64).wrapping_mul(h2));
    (combined % num_bits as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_matches_known_vector() {
        // FNV-1a 64 of the empty input is the offset basis itself.
        assert_eq!(fnv1a(b"", FNV_OFFSET), FNV_OFFSET);
        // Published FNV-1a 64 test vector for "a".
        assert_eq!(fnv1a(b"a", FNV_OFFSET), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn pair_is_deterministic_and_distinct() {
        let (a1, a2) = hash_pair(b"hello");
        let (b1, b2) = hash_pair(b"hello");
        assert_eq!((a1, a2), (b1, b2), "hashing must be deterministic");
        assert_ne!(a1, a2, "the two base hashes should differ");
        assert_eq!(a2 & 1, 1, "h2 must be forced odd");
    }

    #[test]
    fn indices_land_in_range() {
        let (h1, h2) = hash_pair(b"some key");
        let m = 1024;
        for i in 0..8 {
            assert!(bit_index(h1, h2, i, m) < m);
        }
    }

    #[test]
    fn different_inputs_diverge() {
        assert_ne!(hash_pair(b"cat"), hash_pair(b"dog"));
    }
}
