//! A fast, non-cryptographic hasher (Pill 8).
//!
//! `std`'s `HashMap` defaults to SipHash 1-3 — keyed and DoS-resistant, which is
//! the right default for untrusted keys but pure overhead here: the keys are
//! ~400 station names from a file you control. This is FxHash, the hasher rustc
//! uses internally — a multiply-and-xor per 8-byte word, a few cycles instead of
//! SipHash's many rounds.
//!
//! In production you'd depend on the `rustc-hash` crate; you build it once here
//! to see there is no magic. **Never** use this for internet-facing keys.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// FxHash's mixing constant (a large odd number with good bit diffusion).
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// A FxHash-style hasher: fold each input word into the state with rotate-xor
/// then multiply.
#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    /// Mix one 64-bit word into the running hash.
    ///
    /// TODO (step 3): `self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);`
    #[inline]
    fn add(&mut self, word: u64) {
        let _ = word;
        todo!("rotate_left(5) ^ word, then wrapping_mul(SEED) — see step 3 hint")
    }
}

impl Hasher for FxHasher {
    /// Feed bytes in 8-byte chunks (zero-padding the last partial chunk), mixing
    /// each into the state with [`FxHasher::add`].
    ///
    /// TODO (step 3): iterate `bytes.chunks(8)`, pack each into a `[u8; 8]`,
    /// `u64::from_le_bytes`, and `self.add(..)`. See the step-3 hint.
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let _ = bytes;
        todo!("hash bytes 8 at a time — see step 3 hint")
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// A `HashMap` using [`FxHasher`] instead of SipHash. Drop-in for the hot-loop
/// keyspace: `FastMap<&[u8], Stats>`.
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn hash_of(bytes: &[u8]) -> u64 {
        let mut h = FxHasher::default();
        h.write(bytes);
        h.finish()
    }

    #[test]
    fn deterministic() {
        assert_eq!(hash_of(b"Hamburg"), hash_of(b"Hamburg"));
    }

    #[test]
    fn distinguishes_distinct_keys() {
        assert_ne!(hash_of(b"Hamburg"), hash_of(b"Bulawayo"));
        assert_ne!(hash_of(b"Abha"), hash_of(b"Abidjan"));
    }

    #[test]
    fn empty_is_seed_independent_of_panic() {
        // An empty key must hash without touching `add`'s missing impl path
        // beyond the (legal) zero-chunk case; just assert it returns *something*.
        let h = hash_of(b"");
        let _ = h;
    }

    #[test]
    fn works_as_a_map() {
        let mut m: FastMap<&[u8], i32> = FastMap::default();
        m.insert(b"a", 1);
        m.insert(b"b", 2);
        *m.entry(b"a").or_default() += 10;
        assert_eq!(m.get(b"a".as_slice()), Some(&11));
        assert_eq!(m.get(b"b".as_slice()), Some(&2));
    }

    // Hash trait import kept meaningful so the file documents the contract.
    #[allow(dead_code)]
    fn _assert_key_is_hashable<T: Hash>() {}
}
