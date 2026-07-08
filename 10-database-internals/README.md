# lsmkv

A **crash-safe, LSM-tree embedded key-value store** — the storage engine under things like LevelDB, RocksDB, and Cassandra, built from the bytes up in pure Rust with no dependencies. This crate is a **build-it-yourself lab**: the types, the CRC, the memtable, the wiring, the CLI, and the tests are given; the interesting parts — the record codec, the write-ahead log, the SSTable format, the read path, flush, and compaction — are `todo!()` stubs, each backed by a test that fails until you implement it.

A log-structured merge tree turns random writes into sequential appends: writes go to a write-ahead log and an in-memory sorted map, spill to immutable sorted files (SSTables) when that fills, and are periodically merged. Underneath it all is one guarantee — the reason a database is not just a serialized `HashMap`: **a write that returns is durable, and `open` replays the log to recover the in-memory state a crash destroyed.**

## What it does

- Stores `key -> value` (arbitrary bytes) in a directory, with `put` / `get` / `delete` / `scan` and an explicit `flush` / `compact`.
- Makes every write **durable before it returns**, via a CRC-framed write-ahead log that's `fsync`'d on each `put` (`src/wal.rs`).
- **Recovers from a crash** on `open`: replays the WAL over the on-disk SSTables to rebuild the memtable that was in RAM when the process died (`src/db.rs`).
- Flushes the memtable to immutable, sorted, index-bearing **SSTables** (`src/sstable.rs`) and reads them with a binary search + a single positioned read (`pread`).
- **Compacts** many SSTables into one, keeping the newest version of each key and dropping tombstones (`src/compaction.rs`).
- Ships a `kv` CLI so you can drive a real on-disk store from the shell — and kill it between commands to watch recovery work.

## What you'll build

- **The record codec** (`encoding.rs`): the self-delimiting `[seq][kind][klen][key][vlen][value]` format that every WAL frame and SSTable entry is made of.
- **The write-ahead log** (`wal.rs`): framed `[crc][len][payload]` appends, an explicit `fsync`, and a `replay` that recovers every intact record and **stops cleanly at a torn tail**.
- **The SSTable** (`sstable.rs`): an atomic, fsync'd writer (temp file → rename → fsync dir); a reader that validates the footer magic and loads the key→offset index; a `get` that binary-searches and `pread`s one record; and an `iter` for merges.
- **The store** (`db.rs`): the durability dance on `put`/`delete`, the newest-wins `get`, a crash-safe `flush` (SSTable durable *then* WAL reset), and the recovery path in `open`.
- **Compaction** (`compaction.rs`): the newest-`seq`-wins k-way merge that reclaims space and read performance.

## Running it

The engine is `todo!()` stubs until you fill them in. The workflow:

```bash
cargo check --all-targets   # clean from the start — the scaffold compiles
cargo test                  # unit + integration; fails on the stubs, green when you're done
cargo bench                 # criterion: WAL-synced put throughput, get hit vs miss
```

Drive a real store from the shell with the `kv` CLI — each invocation is a **fresh process**, so every read after a write proves recovery across a restart:

```bash
cargo run --bin kv -- ./data put user:1 alice
cargo run --bin kv -- ./data put user:2 bob
cargo run --bin kv -- ./data get user:1     # -> alice   (read back from the WAL, no flush yet)
cargo run --bin kv -- ./data del user:1
cargo run --bin kv -- ./data get user:1     # -> (not found)
cargo run --bin kv -- ./data flush          # memtable -> SSTable; WAL truncated
ls ./data                                   # wal.log + NNNNNNNNNN.sst
cargo run --bin kv -- ./data scan           # every live key=value, sorted
cargo run --bin kv -- ./data compact        # merge SSTables into one, drop tombstones
cargo run --bin kv -- ./data stats
```

Because each `put`/`del` is WAL-synced before it returns, you can kill the process at any moment and the next command still sees every acknowledged write — the crash safety, observable by hand.

## How it works

A single `put` flows down four layers, and the design *is* those layers:

- **Write-ahead log → durability.** The write is appended to `wal.log` and `fsync`'d *before* anything else. That's the instant it becomes safe; the log is the source of truth a crash recovers from. A per-frame CRC lets `replay` distinguish a complete record from a half-written tail (a torn write) and stop exactly at the last good one.
- **Memtable → speed and sort order.** The write then lands in an in-memory `BTreeMap`, so reads see it immediately and it's already sorted for the eventual flush. When it grows past a threshold, it spills to disk.
- **SSTable → fast, immutable on-disk storage.** A flush writes the memtable as an immutable sorted file with a `[data][index][footer]` layout. The in-memory index makes a lookup a binary search plus one `pread`; immutability means no torn pages and no locks. The write is made atomic by writing a temp file and `rename`-ing it into place.
- **Compaction → bounded reads and space.** Many SSTables are merged into one, keeping only the highest-`seq` record per key and dropping tombstones, so `get` doesn't probe an ever-growing pile of files.

Two invariants hold the whole thing together. Every record carries a monotonic **sequence number**, and *"higher `seq` wins"* drives the read path, the merge, and recovery alike. And every durability-sensitive step follows one **ordering rule** — *make the new copy durable before you delete the old one*: `flush` fsyncs the SSTable before it resets the WAL; `compact` fsyncs the merged file before it deletes the inputs. Reverse either and a crash in the wrong microsecond loses data.

Depth — durability and `fsync` semantics, the LSM-vs-B-tree amplification trade-offs, the page cache, MVCC and snapshots, and why the recovery is idempotent — lives in the learn file.

## Project layout

| File | Status |
| --- | --- |
| `Cargo.toml` | Given — zero-dependency lib + `kv` bin + `criterion` bench; release profile (LTO). |
| `src/lib.rs` | Given — module map and the crate-level tour. |
| `src/error.rs` | Given — the `Error`/`Result` type (`Io` vs `Corrupt`). |
| `src/encoding.rs` | **TODO Step 1** — `Record::encode`/`decode`. `crc32`, `sync_dir`, the types are given. |
| `src/wal.rs` | **TODO Step 2** — `append`, `sync`, `replay`. `open`, `reset` given. |
| `src/memtable.rs` | Given — the sorted, newest-wins in-memory buffer. |
| `src/sstable.rs` | **TODO Steps 3–4** — `write`, `open`, `get`, `iter`. Path helpers, `IndexEntry`, `read_at` given. |
| `src/db.rs` | **TODO Steps 5–6** — recovery in `open_with`, `get`, `put`, `delete`, `flush`. `compact` wiring, `scan`, `stats` given. |
| `src/compaction.rs` | **TODO Step 7** — the newest-wins merge. `newest_per_key` helper given. |
| `src/bin/kv.rs` | Given — the command-line front end. |
| `tests/integration.rs` | Given — round-trip, crash-before-flush recovery, tombstones-survive-flush, compaction, scan. |
| `benches/kv.rs` | Given — criterion benchmarks for `put` and `get` (hit/miss). |

## Status

Scaffold — ready to build. `cargo check --all-targets` is clean; `cargo test` fails on the `todo!()` stubs and goes green as you implement Steps 1–7. The concept pills — durability and `fsync`, the write-ahead log, torn writes and CRCs, the LSM tree, SSTables and the block index, tombstones, the read path, durability ordering, crash recovery, LSM-vs-B-tree and the page cache, and MVCC — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [MIT license](https://opensource.org/licenses/MIT) or [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
