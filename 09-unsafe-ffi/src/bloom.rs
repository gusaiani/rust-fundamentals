//! The safe Bloom filter core — pure, `unsafe`-free Rust (Pills 1–3, 10).
//!
//! This is the layer with *no* raw pointers and *no* `unsafe`: a normal Rust
//! data structure you'd be happy to publish on its own. Everything dangerous
//! lives one module over in [`crate::ffi`]. Keeping the core safe and complete
//! first — and only then wrapping it — is the single most important habit in
//! FFI work: you debug the logic in safe Rust, where the compiler still has
//! your back, before you expose it over a boundary where it doesn't.

use crate::hash;

/// Magic bytes and version for the [`Bloom::to_bytes`] / [`Bloom::from_bytes`]
/// wire format, so a corrupt or foreign buffer is rejected instead of
/// reinterpreted (Pill 8).
const MAGIC: [u8; 4] = *b"CBLM";
const VERSION: u8 = 1;

/// A classic Bloom filter: a bit array plus `k` hash probes per item.
///
/// The bits are packed into `u64` words — 64 bits each — so `num_bits` is always
/// rounded up to a multiple of 64 and `bits.len() == num_bits / 64`.
pub struct Bloom {
    /// The bit array, packed 64 bits per word.
    bits: Vec<u64>,
    /// `m` — the number of usable bits (`== bits.len() * 64`).
    num_bits: usize,
    /// `k` — the number of hash probes per item.
    num_hashes: u32,
    /// Count of `add` calls, for stats/reporting. Not used by the algorithm.
    inserted: u64,
}

impl Bloom {
    /// Build a filter sized for `expected_items` insertions at a target
    /// `fp_rate` false-positive probability (e.g. `0.01` for 1%).
    ///
    /// The optimal sizing (derivation in Pill 2) is:
    ///
    /// ```text
    /// m = ceil( -(n * ln p) / (ln 2)^2 )      // bits
    /// k = round( (m / n) * ln 2 )             // probes
    /// ```
    ///
    /// Round `m` up to a multiple of 64 for word packing, and clamp both `m` and
    /// `k` to at least 1 so degenerate inputs (`n = 0`) still yield a usable
    /// filter. Then delegate to [`Bloom::with_params`].
    pub fn new(expected_items: usize, fp_rate: f64) -> Bloom {
        // Cast the count to float for the sizing math; ln(2)^2 is the standard denominator
        let n = expected_items.max(1) as f64; // clamp n≥1 so n=0 doesn't divide by zero
        let ln2 = std::f64::consts::LN_2;
        let m_float = -(n * fp_rate.ln()) / (ln2 * ln2);

        // Round m up to the nearest multiple of 64 so it packs evenly into u64 words.
        let m = (m_float.ceil() as usize).div_ceil(64) * 64;

        // Optimal probe count: k = (m / n) * ln 2, rounded to the nearest whole probe.
        let k = ((m as f64 / n) * ln2).round() as usize;

        Self::with_params(m, k as u32)
    }

    /// Build a filter with an explicit bit count and probe count.
    ///
    /// `num_bits` is rounded up to a multiple of 64. This is the low-level
    /// constructor [`Bloom::new`] and [`Bloom::from_bytes`] funnel through; it's
    /// given so you have a known-good way to allocate the word array.
    pub fn with_params(num_bits: usize, num_hashes: u32) -> Bloom {
        let words = num_bits.div_ceil(64).max(1);
        Bloom {
            bits: vec![0; words],
            num_bits: words * 64,
            num_hashes: num_hashes.max(1),
            inserted: 0,
        }
    }

    /// Set the bit at `index` (panics in debug if out of range — callers reduce
    /// modulo `num_bits` first). Given: this is the word/bit split that defines
    /// the packed layout.
    fn set_bit(&mut self, index: usize) {
        self.bits[index / 64] |= 1u64 << (index % 64);
    }

    /// Test the bit at `index`. Given — the read counterpart of [`set_bit`].
    fn get_bit(&self, index: usize) -> bool {
        (self.bits[index / 64] >> (index % 64)) & 1 == 1
    }

    /// Insert `item`: set the `k` bits it hashes to.
    pub fn add(&mut self, item: &[u8]) {
        let (h1, h2) = hash::hash_pair(item);
        for i in 0..self.num_hashes {
            let index = hash::bit_index(h1, h2, i, self.num_bits);
            self.set_bit(index);
        }
        self.inserted += 1;
    }

    /// Query `item`: `true` if it *might* be present, `false` if it's
    /// *definitely* absent.
    pub fn contains(&self, item: &[u8]) -> bool {
        let (h1, h2) = hash::hash_pair(item);
        (0..self.num_hashes).all(|i| {
            let index = hash::bit_index(h1, h2, i, self.num_bits);
            self.get_bit(index)
        })
    }

    /// `m`, the number of bits in the array.
    pub fn num_bits(&self) -> usize {
        self.num_bits
    }

    /// `k`, the number of probes per item.
    pub fn num_hashes(&self) -> u32 {
        self.num_hashes
    }

    /// How many items have been `add`ed (a plain counter, not a set size).
    pub fn approx_items(&self) -> u64 {
        self.inserted
    }

    /// Serialize to a self-describing byte buffer.
    ///
    /// Layout: `MAGIC` (4 bytes), `VERSION` (1), `num_hashes` (u32 LE),
    /// `inserted` (u64 LE), `num_bits` (u64 LE), then the `bits` words as little-
    /// endian `u64`s. This is the buffer [`crate::ffi::cbloom_serialize`] hands
    /// to C, so keep it flat and endianness-explicit.

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.push(VERSION);
        buf.extend_from_slice(&self.num_hashes.to_le_bytes());
        buf.extend_from_slice(&self.inserted.to_le_bytes());
        buf.extend_from_slice(&(self.num_bits as u64).to_le_bytes());
        buf
    }

    /// Reconstruct a filter from [`to_bytes`] output, or `None` if the buffer is
    /// truncated, has the wrong magic, or an unknown version.
    ///
    /// Validate *before* trusting any length — this is the deserialization that,
    /// on the C side, runs on bytes from a file or socket (Pill 8).
    ///
    /// TODO (step 3): check `MAGIC`/`VERSION`, read the header back with
    /// `u64::from_le_bytes`/`u32::from_le_bytes`, verify the remaining length
    /// matches `num_bits / 64` words, then read the words.
    pub fn from_bytes(data: &[u8]) -> Option<Bloom> {
        let _ = (data, MAGIC, VERSION);
        todo!("bloom.rs: validate magic/version/length, then read header + words back")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizing_is_reasonable() {
        // 1000 items at 1% should want ~9585 bits and 7 probes (textbook values).
        let b = Bloom::new(1000, 0.01);
        assert!(b.num_bits() >= 9585 && b.num_bits() <= 9585 + 64);
        assert_eq!(b.num_hashes(), 7);
        assert_eq!(b.num_bits() % 64, 0, "bits must be word-aligned");
    }

    #[test]
    fn no_false_negatives() {
        // The defining guarantee: everything inserted must report present.
        let mut b = Bloom::new(1000, 0.01);
        let items: Vec<String> = (0..1000).map(|i| format!("item-{i}")).collect();
        for it in &items {
            b.add(it.as_bytes());
        }
        for it in &items {
            assert!(b.contains(it.as_bytes()), "false negative for {it}");
        }
        assert_eq!(b.approx_items(), 1000);
    }

    #[test]
    fn absent_items_mostly_absent() {
        // False positives are allowed but should be rare at the configured rate.
        let mut b = Bloom::new(1000, 0.01);
        for i in 0..1000 {
            b.add(format!("present-{i}").as_bytes());
        }
        let fps = (0..1000)
            .filter(|i| b.contains(format!("absent-{i}").as_bytes()))
            .count();
        assert!(fps < 50, "false-positive rate way over target: {fps}/1000");
    }

    #[test]
    fn roundtrips_through_bytes() {
        let mut b = Bloom::new(500, 0.02);
        for i in 0..500 {
            b.add(format!("k{i}").as_bytes());
        }
        let bytes = b.to_bytes();
        let restored = Bloom::from_bytes(&bytes).expect("valid buffer");
        assert_eq!(restored.num_bits(), b.num_bits());
        assert_eq!(restored.num_hashes(), b.num_hashes());
        for i in 0..500 {
            assert!(restored.contains(format!("k{i}").as_bytes()));
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(Bloom::from_bytes(b"not a filter").is_none());
        assert!(Bloom::from_bytes(&[]).is_none());
    }
}
