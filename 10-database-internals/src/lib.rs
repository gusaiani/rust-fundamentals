//! `lsmkv` — a crash-safe, LSM-tree embedded key-value store, built to learn how
//! real storage engines (LevelDB, RocksDB, Cassandra, SQLite's LSM) actually
//! keep data safe on disk.
//!
//! The design is a **log-structured merge tree**, and the whole module is the
//! journey a single `put` takes down it:
//!
//! 1. **Write-ahead log** ([`wal`]) — the write is appended to an on-disk log
//!    and `fsync`'d *before* anything else. That's what "durable" means; the log
//!    is the source of truth a crash recovers from.
//! 2. **Memtable** ([`memtable`]) — the write is then applied to a sorted
//!    in-memory map. Reads see it immediately; it's small and fast.
//! 3. **SSTable** ([`sstable`]) — when the memtable fills, it's flushed to an
//!    immutable, sorted on-disk file with a key→offset index, and the WAL is
//!    reset. Data now lives on disk in a form built for fast lookups.
//! 4. **Compaction** ([`compaction`]) — many SSTables are periodically merged
//!    into one, keeping only the newest version of each key and dropping
//!    tombstones, to bound read cost and reclaim space.
//!
//! [`Db`] ([`db`]) ties them together: `put`/`delete`/`get`, a size-triggered
//! flush, explicit `compact`, and — the headline feature — `open` that **replays
//! the WAL to recover** the in-memory state that a crash destroyed. Every record
//! carries a monotonic sequence number ([`Seq`]), and the single rule *"higher
//! `seq` wins"* drives the read path, the merge, and recovery alike (and is the
//! seed of MVCC).
//!
//! The crate is a **build-it-yourself lab**: the types, the CRC, the memtable,
//! and all the wiring are given; the interesting parts — the record codec, the
//! WAL append/replay, the SSTable format, the read path, flush, and compaction —
//! are `todo!()` stubs, each backed by a test that fails until you fill it in.
//! Work top to bottom through Steps 1–7; the step for each stub is in its doc
//! comment, and the concepts are in [`README-LEARN.md`](https://github.com/your-username/lsmkv).

pub mod compaction;
pub mod db;
pub mod encoding;
pub mod error;
pub mod memtable;
pub mod sstable;
pub mod wal;

pub use db::{Db, Options, Stats};
pub use encoding::{Record, Seq, ValueKind};
pub use error::{Error, Result};
