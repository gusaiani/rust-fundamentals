//! Compaction — merging many SSTables into one (Pill 10).
//!
//! Flushes keep producing new SSTables, so over time a key can have several
//! versions scattered across files: an old `Put` in table 3, a newer `Put` in
//! table 5, maybe a `Delete` tombstone in table 7. Reads stay correct (newest
//! `seq` wins), but they get slower — `get` may probe every table — and dead
//! versions waste disk. **Compaction** reclaims both: it merges the tables into
//! a single new one that keeps only the newest record per key.
//!
//! Because this store compacts **all** SSTables at once (a full merge), there is
//! no older table left to un-shadow a delete — so a tombstone that wins can be
//! **physically dropped**, not just carried forward. (In a leveled store you'd
//! only drop a tombstone when compacting the bottom level, for exactly this
//! reason.)
//!
//! This is **your Step 7**. The [`crate::db::Db::compact`] method handles the
//! before/after wiring (writing durably, swapping tables in, deleting the old
//! files); you write the merge that produces the record set.

use std::path::{Path, PathBuf};

use crate::encoding::{Record, ValueKind};
use crate::error::Result;
use crate::sstable::SsTable;

/// Merge every table in `tables` into one new SSTable with id `new_id`, written
/// under `dir`, and return its path. **Step 7 — your code.**
///
/// The algorithm:
/// 1. Collect all records from every table (`SsTable::iter`). The tables overlap
///    in key space, so the same key may appear many times with different `seq`s.
/// 2. Reduce to the **newest record per key**: keep the one with the highest
///    `seq`. (A `BTreeMap<Vec<u8>, Record>` keyed by key, updated only when the
///    incoming `seq` is higher, gives you both the dedup and the final sorted
///    order for free — see [`crate::db::Db::scan`] for the same move.)
/// 3. **Drop tombstones:** if the surviving record for a key is a
///    [`ValueKind::Delete`], omit the key entirely. Safe here because this is a
///    full compaction — no older table survives to resurrect it.
/// 4. Emit the survivors in ascending key order and hand them to
///    [`SsTable::write`], which writes them durably and returns the path.
///
/// Note you don't need to worry about *which* table is newer as you collect —
/// the `seq` on each record already encodes global write order, so "highest seq
/// wins" is correct no matter what order you visit the tables in.
pub fn compact(dir: &Path, new_id: u64, tables: &[SsTable]) -> Result<PathBuf> {
    let mut records = Vec::new();
    for table in tables {
        records.extend(table.iter()?);
    }

    let mut survivors = newest_per_key(records);
    survivors.retain(|rec| rec.value != ValueKind::Delete);

    SsTable::write(dir, new_id, &survivors)
}

/// Reduce a batch of records to the newest per key, tombstones included. A
/// **given** helper you may use from [`compact`] (or roll your own). Returns the
/// survivors sorted ascending by key.
#[allow(dead_code)]
pub(crate) fn newest_per_key(records: impl IntoIterator<Item = Record>) -> Vec<Record> {
    use std::collections::BTreeMap;
    let mut best: BTreeMap<Vec<u8>, Record> = BTreeMap::new();
    for rec in records {
        match best.get(&rec.key) {
            Some(existing) if existing.seq >= rec.seq => {}
            _ => {
                best.insert(rec.key.clone(), rec);
            }
        }
    }
    best.into_values().collect()
}
