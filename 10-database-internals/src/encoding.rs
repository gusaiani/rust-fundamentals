//! The on-disk vocabulary: the [`Record`] type, its wire format, and the CRC32
//! that guards every framed write (Pills 3–4).
//!
//! Everything the store persists — every WAL frame, every SSTable entry — is a
//! [`Record`]: a monotonically increasing sequence number, a key, and a value
//! that is either a real payload ([`ValueKind::Put`]) or a **tombstone**
//! ([`ValueKind::Delete`]). The sequence number is what makes ordering
//! unambiguous across the WAL, the memtable, and many SSTables: **higher `seq`
//! wins**, which is the whole basis of both the read path and compaction (and
//! the seed of real MVCC — Pill 13).
//!
//! The `crc32`, `sync_dir`, and the primitive layout constants are **given**.
//! The two functions that turn a `Record` into bytes and back —
//! [`Record::encode`] and [`Record::decode`] — are **your Step 1**: get these
//! right and the WAL and SSTable both fall into place, because they're the only
//! code that knows the byte layout of a record.

use std::fs;
use std::io;
use std::path::Path;

/// A sequence number: a global, monotonically increasing version stamp put on
/// every write. Never reused, never goes backwards (even across a restart —
/// recovery restores the high-water mark). Newer write ⇒ larger `seq`.
pub type Seq = u64;

/// The two things a key can map to on disk.
///
/// A delete is *not* the absence of a record — it's a real record carrying a
/// [`ValueKind::Delete`] tombstone, so that it can shadow an older value that
/// still lives in an older SSTable (Pill 6). Tombstones are only physically
/// dropped during a full compaction, when nothing older survives to un-shadow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// A live value.
    Put(Vec<u8>),
    /// A tombstone: this key is deleted as of this record's `seq`.
    Delete,
}

/// One logical write: `(seq, key) -> value-or-tombstone`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The version stamp for this write (see [`Seq`]).
    pub seq: Seq,
    /// The key. Arbitrary bytes; may be empty in theory but the store uses
    /// non-empty keys.
    pub key: Vec<u8>,
    /// The value or tombstone.
    pub value: ValueKind,
}

/// Tag byte written before the value: a live put.
pub const KIND_PUT: u8 = 0;
/// Tag byte written before the value: a tombstone.
pub const KIND_DELETE: u8 = 1;

/// Magic number in the SSTable footer, ASCII `"LSM1"`, so a foreign or damaged
/// file is rejected instead of misread (used in [`crate::sstable`]).
pub const SST_MAGIC: u32 = 0x4C_53_4D_31;

impl Record {
    /// Construct a put record.
    pub fn put(seq: Seq, key: Vec<u8>, value: Vec<u8>) -> Record {
        Record {
            seq,
            key,
            value: ValueKind::Put(value),
        }
    }

    /// Construct a tombstone record.
    pub fn tombstone(seq: Seq, key: Vec<u8>) -> Record {
        Record {
            seq,
            key,
            value: ValueKind::Delete,
        }
    }

    /// Serialize this record to a self-delimiting byte string.
    ///
    /// **Step 1a — your code.** The layout (all little-endian) is up to you, but
    /// it must be *self-delimiting* — [`Record::decode`] has to know where the
    /// record ends without any outside length. A layout that works:
    ///
    /// ```text
    /// seq   : u64
    /// kind  : u8            (KIND_PUT or KIND_DELETE)
    /// klen  : u32           key length
    /// key   : klen bytes
    /// vlen  : u32           value length (0 for a tombstone)
    /// value : vlen bytes    (absent for a tombstone)
    /// ```
    ///
    /// Return the bytes. `decode(encode(r)) == (r, encode(r).len())` must hold —
    /// the round-trip test in this module checks exactly that.
    pub fn encode(&self) -> Vec<u8> {
        let (kind, value): (u8, &[u8]) = match &self.value {
            ValueKind::Put(bytes) => (KIND_PUT, bytes),
            ValueKind::Delete => (KIND_DELETE, &[]),
        };

        let mut buf = Vec::new();
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.push(kind);
        buf.extend_from_slice(&(self.key.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.key);
        buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
        buf.extend_from_slice(value);
        buf
    }

    /// Parse one record from the front of `buf`.
    ///
    /// **Step 1b — your code.** Read exactly what [`Record::encode`] wrote, and
    /// return the [`Record`] together with **how many bytes it consumed**, so a
    /// caller can walk a buffer of many records back-to-back (that's how the WAL
    /// replays and how an SSTable iterates). Return `None` if `buf` is too short
    /// for a complete record or the `kind` byte is neither `KIND_PUT` nor
    /// `KIND_DELETE` — a short/garbage tail must never panic (a torn WAL write
    /// looks exactly like this, and recovery relies on it being a clean `None`).
    pub fn decode(buf: &[u8]) -> Option<(Record, usize)> {
        let mut pos = 0;

        let seq = u64::from_le_bytes(buf.get(pos..pos + 8)?.try_into().ok()?);
        pos += 8;

        let kind = *buf.get(pos)?;
        pos += 1;

        let klen = u32::from_le_bytes(buf.get(pos..pos + 4)?.try_into().ok()?) as usize;
        pos += 4;
        
        let key = buf.get(pos..pos + klen)?.to_vec();
        pos += klen;

        let vlen = u32::from_le_bytes(buf.get(pos..pos + 4)?.try_into().ok()?) as usize;
        pos += 4;
        let value_bytes = buf.get(pos..pos + vlen)?.to_vec();
        pos += vlen;

        let value = match kind {
            KIND_PUT => ValueKind::Put(value_bytes),
            KIND_DELETE => ValueKind::Delete,
            _ => return None,
        };

        Some((Record { seq, key, value }, pos))
    }
}

/// CRC-32 (IEEE 802.3, the zlib/gzip polynomial) of `data`. **Given.**
///
/// Every WAL frame stores the CRC of its payload so recovery can tell a
/// complete, intact record from a half-written tail after a crash (Pill 4). This
/// is the standard table-driven implementation; the table is built at compile
/// time by [`crc32_table`].
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC_TABLE[idx];
    }
    crc ^ 0xFFFF_FFFF
}

/// The 256-entry CRC-32 lookup table, one entry per byte value. **Given.**
static CRC_TABLE: [u32; 256] = crc32_table();

/// Build the CRC-32 table at compile time (`const fn`), so there's no runtime
/// initialization and no dependency. **Given.**
const fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB8_8320; // reflected 0x04C11DB7
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// `fsync` a **directory** so that a file created/renamed inside it is durable —
/// not just the file's contents, but the directory entry pointing at it.
/// **Given** (Pill 2).
///
/// This is the step everyone forgets: after you write and `fsync` a new SSTable
/// and `rename` it into place, the *name* only survives a crash if the directory
/// itself is synced. On Linux/macOS you open the directory and `sync_all` it.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    // A directory can be opened read-only and fsync'd; that flushes its entries.
    let f = fs::File::open(dir)?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_known_vector() {
        // The canonical CRC-32 check value for the ASCII string "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        // Empty input is 0 after the pre/post conditioning.
        assert_eq!(crc32(b""), 0x0000_0000);
    }

    #[test]
    fn record_round_trips() {
        let cases = [
            Record::put(1, b"alpha".to_vec(), b"one".to_vec()),
            Record::put(2, b"".to_vec(), b"empty-key".to_vec()),
            Record::put(3, b"big".to_vec(), vec![0xAB; 5000]),
            Record::tombstone(4, b"gone".to_vec()),
        ];
        for rec in cases {
            let bytes = rec.encode();
            let (back, consumed) = Record::decode(&bytes).expect("decode");
            assert_eq!(back, rec);
            assert_eq!(consumed, bytes.len(), "decode must report the exact length");
        }
    }

    #[test]
    fn decode_walks_a_concatenated_buffer() {
        // Two records back-to-back: decode must consume exactly the first, so the
        // caller can decode the second from the remainder. This is the WAL replay
        // and SSTable iteration invariant.
        let a = Record::put(1, b"a".to_vec(), b"1".to_vec());
        let b = Record::put(2, b"b".to_vec(), b"2".to_vec());
        let mut buf = a.encode();
        buf.extend_from_slice(&b.encode());

        let (ra, n) = Record::decode(&buf).expect("first");
        assert_eq!(ra, a);
        let (rb, _) = Record::decode(&buf[n..]).expect("second");
        assert_eq!(rb, b);
    }

    #[test]
    fn decode_rejects_a_short_tail() {
        // A truncated record (the torn-write case) must be a clean None, not a panic.
        let bytes = Record::put(7, b"key".to_vec(), b"value".to_vec()).encode();
        assert!(Record::decode(&bytes[..bytes.len() - 3]).is_none());
        assert!(Record::decode(&[]).is_none());
    }
}
