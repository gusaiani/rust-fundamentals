//! Durability: a point-in-time snapshot of the keyspace (Pill 10), loaded back
//! with a memory-mapped read (Pill 11).
//!
//! The little-endian read/write helpers and the file format are **given**; the
//! `save` (step 7) and `load` (step 8) bodies are yours.
//!
//! ## File format (`*.rdb`)
//!
//! ```text
//! magic:  8 bytes  = b"RUDISDB1"
//! then, repeated until EOF, one record per key:
//!   key_len:    u32 (LE)
//!   key:        key_len bytes
//!   val_len:    u32 (LE)
//!   val:        val_len bytes
//!   expires_at: u64 (LE)   (0 = no expiry; absolute now_ms deadline otherwise)
//! ```
//!
//! It's a flat, length-prefixed log — the same framing idea as RESP, applied to
//! a file. No compression, no checksums; adding a CRC is a stretch goal.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

use crate::store::Store;
use crate::{Error, Result};

/// 8-byte file signature. A load that doesn't see this rejects the file rather
/// than parsing garbage.
pub const MAGIC: &[u8; 8] = b"RUDISDB1";

// ---- little-endian helpers (given) -----------------------------------------

/// Append a u32 to `out` in little-endian order.
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Append a u64 to `out` in little-endian order.
fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Read a u32 at `off`, returning `(value, new_offset)`, or `None` if `bytes`
/// is too short. Bounds-checking here is what makes parsing a *mapped* file
/// safe — a truncated file yields `None`, never a panic or out-of-bounds read.
fn get_u32(bytes: &[u8], off: usize) -> Option<(u32, usize)> {
    let end = off.checked_add(4)?;
    let slice = bytes.get(off..end)?;
    Some((u32::from_le_bytes(slice.try_into().unwrap()), end))
}

/// Read a u64 at `off`, returning `(value, new_offset)`, or `None` if too short.
fn get_u64(bytes: &[u8], off: usize) -> Option<(u64, usize)> {
    let end = off.checked_add(8)?;
    let slice = bytes.get(off..end)?;
    Some((u64::from_le_bytes(slice.try_into().unwrap()), end))
}

/// Read `len` bytes at `off`, returning `(slice, new_offset)`, or `None`.
fn get_bytes(bytes: &[u8], off: usize, len: usize) -> Option<(&[u8], usize)> {
    let end = off.checked_add(len)?;
    let slice = bytes.get(off..end)?;
    Some((slice, end))
}

// ---- save / load (STEPS 7 & 8 — implement these) ---------------------------

/// Write the whole keyspace to `path` atomically-enough for a learning server:
/// build the bytes in memory, write them, and `fsync`.
pub fn save(store: &Store, path: &Path) -> Result<()> {
    use std::io::Write;

    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);

    for (key, value, expires_at) in store.snapshot_iter() {
        put_u32(&mut buf, key.len() as u32);
        buf.extend_from_slice(key);
        put_u32(&mut buf, value.len() as u32);
        buf.extend_from_slice(value);
        put_u64(&mut buf, expires_at);
    }

    let mut f = File::create(path)?;
    f.write_all(&buf)?;
    f.sync_all()?;
    Ok(())
}

/// Load a snapshot into `store` by **memory-mapping** the file and parsing the
/// keyspace straight out of the mapping (Pill 11).
///
/// TODO (step 8):
///   1. `let file = File::open(path)?;`
///   2. `let mmap = unsafe { Mmap::map(&file)? };` — the `unsafe` is honest: the
///      borrow checker can't stop another process truncating the file while it's
///      mapped (which would SIGBUS). For a snapshot we load once at boot, it's
///      fine. `let bytes: &[u8] = &mmap;` — now treat the file as a slice.
///   3. Check the header: the first 8 bytes must equal `MAGIC`, else
///      `Err(Error::Snapshot("bad magic".into()))`. Start `off = 8`.
///   4. Loop while `off < bytes.len()`: use `get_u32`/`get_bytes`/`get_u64` to
///      read key_len, key, val_len, val, expires_at. Any `None` (truncated
///      record) → `Err(Error::Snapshot("truncated".into()))`. Otherwise
///      `store.load_entry(key.to_vec(), val.to_vec(), expires_at)` and advance
///      `off`.
///   No read() loop, no growing buffer: the OS faults pages in on demand as you
///   walk the slice — that's the mmap win for a load-once-then-scan workload.
pub fn load(path: &Path, store: &mut Store) -> Result<()> {
    let file = File::open(path)?;

    let mmap = unsafe { Mmap::map(&file)? };
    let bytes: &[u8] = &mmap;

    if bytes.len() < 8 || &bytes[..8] != MAGIC {
        return Err(Error::Snapshot("badmagic".into()));
    }

    let mut off = 8;

    while off < bytes.len() {
        let (key_len, off1) =
            get_u32(bytes, off).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (key, off2) = get_bytes(bytes, off1, key_len as usize)
            .ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (val_len, off3) =
            get_u32(bytes, off2).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (value, off4) = get_bytes(bytes, off3, val_len as usize)
            .ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (expires_at, off5) =
            get_u64(bytes, off4).ok_or_else(|| Error::Snapshot("truncated".into()))?;

        store.load_entry(key.to_vec(), value.to_vec(), expires_at);
        off = off5;
    }

    Ok(())
}
