//! End-to-end tests over the public [`lsmkv`] API. **Given.**
//!
//! These drive the store the way the CLI and a real embedder do — and, crucially,
//! they simulate crashes by dropping a [`Db`] without a clean shutdown and
//! reopening it. They `panic` (`not yet implemented`) until you fill in the
//! `todo!()` stubs; a green run of this file is the signal that Steps 1–7 are
//! done. Each uses a throwaway temp directory that's removed even on panic.

use lsmkv::{Db, Options};
use tempfile::tempdir;

/// A store that flushes often, so tests exercise the on-disk path without
/// needing megabytes of data.
fn small_db(dir: &std::path::Path) -> Db {
    Db::open_with(dir, Options { memtable_flush_bytes: 256 }).expect("open")
}

#[test]
fn put_then_get_roundtrips() {
    let dir = tempdir().unwrap();
    let mut db = Db::open(dir.path()).unwrap();
    db.put(b"name", b"ada").unwrap();
    db.put(b"lang", b"rust").unwrap();
    assert_eq!(db.get(b"name").unwrap().as_deref(), Some(&b"ada"[..]));
    assert_eq!(db.get(b"lang").unwrap().as_deref(), Some(&b"rust"[..]));
    assert_eq!(db.get(b"missing").unwrap(), None);
}

#[test]
fn overwrite_keeps_the_newest_value() {
    let dir = tempdir().unwrap();
    let mut db = Db::open(dir.path()).unwrap();
    db.put(b"k", b"v1").unwrap();
    db.put(b"k", b"v2").unwrap();
    db.put(b"k", b"v3").unwrap();
    assert_eq!(db.get(b"k").unwrap().as_deref(), Some(&b"v3"[..]));
}

#[test]
fn delete_hides_the_key() {
    let dir = tempdir().unwrap();
    let mut db = Db::open(dir.path()).unwrap();
    db.put(b"k", b"v").unwrap();
    db.delete(b"k").unwrap();
    assert_eq!(db.get(b"k").unwrap(), None);
}

#[test]
fn survives_a_crash_before_flush() {
    // The whole point of the WAL: writes that were only ever in the memtable must
    // come back after an unclean restart.
    let dir = tempdir().unwrap();
    {
        let mut db = Db::open(dir.path()).unwrap();
        for i in 0..50 {
            db.put(format!("key{i}").as_bytes(), format!("val{i}").as_bytes())
                .unwrap();
        }
        // Drop WITHOUT flushing — nothing reached an SSTable; it's all in the WAL.
        drop(db);
    }
    let db = Db::open(dir.path()).unwrap();
    for i in 0..50 {
        assert_eq!(
            db.get(format!("key{i}").as_bytes()).unwrap(),
            Some(format!("val{i}").into_bytes()),
            "key{i} was lost across the crash"
        );
    }
}

#[test]
fn data_persists_across_flush_and_reopen() {
    let dir = tempdir().unwrap();
    {
        let mut db = small_db(dir.path());
        for i in 0..200 {
            db.put(format!("k{i:04}").as_bytes(), format!("v{i}").as_bytes())
                .unwrap();
        }
        db.flush().unwrap(); // force everything to disk
        drop(db);
    }
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.get(b"k0000").unwrap(), Some(b"v0".to_vec()));
    assert_eq!(db.get(b"k0199").unwrap(), Some(b"v199".to_vec()));
    assert_eq!(db.stats().memtable_entries, 0, "reopened store should read from SSTables");
}

#[test]
fn delete_survives_flush() {
    // A tombstone that's been flushed to an SSTable must still shadow an older
    // put that lives in an even older SSTable.
    let dir = tempdir().unwrap();
    let mut db = small_db(dir.path());
    db.put(b"gone", b"here").unwrap();
    db.flush().unwrap(); // put is now in SSTable A
    db.delete(b"gone").unwrap();
    db.flush().unwrap(); // tombstone is now in SSTable B (newer)
    assert_eq!(db.get(b"gone").unwrap(), None);

    // ...and across a reopen.
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.get(b"gone").unwrap(), None);
}

#[test]
fn compaction_merges_and_drops_tombstones() {
    let dir = tempdir().unwrap();
    let mut db = small_db(dir.path());

    // Several flushes -> several SSTables, with overwrites and a delete.
    db.put(b"a", b"1").unwrap();
    db.put(b"b", b"1").unwrap();
    db.flush().unwrap();
    db.put(b"a", b"2").unwrap(); // overwrites a
    db.put(b"c", b"1").unwrap();
    db.flush().unwrap();
    db.delete(b"b").unwrap(); // tombstones b
    db.flush().unwrap();

    assert!(db.stats().sstables >= 2, "expected several SSTables before compaction");

    db.compact().unwrap();
    assert_eq!(db.stats().sstables, 1, "compaction should leave a single SSTable");

    // Live data is intact and reflects the newest versions...
    assert_eq!(db.get(b"a").unwrap(), Some(b"2".to_vec()));
    assert_eq!(db.get(b"c").unwrap(), Some(b"1".to_vec()));
    assert_eq!(db.get(b"b").unwrap(), None);

    // ...and the tombstone was physically dropped: b is simply gone.
    let keys: Vec<_> = db.scan().unwrap().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"a".to_vec(), b"c".to_vec()]);
}

#[test]
fn scan_returns_sorted_live_pairs() {
    let dir = tempdir().unwrap();
    let mut db = small_db(dir.path());
    db.put(b"banana", b"2").unwrap();
    db.put(b"apple", b"1").unwrap();
    db.put(b"cherry", b"3").unwrap();
    db.delete(b"banana").unwrap();
    let got = db.scan().unwrap();
    assert_eq!(
        got,
        vec![
            (b"apple".to_vec(), b"1".to_vec()),
            (b"cherry".to_vec(), b"3".to_vec()),
        ]
    );
}
