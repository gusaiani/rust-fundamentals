//! The write-ahead log (Pills 3–4).
//!
//! Before a write is applied to the memtable, it is appended to this log and the
//! log is `fsync`'d. That ordering — **log, sync, then apply** — is what makes
//! the store crash-safe: the memtable lives only in RAM, so if the process dies,
//! recovery replays the WAL to rebuild it. A write is "durable" the instant its
//! WAL frame is on disk, not when it later reaches an SSTable.
//!
//! Each record is stored as a **frame**:
//!
//! ```text
//! crc : u32   CRC-32 of the payload (Pill 4)
//! len : u32   payload length in bytes
//! payload     record.encode()  (len bytes)
//! ```
//!
//! The CRC + length is what lets recovery detect a **torn write**: if the
//! machine died mid-append, the last frame is short or its bytes don't match the
//! CRC. Replay stops cleanly at the first bad frame and keeps everything before
//! it — the classic "recover up to the last good record" guarantee.
//!
//! `open` and `reset` are **given**; `append`, `sync`, and `replay` are **your
//! Step 2**.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::encoding::Record;
use crate::error::Result;

/// An append-only log file plus its path (so it can be truncated on flush).
pub struct Wal {
    file: File,
    path: PathBuf,
}

impl Wal {
    /// Open (creating if absent) the WAL at `path` for appending. **Given.**
    ///
    /// Opened read+write with append semantics so every write goes to the end.
    pub fn open(path: &Path) -> Result<Wal> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;
        Ok(Wal {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Append one record as a framed `[crc][len][payload]` entry.
    ///
    /// **Step 2a — your code.** Encode the record, compute
    /// [`crate::encoding::crc32`] over the payload, and write the three parts
    /// (all little-endian) with a single `write_all` per part (or build one
    /// buffer and write it once — fewer syscalls). Do **not** `fsync` here;
    /// durability is a separate, explicit [`Wal::sync`] call so the caller can
    /// batch appends before paying for one sync.
    pub fn append(&mut self, record: &Record) -> Result<()> {
        let payload = record.encode();
        let crc = crate::encoding::crc32(&payload);

        // One contiguous frame: crc, then len, then payload - all little-endian.
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&crc.to_le_bytes()); // u32
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // u32
        frame.extend_from_slice(&payload);

        self.file.write_all(&frame)?;
        Ok(())
    }

    /// Flush and `fsync` the log so every appended frame is durable on the
    /// physical device.
    ///
    /// **Step 2b — your code.** `flush` pushes the userspace buffer to the OS;
    /// `sync_all` (`fsync`) forces the OS page cache to the disk. Both are needed
    /// — `write` alone leaves the data in a cache that a power cut erases (Pill
    /// 2). This is the call that actually makes a `put` durable.
    pub fn sync(&mut self) -> Result<()> {
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(())
    }

    /// Read back every intact record, in write order, stopping at the first torn
    /// or corrupt frame. **Step 2c — your code.**
    ///
    /// Read the whole file into a buffer and walk it frame by frame:
    /// 1. Need at least 8 bytes for `[crc][len]`; if fewer remain, stop.
    /// 2. Read `len`; if fewer than `len` payload bytes remain, stop (torn tail).
    /// 3. Compute the CRC of those `len` bytes; if it doesn't match the stored
    ///    `crc`, stop (corrupt tail).
    /// 4. `Record::decode` the payload; push it; advance past the frame.
    ///
    /// "Stop" always means *return what you have so far* — a partial last frame
    /// after a crash is expected and must not be an error. This is a `&Path`
    /// associated function (no `&self`) because recovery calls it before there's
    /// a live `Wal` to append to.
    pub fn replay(path: &Path) -> Result<Vec<Record>> {
        let mut file = match File::open(path) {
            Ok(f) => f,
            // No WAL yet ⇒ nothing to recover.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let mut records = Vec::new();
        let mut pos = 0;

        loop {
            // 1. Need 8 bytes for the [crc][len] header; if fewer remain, we're done
            let Some(header) = buf.get(pos..pos + 8) else {
                break;
            };
            let crc = u32::from_le_bytes(header[0..4].try_into().unwrap());
            let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;

            // 2. Need `len` payload bytes; a short tail is a torn write - stop
            let Some(payload) = buf.get(pos + 8..pos + 8 + len) else {
                break;
            };

            // 3. Recompute the CRC; a mismatch is a corrupt tail - stop.
            if crate::encoding::crc32(payload) != crc {
                break;
            }

            // 4. Decode the record; a decode failure is also a torn frame - stop.
            let Some((record, _consumed)) = Record::decode(payload) else {
                break;
            };
            records.push(record);

            // Advance past this whole frame: 8-byte header + len payload bytes.
            pos += 8 + len;
        }

        Ok(records)
    }

    /// Truncate the log back to empty and `fsync` it. **Given.**
    ///
    /// Called *after* a flush has durably written the memtable's contents into a
    /// new SSTable: once that data lives on disk, the WAL frames that carried it
    /// are redundant and the log can be reset so it doesn't grow forever. The
    /// ordering (flush first, reset second) is the durability rule of Pill 9 —
    /// reset too early and a crash loses data.
    pub fn reset(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.sync_all()?;
        // Re-open so the append cursor is back at offset 0 on all platforms.
        self.file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.path)?;
        Ok(())
    }
}
