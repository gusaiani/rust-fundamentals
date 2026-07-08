//! The store itself — the [`Db`] that wires the WAL, the memtable, and the
//! SSTables into one crash-safe key-value engine (Pills 8–11).
//!
//! The moving parts and how a request flows through them:
//!
//! - **`put` / `delete`** — assign the next sequence number, append the record
//!   to the [`crate::wal::Wal`] and `sync` it (now durable), then insert it into
//!   the [`crate::memtable::MemTable`]. If the memtable has grown past the flush
//!   threshold, [`Db::flush`] writes it out as a new SSTable.
//! - **`get`** — check the memtable first (newest data), then each SSTable from
//!   newest to oldest; the **first** hit wins, and a tombstone hit means "not
//!   found". Sequence numbers guarantee this order is the correct one.
//! - **`open`** — recovery: load the existing SSTables, then replay the WAL to
//!   rebuild the memtable that was in RAM when the process last died (Pill 11).
//! - **`flush` / `compact`** — move data down the hierarchy durably: memtable →
//!   SSTable, then many SSTables → one (Pills 9–10).
//!
//! `open_with`'s directory scan, `scan`, `stats`, and `maybe_flush` are
//! **given**; `open`, `get`, `put`, `delete`, and `flush` are **your Steps 5–6**
//! (compaction is Step 7, in [`crate::compaction`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::compaction;
use crate::encoding::{self, Record, Seq, ValueKind};
use crate::error::Result;
use crate::memtable::MemTable;
use crate::sstable::{self, SsTable};
use crate::wal::Wal;

/// Tunables for the store. **Given.**
#[derive(Clone, Debug)]
pub struct Options {
    /// Flush the memtable to an SSTable once its estimated size reaches this many
    /// bytes. Small values make flushes frequent (handy for tests); real stores
    /// use tens of megabytes.
    pub memtable_flush_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options { memtable_flush_bytes: 1 << 20 } // 1 MiB
    }
}

/// A snapshot of the store's shape, for the CLI `stats` command. **Given.**
#[derive(Clone, Debug)]
pub struct Stats {
    /// Keys currently buffered in the memtable (not yet flushed).
    pub memtable_entries: usize,
    /// Estimated memtable size in bytes.
    pub memtable_bytes: usize,
    /// Number of on-disk SSTables.
    pub sstables: usize,
    /// Total records across all SSTables (includes shadowed versions/tombstones).
    pub sstable_records: usize,
    /// The next sequence number that will be assigned.
    pub next_seq: Seq,
}

/// An embedded, crash-safe, LSM-tree key-value store backed by a directory.
pub struct Db {
    dir: PathBuf,
    // `wal` and `opts` are read by your Step 5/6 code (recovery, put/delete,
    // maybe_flush); allow(dead_code) keeps the scaffold clean until then.
    #[allow(dead_code)]
    wal: Wal,
    mem: MemTable,
    /// Live SSTables, **newest first** (descending id). `get` walks this in order.
    sstables: Vec<SsTable>,
    /// The next sequence number to hand out.
    seq: Seq,
    /// The id to give the next SSTable written.
    next_sst_id: u64,
    #[allow(dead_code)]
    opts: Options,
}

impl Db {
    /// Open (creating if needed) the store rooted at `dir`, with default options.
    pub fn open(dir: impl AsRef<Path>) -> Result<Db> {
        Db::open_with(dir, Options::default())
    }

    /// Open the store with explicit [`Options`]. **Step 5 — your recovery code.**
    ///
    /// Most of the plumbing is given below (create the dir, list and open the
    /// existing SSTables, compute `next_sst_id`). **Your part is the recovery
    /// itself**, marked `TODO` in the body:
    ///
    /// 1. `Wal::replay` the `wal.log` in `dir` to get the records that were in
    ///    the memtable when the process last stopped.
    /// 2. Insert each into a fresh [`MemTable`] (newest-wins is automatic).
    /// 3. Open the live `Wal` for appending.
    /// 4. Set `seq` to **one past** the highest sequence number seen anywhere —
    ///    across both the replayed WAL records *and* every SSTable's
    ///    [`SsTable::max_seq`] — so sequence numbers never repeat or go backwards
    ///    across a restart (Pill 13).
    ///
    /// Assemble and return the [`Db`].
    pub fn open_with(dir: impl AsRef<Path>, opts: Options) -> Result<Db> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;

        // --- given: load the on-disk SSTables, newest (highest id) first ---
        let mut ids = list_sstable_ids(&dir)?;
        ids.sort_unstable();
        let mut sstables = Vec::new();
        for id in &ids {
            sstables.push(SsTable::open(&sstable::sst_path(&dir, *id), *id)?);
        }
        sstables.reverse(); // newest first
        let next_sst_id = ids.last().map(|m| m + 1).unwrap_or(0);
        let wal_path = dir.join("wal.log");

        // --- your Step 5: replay the WAL and restore the sequence high-water mark ---
        let _ = (&wal_path, &sstables, &opts, next_sst_id);
        todo!(
            "Step 5: replay wal.log into a fresh MemTable, open the live Wal, \
             set seq = 1 + max(seq across replayed records and every sstable.max_seq()), \
             then build the Db"
        )
    }

    /// Fetch the current value for `key`, or `None` if absent or deleted.
    /// **Step 6a — your code.**
    ///
    /// Check sources newest → oldest and stop at the first that knows the key:
    /// 1. the memtable ([`MemTable::get`]);
    /// 2. then each SSTable in `self.sstables` (already newest-first).
    ///
    /// The first source that contains `key` is authoritative: if that record is
    /// a [`ValueKind::Put`] return the bytes; if it's a [`ValueKind::Delete`]
    /// tombstone return `None` **without** consulting older tables. If nothing
    /// has the key, `None`.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let _ = key;
        todo!("Step 6a: memtable first, then sstables newest-first; first hit wins; tombstone => None")
    }

    /// Insert or overwrite `key -> value`. **Step 6b — your code.**
    ///
    /// The durability dance (Pill 2), in order:
    /// 1. take the next sequence number (`self.seq`, then increment it);
    /// 2. build a `Record::put(seq, key, value)`;
    /// 3. `self.wal.append(&record)` then `self.wal.sync()` — now it survives a
    ///    crash;
    /// 4. `self.mem.insert(record)`;
    /// 5. `self.maybe_flush()`.
    ///
    /// Do **not** reorder 3 and 4: applying to the memtable before the WAL is
    /// synced is the classic lost-write bug.
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        let _ = (key, value);
        todo!("Step 6b: next seq, WAL append+sync, memtable insert, maybe_flush")
    }

    /// Delete `key` by writing a tombstone. **Step 6c — your code.**
    ///
    /// Identical to [`Db::put`] but with `Record::tombstone(seq, key)`. You do
    /// *not* remove anything from disk here — the tombstone shadows older values
    /// until a compaction physically drops them (Pill 6).
    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
        let _ = key;
        todo!("Step 6c: next seq, WAL append+sync of a tombstone, memtable insert, maybe_flush")
    }

    /// Write the memtable out as a new immutable SSTable, then reset the WAL.
    /// **Step 6d — your code.**
    ///
    /// 1. If the memtable is empty, there's nothing to do — return.
    /// 2. Collect `self.mem.records()` (already key-sorted) into a `Vec`.
    /// 3. `SsTable::write(&self.dir, self.next_sst_id, &records)` — this fsyncs
    ///    the new file and its directory entry, so the data is now durable on
    ///    disk.
    /// 4. `SsTable::open` it and push it to the **front** of `self.sstables`
    ///    (it's the newest); bump `self.next_sst_id`.
    /// 5. `self.mem.clear()`.
    /// 6. **Only now** `self.wal.reset()` — the records the WAL held are safely
    ///    in the SSTable, so the log can be truncated (Pill 9). Reset before the
    ///    SSTable is durable and a crash in between loses those writes.
    pub fn flush(&mut self) -> Result<()> {
        todo!("Step 6d: write sorted memtable -> new SSTable (durable), swap it in, clear memtable, THEN reset WAL")
    }

    /// Merge all SSTables into one, dropping shadowed versions and tombstones.
    /// **Step 7 — your code lives in [`crate::compaction`].** This method is the
    /// **given** wiring around it.
    pub fn compact(&mut self) -> Result<()> {
        if self.sstables.len() < 2 {
            return Ok(()); // nothing to merge
        }
        let new_id = self.next_sst_id;
        let path = compaction::compact(&self.dir, new_id, &self.sstables)?;
        let merged = SsTable::open(&path, new_id)?;

        // The new table is durable (compact fsync'd it). Now it's safe to drop
        // the inputs: swap state first, then delete the old files.
        let old_ids: Vec<u64> = self.sstables.iter().map(|t| t.id).collect();
        self.sstables.clear();
        self.sstables.push(merged);
        self.next_sst_id += 1;

        for id in old_ids {
            std::fs::remove_file(sstable::sst_path(&self.dir, id))?;
        }
        encoding::sync_dir(&self.dir)?;
        Ok(())
    }

    /// All live key/value pairs in ascending key order. **Given** — and a worked
    /// example of the newest-wins merge you'll re-implement in compaction.
    ///
    /// Gathers every record from every SSTable and the memtable, keeps the one
    /// with the highest `seq` per key, then drops tombstones. Reads everything
    /// into memory — fine for a teaching store, not what a real range scan does
    /// (that streams a merge of the already-sorted sources).
    pub fn scan(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut best: BTreeMap<Vec<u8>, Record> = BTreeMap::new();
        for sst in &self.sstables {
            for rec in sst.iter()? {
                keep_newest(&mut best, rec);
            }
        }
        for rec in self.mem.records() {
            keep_newest(&mut best, rec.clone());
        }
        let mut out = Vec::new();
        for (key, rec) in best {
            if let ValueKind::Put(v) = rec.value {
                out.push((key, v));
            }
        }
        Ok(out)
    }

    /// A [`Stats`] snapshot for the CLI. **Given.**
    pub fn stats(&self) -> Stats {
        Stats {
            memtable_entries: self.mem.len(),
            memtable_bytes: self.mem.approx_bytes(),
            sstables: self.sstables.len(),
            sstable_records: self.sstables.iter().map(|t| t.len()).sum(),
            next_seq: self.seq,
        }
    }

    /// Flush if the memtable has crossed the size threshold. **Given** — called
    /// by `put`/`delete`.
    #[allow(dead_code)] // called by your `put`/`delete` in Step 6
    fn maybe_flush(&mut self) -> Result<()> {
        if self.mem.approx_bytes() >= self.opts.memtable_flush_bytes {
            self.flush()?;
        }
        Ok(())
    }
}

/// Keep `rec` in `best` only if it's newer (higher `seq`) than what's there.
/// **Given** helper used by [`Db::scan`].
fn keep_newest(best: &mut BTreeMap<Vec<u8>, Record>, rec: Record) {
    match best.get(&rec.key) {
        Some(existing) if existing.seq >= rec.seq => {}
        _ => {
            best.insert(rec.key.clone(), rec);
        }
    }
}

/// List the ids of every `*.sst` file directly in `dir`. **Given** — the readdir
/// plumbing so `open_with` doesn't have to.
fn list_sstable_ids(dir: &Path) -> Result<Vec<u64>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if let Some(id) = sstable::parse_sst_id(&path) {
            ids.push(id);
        }
    }
    Ok(ids)
}
