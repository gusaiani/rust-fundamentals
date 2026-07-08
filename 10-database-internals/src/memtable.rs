//! The memtable — the in-memory write buffer (Pill 5). **Given in full.**
//!
//! Every write lands here after it's been appended to the WAL. It's a
//! `BTreeMap` keyed by the raw key, so it's always **sorted** — which is exactly
//! the order an SSTable wants when the memtable is flushed to disk, so the flush
//! is a straight sequential write with no sorting step.
//!
//! Only the newest write per key is kept: a second `put` (or a `delete`) to the
//! same key overwrites the entry in place. That's correct because the WAL still
//! has the full history for recovery, and older versions on disk are shadowed by
//! `seq` order anyway. The map holds the whole [`Record`] (seq + value), so a
//! flush preserves the sequence numbers the on-disk merge later relies on.
//!
//! `approx_bytes` is a running estimate of heap footprint; the [`crate::db::Db`]
//! uses it to decide when the memtable is big enough to flush.

use std::collections::BTreeMap;

use crate::encoding::{Record, ValueKind};

/// An in-memory, sorted, newest-wins buffer of records keyed by key.
#[derive(Default)]
pub struct MemTable {
    map: BTreeMap<Vec<u8>, Record>,
    approx_bytes: usize,
}

impl MemTable {
    /// An empty memtable.
    pub fn new() -> MemTable {
        MemTable { map: BTreeMap::new(), approx_bytes: 0 }
    }

    /// Insert or overwrite the entry for `record.key`, keeping the new record.
    ///
    /// Updates the byte estimate by the delta between the old and new entries so
    /// `approx_bytes` tracks the live footprint even across overwrites.
    pub fn insert(&mut self, record: Record) {
        let new_size = entry_size(&record);
        if let Some(old) = self.map.insert(record.key.clone(), record) {
            self.approx_bytes -= entry_size(&old);
        }
        self.approx_bytes += new_size;
    }

    /// The newest record for `key`, if any is buffered here.
    ///
    /// A returned [`ValueKind::Delete`] means "known deleted" — the caller must
    /// treat that as a definitive miss and **not** fall through to older
    /// SSTables (that's the whole point of a tombstone).
    pub fn get(&self, key: &[u8]) -> Option<&Record> {
        self.map.get(key)
    }

    /// The records in ascending key order — the flush order for an SSTable.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.map.values()
    }

    /// Number of distinct keys currently buffered.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the memtable holds no entries.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// A rough estimate of the heap bytes held, for the flush threshold.
    pub fn approx_bytes(&self) -> usize {
        self.approx_bytes
    }

    /// Drop everything (called after a successful flush to a new SSTable).
    pub fn clear(&mut self) {
        self.map.clear();
        self.approx_bytes = 0;
    }
}

/// A cheap per-entry size estimate: key + value bytes + fixed per-record
/// overhead (seq, tags, the map node). Doesn't need to be exact — it only gates
/// when to flush.
fn entry_size(record: &Record) -> usize {
    let value_len = match &record.value {
        ValueKind::Put(v) => v.len(),
        ValueKind::Delete => 0,
    };
    record.key.len() + value_len + 48
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_write_wins_in_place() {
        let mut m = MemTable::new();
        m.insert(Record::put(1, b"k".to_vec(), b"v1".to_vec()));
        m.insert(Record::put(2, b"k".to_vec(), b"v2".to_vec()));
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(b"k").unwrap().seq, 2);
    }

    #[test]
    fn a_delete_overwrites_a_put() {
        let mut m = MemTable::new();
        m.insert(Record::put(1, b"k".to_vec(), b"v".to_vec()));
        m.insert(Record::tombstone(2, b"k".to_vec()));
        assert_eq!(m.get(b"k").unwrap().value, ValueKind::Delete);
    }

    #[test]
    fn records_come_out_sorted() {
        let mut m = MemTable::new();
        for k in ["c", "a", "b"] {
            m.insert(Record::put(1, k.as_bytes().to_vec(), b"x".to_vec()));
        }
        let keys: Vec<_> = m.records().map(|r| r.key.clone()).collect();
        assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }
}
