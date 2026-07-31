//! SSTables — **S**orted **S**tring **Table**s: immutable, sorted, on-disk files
//! (Pills 7–8). This is where data lives once it outgrows the memtable.
//!
//! An SSTable is written once and never modified. Its layout, in three regions:
//!
//! ```text
//! [ data   ]  every record, in ascending key order, back-to-back (record.encode())
//! [ index  ]  one entry per record: [klen u32][key][offset u64][len u32]
//! [ footer ]  [index_offset u64][index_len u64][count u64][max_seq u64][magic u32]
//! ```
//!
//! The **index** is the point of the format: it's a sorted key→location map that
//! lets `get` binary-search for a key and then do a single positioned read
//! (`pread`) of just that record — no scanning the whole file (Pill 7). Because
//! the file is immutable, the index never goes stale. The **footer** is a
//! fixed-size trailer read first, so the reader can find the index without
//! scanning; its `magic` rejects foreign/corrupt files and `max_seq` lets
//! recovery restore the sequence high-water mark without reading the data.
//!
//! Positioned reads use `FileExt::read_exact_at` (`pread`): read at an offset
//! without a seek, through the OS page cache (Pill 12). That's Unix-only, which
//! is fine for this course (macOS/Linux).
//!
//! The path/id helpers and the [`IndexEntry`] type are **given**; `write`,
//! `open`, `get`, and `iter` are **your Steps 3 & 4**.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use crate::encoding::{Record, Seq, SST_MAGIC};
use crate::error::{Error, Result};

/// Size in bytes of the fixed footer: four `u64`s plus one `u32` magic.
pub const FOOTER_SIZE: u64 = 8 * 4 + 4;

/// One index slot: a key and where its record sits in the data region. **Given.**
#[derive(Clone, Debug)]
pub struct IndexEntry {
    /// The record's key (index is sorted ascending by this).
    pub key: Vec<u8>,
    /// Byte offset of the record within the file.
    pub offset: u64,
    /// Encoded length of the record in bytes.
    pub len: u32,
}

/// A read handle to an SSTable: the open file plus its in-memory index.
///
/// `open` loads the whole index into RAM (small — one entry per key); the data
/// stays on disk and is read on demand by `get`.
pub struct SsTable {
    /// This table's numeric id (also its filename stem); higher id = newer.
    pub id: u64,
    // `file` and `data_len` are consumed by your Step 3/4 code (`get`/`iter`);
    // allow(dead_code) keeps the scaffold warning-free until you wire them up.
    #[allow(dead_code)]
    file: File,
    index: Vec<IndexEntry>,
    /// Length of the data region == offset where the index starts. `iter` reads
    /// `[0, data_len)` as the records. Set this from the footer in `open`.
    #[allow(dead_code)]
    data_len: u64,
    max_seq: Seq,
}

/// The filesystem path of the SSTable with the given `id` in `dir`. **Given.**
///
/// Zero-padded so a lexical directory listing is also numeric (id) order.
pub fn sst_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id:010}.sst"))
}

/// Parse an SSTable id back out of a file name like `0000000007.sst`. **Given.**
/// Returns `None` for anything that isn't one of ours.
pub fn parse_sst_id(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".sst")?;
    stem.parse().ok()
}

impl SsTable {
    /// Write `records` (which **must already be sorted ascending by key**, with
    /// no duplicate keys) as a new SSTable, atomically. **Step 3a — your code.**
    ///
    /// Do it crash-safely (Pill 9):
    /// 1. Build the file body in memory (or a `BufWriter`): concatenate every
    ///    `record.encode()`, remembering each record's `offset` and `len` to fill
    ///    an [`IndexEntry`]; track the maximum `seq` seen.
    /// 2. Append the index region: for each entry, `[klen u32][key][offset u64][len u32]`.
    /// 3. Append the footer: `[index_offset u64][index_len u64][count u64][max_seq u64][SST_MAGIC u32]`.
    /// 4. Write it all to a **temporary** file (`sst_path(...)` + `.tmp`),
    ///    `sync_all` it, then `rename` it onto the final `sst_path(dir, id)`, then
    ///    [`crate::encoding::sync_dir`] the directory. The rename is atomic, so a
    ///    crash leaves either no file or a complete one — never a half-written
    ///    SSTable a reader could trip over.
    ///
    /// Return the final path.
    pub fn write(dir: &Path, id: u64, records: &[Record]) -> Result<PathBuf> {
        let mut data = Vec::new();
        let mut index: Vec<IndexEntry> = Vec::new();
        let mut max_seq: Seq = 0;

        for record in records {
            let encoded = record.encode();
            let offset = data.len() as u64;
            let len = encoded.len() as u32;

            index.push(IndexEntry {
                key: record.key.clone(),
                offset,
                len,
            });
            max_seq = max_seq.max(record.seq);
            data.extend_from_slice(&encoded);
        }

        let index_offset = data.len() as u64;

        for entry in &index {
            data.extend_from_slice(&(entry.key.len() as u32).to_le_bytes());
            data.extend_from_slice(&entry.key);
            data.extend_from_slice(&entry.offset.to_le_bytes());
            data.extend_from_slice(&entry.len.to_le_bytes());
        }

        let index_len = data.len() as u64 - index_offset;

        data.extend_from_slice(&index_offset.to_le_bytes()); // u64: where the index starts
        data.extend_from_slice(&index_len.to_le_bytes()); // u64: how long the index is
        data.extend_from_slice(&(index.len() as u64).to_le_bytes()); // u64: record count
        data.extend_from_slice(&max_seq.to_le_bytes()); // u8: high-water seq
        data.extend_from_slice(&SST_MAGIC.to_le_bytes()); // u32: format marker

        let final_path = sst_path(dir, id);
        let tmp_path = final_path.with_extension("sst.tmp");

        // 1. Write the whole image to the temp file and fsync its contents.
        let mut file = File::create(&tmp_path)?;
        file.write_all(&data)?;
        file.sync_all()?;

        // 2. Atomically swap it into place under the real name.
        std::fs::rename(&tmp_path, &final_path)?;

        crate::encoding::sync_dir(dir)?;

        Ok(final_path)
    }

    /// Open an SSTable for reading: validate the footer and load the index.
    /// **Step 3b — your code.**
    ///
    /// 1. Open the file; read its length from `metadata`.
    /// 2. `read_exact_at` the last [`FOOTER_SIZE`] bytes; parse the five fields;
    ///    check `magic == SST_MAGIC` (else `Err(Error::Corrupt(..))`).
    /// 3. `read_exact_at` `index_len` bytes starting at `index_offset`; parse them
    ///    into a `Vec<IndexEntry>` (it's already sorted by key on disk).
    /// 4. Return the [`SsTable`] holding the file, the index, and `max_seq`.
    pub fn open(path: &Path, id: u64) -> Result<SsTable> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();

        if file_len < FOOTER_SIZE {
            return Err(Error::Corrupt(format!(
                "file {} is only {file_len} bytes, shorter than the {FOOTER_SIZE}-byte footer",
                path.display()
            )));
        }

        // The footer is the last FOOTER_SIZE bytes; read them with the given helper.
        let footer = read_exact_at(&file, file_len - FOOTER_SIZE, FOOTER_SIZE as usize)?;

        let index_offset = u64::from_le_bytes(footer[0..8].try_into().unwrap());
        let index_len = u64::from_le_bytes(footer[8..16].try_into().unwrap());
        let count = u64::from_le_bytes(footer[16..24].try_into().unwrap());
        let max_seq = u64::from_le_bytes(footer[24..32].try_into().unwrap());
        let magic = u32::from_le_bytes(footer[32..36].try_into().unwrap());

        if magic != SST_MAGIC {
            return Err(Error::Corrupt(format!(
                "bad magic {magic:#010x} in {} (expected {SST_MAGIC:#010x})",
                path.display()
            )));
        }

        let index_bytes = read_exact_at(&file, index_offset, index_len as usize)?;

        let mut index: Vec<IndexEntry> = Vec::with_capacity(count as usize);
        let mut pos = 0;
        while pos < index_bytes.len() {
            let klen = u32::from_le_bytes(index_bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            let key = index_bytes[pos..pos + klen].to_vec();
            pos += klen;
            let offset = u64::from_le_bytes(index_bytes[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let len = u32::from_le_bytes(index_bytes[pos..pos + 4].try_into().unwrap());
            pos += 4;

            index.push(IndexEntry { key, offset, len });
        }

        Ok(SsTable {
            id,
            file,
            index,
            data_len: index_offset,
            max_seq,
        })
    }

    /// Look up `key`: return its newest record in *this* table, or `None`.
    /// **Step 4 — your code.**
    ///
    /// Binary-search `self.index` by key (`slice::binary_search_by`). On a hit,
    /// `read_exact_at` exactly `entry.len` bytes at `entry.offset`, then
    /// `Record::decode` them and return the record. On a miss, `None`. This is
    /// one `pread` of one record — the payoff of keeping a sorted index.
    ///
    /// Note this returns the raw [`Record`] (which may be a tombstone); it's the
    /// [`crate::db::Db`] read path that decides a tombstone means "not found".
    pub fn get(&self, key: &[u8]) -> Result<Option<Record>> {
        // Compare entry-vs-needle, in that order: binary_search_by wants
        // Less when the probed element sorts before the target
        let slot = self
            .index
            .binary_search_by(|entry| entry.key.as_slice().cmp(key));

        let entry = match slot {
            Ok(i) => &self.index[i],
            Err(_) => return Ok(None), // key isn't in this table
        };

        let bytes = self.read_at(entry.offset, entry.len as usize)?;

        let (record, _consumed) = Record::decode(&bytes).ok_or_else(|| {
            Error::Corrupt(format!(
                "bad record at offset {} in sstable {}",
                entry.offset, self.id
            ))
        })?;

        Ok(Some(record))
    }

    /// Every record in the table, in key order. Used by compaction and scans.
    /// **Step 3c — your code.**
    ///
    /// `read_exact_at` the whole data region (`[0, self.data_len)` — the index
    /// starts right after the data) into one buffer, then walk it with
    /// `Record::decode`, advancing by the reported consumed length each time
    /// until the buffer is exhausted.
    pub fn iter(&self) -> Result<Vec<Record>> {
        let data = self.read_at(0, self.data_len as usize)?;

        let mut records = Vec::with_capacity(self.index.len());
        let mut pos = 0;
        while pos < data.len() {
            let (record, consumed) = Record::decode(&data[pos..]).ok_or_else(|| {
                Error::Corrupt(format!("bad record at offset {pos} in sstable {}", self.id))
            })?;
            records.push(record);
            pos += consumed; // decode reports exactly how many bytes it ate
        }

        Ok(records)
    }

    /// The highest sequence number stored in this table. **Given** (from the
    /// footer). Recovery folds this into the global high-water mark so sequence
    /// numbers keep increasing across restarts even after the WAL is reset.
    pub fn max_seq(&self) -> Seq {
        self.max_seq
    }

    /// Number of records (index entries) in the table. **Given.**
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the table is empty. **Given.**
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// A positioned read of exactly `len` bytes at `offset` (`pread`). **Given
    /// helper** — use it from `get`/`iter`/`open`. Wraps a short read as a
    /// `Corrupt` error, since a well-formed SSTable is never shorter than its own
    /// footer says.
    pub(crate) fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        read_exact_at(&self.file, offset, len)
    }
}

/// Read exactly `len` bytes at `offset`, erroring if the file is too short.
/// **Given** free function so `SsTable::open` (which has no `self` yet) can use
/// it too.
pub(crate) fn read_exact_at(file: &File, offset: u64, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    file.read_exact_at(&mut buf, offset)
        .map_err(|_| Error::Corrupt(format!("short read of {len} bytes at offset {offset}")))?;
    Ok(buf)
}
