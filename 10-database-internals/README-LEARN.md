# Database Internals — in 5-Minute Pills

## Goal

Build a real, crash-safe **embedded key-value store** — the storage engine underneath things like LevelDB, RocksDB, Cassandra, and SQLite's LSM backend — as a **log-structured merge tree** in pure Rust. You'll write the four layers a durable store is made of: a **write-ahead log** that makes a write survive a power cut the instant it returns, an in-memory **memtable**, immutable on-disk **SSTables** with a key→offset index, and **compaction** that merges them. The payoff is the headline feature of any real database: `open()` **replays the WAL to recover** the exact state that was in memory when the process was killed mid-write — you'll test it by dropping the database without a clean shutdown and watching every acknowledged write come back. By the end you understand, from the bytes up, how a database keeps your data when the machine dies, and why write-heavy systems are built as LSM trees instead of B-trees.

## Time estimate

~1 day (13 pills × 5 min + project)

## What you'll learn

- **What "durable" actually means** — that `write()` returns long before your data is on the platter, that only `fsync` makes it safe, and that you must `fsync` the *directory* too, not just the file
- **The write-ahead log** — why you log *before* you apply, how replay rebuilds lost in-memory state, and why this one idea is the foundation of crash safety
- **Torn writes & CRCs** — a crash can leave a half-written record; a per-frame checksum + length is how recovery tells a complete record from a shredded tail and stops cleanly
- **The LSM tree** — turning random writes into sequential appends: memtable → flush → SSTable → compaction, the design every write-optimized database shares
- **SSTables** — immutable sorted files with a **block index**, so a lookup is a binary search plus a single positioned read (`pread`) instead of a scan
- **Tombstones** — why a delete is a *record you add*, not a row you remove, and when it's finally safe to physically drop one
- **The read path** — merging newest-to-oldest across memtable and SSTables so the freshest version of a key wins, with sequence numbers making the order unambiguous
- **Durability ordering** — the exact sequence (fsync the SSTable → then reset the WAL; fsync the merge → then delete the inputs) that makes flush and compaction crash-safe rather than data-losing
- **Crash recovery** — replaying the log over the SSTables on `open`, restoring the sequence high-water mark, and why the whole thing is idempotent
- **LSM vs B-tree & the page cache** — write vs read vs space amplification, why the OS page cache is your buffer pool, and when a B-tree is the right call instead
- **MVCC, in miniature** — how the per-write sequence number is already a version stamp, and what it takes to turn it into real snapshot reads

## Concepts

### Pill 1: The Workload — an LSM Key-Value Store

The thing you're building is an **embedded** key-value store: a library, not a server. You call `put(key, value)`, `get(key)`, `delete(key)`, and the data lives in a directory of files that survives your process. That's the shape of RocksDB, LevelDB, LMDB, and the storage layer inside most databases — the network, SQL, and replication are separate concerns bolted on top of exactly this.

The design is a **log-structured merge tree (LSM)**, and its one big idea is: *never update data in place on disk.* A B-tree finds the right page and rewrites it — a random write, an expensive seek. An LSM instead **appends**. Every write goes to an in-memory sorted map (the **memtable**); when that fills, it's written out **all at once** as a new immutable sorted file (an **SSTable**) — one big sequential write, the fastest thing a disk does. Reads check the memtable, then the SSTables newest-to-oldest. Over time you accumulate many SSTables, so a background **compaction** merges them, keeping only the newest version of each key. That's the whole architecture: `memtable → flush → SSTable → compaction`, with a write-ahead log underneath to make it crash-safe. The rest of these pills are the details of getting each layer right.

### Pill 2: "Durable" Is a Lie Until You `fsync`

The single most important fact in storage: when `write()` (or Rust's `File::write_all`) returns, **your data is not on the disk.** It's in the operating system's page cache — RAM the kernel will flush to the device *eventually*. A normal `write` followed by a power cut loses that data completely, even though the call succeeded. This is not a Rust thing or an OS bug; it's how buffered I/O works everywhere, and it's why "I wrote the file" and "the data is safe" are different claims.

The call that actually makes data durable is **`fsync`** (`File::sync_all` in Rust): it blocks until the device confirms the bytes are on stable storage. It's slow — often a millisecond or more, because it may wait for a physical platter or an SSD's flush — which is exactly why a database is careful to `fsync` as little as possible (one log append per write, not the whole data set). There's a second, sneakier requirement: after you create or rename a file, the file's *directory entry* is itself a change the OS caches, so a crash can lose the **name** even if the contents were synced. The fix is to `fsync` the **directory** too — that's what [`encoding::sync_dir`](./src/encoding.rs) does, and forgetting it is a classic real-world data-loss bug. Every durability decision in this project is about *what* to fsync and, crucially, *in what order* (Pills 9–10).

### Pill 3: The Write-Ahead Log

The memtable is fast because it's in RAM — and RAM is exactly what a crash erases. So before a write touches the memtable, it is appended to an on-disk **write-ahead log (WAL)** and the log is `fsync`'d. The rule is in the name: **write ahead** — the log records what you're *about to do* before you do it. Now the durability story is airtight: a `put` is safe the moment its WAL frame is synced, even though the "real" copy is still only in volatile memory. If the process dies, the memtable vanishes, but on restart you **replay** the WAL — re-apply every logged record in order — and the memtable is rebuilt exactly as it was.

This is the oldest trick in databases (ARIES, journaling filesystems, Postgres's WAL, SQLite's rollback/WAL journals — all the same idea) and it's the foundation everything else rests on. It also decouples two speeds: the log is a pure **sequential append** (fast, ordered), while the "apply" step can be an in-memory update or a later batched flush. In this project the WAL is [`wal.rs`](./src/wal.rs): [`Wal::append`](./src/wal.rs) writes a framed record, [`Wal::sync`](./src/wal.rs) fsyncs it, and [`Wal::replay`](./src/wal.rs) reads it all back on recovery. The log is allowed to be redundant — once a flush has put the data in an SSTable, the WAL frames that carried it are dead weight and the log is reset (Pill 9).

### Pill 4: Torn Writes and the CRC

A crash doesn't politely stop between records. It can strike **mid-append**, leaving the last WAL frame half-written: the length says 200 bytes but only 80 made it, or the bytes are physically garbled. Recovery has to notice this and stop — treating a shredded tail as "the log ends here," keeping every intact record before it. If it instead tried to parse the garbage as a record, you'd get corruption or a panic on the very path whose job is to save you.

The mechanism is a **frame**: each record is written as `[crc: u32][len: u32][payload]`, where `crc` is a checksum ([CRC-32](./src/encoding.rs), the same one gzip and Ethernet use) over the payload bytes. On replay you read the length, and if fewer than `len` bytes remain, the frame is torn — **stop**. If the bytes are there but their CRC doesn't match the stored one, they're corrupt — **stop**. Only a frame that's complete *and* checksum-clean is replayed. "Recover up to the last good record" is the exact guarantee a WAL gives, and the CRC + length is how you enforce it. (This is also why `Record::decode` in Pill 5 must return a clean `None` on a short buffer instead of panicking — a torn payload has to be survivable at every layer.)

```rust
// replay, in outline
while remaining >= 8 {
    let (crc, len) = read_header();
    if remaining_payload < len { break; }          // torn tail
    if crc32(&payload) != crc     { break; }        // corrupt tail
    records.push(Record::decode(&payload)?);        // intact — replay it
}
```

### Pill 5: The Record — One Codec, Everything Downstream

Everything the store persists is a [`Record`](./src/encoding.rs): a `seq` (sequence number), a `key`, and a `value` that's either `Put(bytes)` or a `Delete` tombstone. Both the WAL and the SSTable are just sequences of encoded records, so the **one** piece of code that knows the byte layout — `Record::encode` / `Record::decode` — is the foundation the whole store is built on. Get it right and framing, replay, and SSTable iteration all fall out for free; get it wrong and everything downstream reads garbage.

The layout is a self-delimiting, little-endian frame:

```text
seq: u64 | kind: u8 | klen: u32 | key[klen] | vlen: u32 | value[vlen]
```

Two design points matter. First, it's **self-delimiting**: `decode` returns not just the record but *how many bytes it consumed*, so a caller can walk a buffer of records back-to-back — which is exactly how the WAL replays and how an SSTable's data block iterates. Second, `decode` must be **total**: any short or malformed input returns `None`, never panics, because that's the torn-write case from Pill 4. The `seq` field is the subtle one: it's a global, monotonically increasing version stamp on every write, and the single rule *"higher `seq` wins"* is what makes the read path, compaction, and recovery all unambiguous (and is the seed of MVCC — Pill 13).

### Pill 6: The Memtable and the Sorted Requirement

After the WAL, a write lands in the **memtable**: an in-memory `BTreeMap` keyed by the raw key ([`memtable.rs`](./src/memtable.rs), given to you). Two properties make a `BTreeMap` exactly right. It's **sorted**, so when the memtable is flushed to disk it's already in the order an SSTable wants — the flush is a straight sequential write with no sort step. And it keeps only the **newest write per key**: a second `put` (or a `delete`) to the same key overwrites the entry in place, because the WAL still holds the full history for recovery and older on-disk versions are shadowed by `seq` anyway.

The memtable also tracks its approximate byte size, because that's the **flush trigger**: once it grows past a threshold (`memtable_flush_bytes`), the store flushes it to a new SSTable and starts a fresh, empty memtable. This is the valve that bounds memory and turns a stream of small random writes into occasional big sequential ones. Small threshold → frequent small SSTables (and more compaction later); large threshold → fewer, bigger flushes (and more data at risk in the WAL). Real stores set it to tens of megabytes; the tests here set it to 256 bytes so a handful of writes exercises the on-disk path.

### Pill 7: SSTables — Immutable, Sorted, Indexed

An **SSTable** (Sorted String Table) is the on-disk home of flushed data: a file that is **written once and never modified**. Immutability is a feature — no in-place updates means no torn-page problem, no locking, and readers never see a half-changed file. The layout is three regions ([`sstable.rs`](./src/sstable.rs)):

```text
[ data   ]  every record, ascending key order, back-to-back (record.encode())
[ index  ]  one entry per key: [klen u32][key][offset u64][len u32]
[ footer ]  [index_offset u64][index_len u64][count u64][max_seq u64][magic u32]
```

The **index** is the whole point. Because it's a sorted key→location map, a lookup is a **binary search** of the index (in memory — the index is small and loaded on `open`) followed by a **single positioned read** of just that one record's bytes. No scanning the file. The **footer** is a fixed-size trailer read *first*, so the reader can find the index without hunting; its `magic` number rejects foreign or corrupt files instead of misreading them, and `max_seq` lets recovery restore the sequence high-water mark without touching the data (Pill 11). The positioned read uses `pread` (`FileExt::read_exact_at`) — read N bytes at offset X with no seek, straight through the OS page cache (Pill 12). Real SSTables add per-block compression and a Bloom filter (your module 9 filter!) to skip files that can't contain a key; those are stretch goals here.

### Pill 8: The Read Path — Newest Wins

A key can exist in several places at once: the freshest version in the memtable, an older one in a recent SSTable, an even older one further down. `get` has to return the **newest**, and it does so by checking sources in newest-to-oldest order and stopping at the **first** one that knows the key:

1. the **memtable** (always the newest data — it holds writes not yet flushed);
2. then each **SSTable** from newest to oldest (highest id first).

The first source that contains the key is authoritative. If that record is a `Put`, return its bytes. If it's a `Delete` **tombstone**, return "not found" — and crucially, **do not** fall through to older tables, because the tombstone's whole job is to shadow that older value (Pill 6 on why a delete is a record, not a removal). This ordering is correct because newer data is always in a newer source, and the `seq` numbers make it provable: within any single source you already keep the highest `seq`, and across sources newer sources hold higher `seq`s. The cost is **read amplification** — a `get` for an absent key may probe every SSTable — which is what compaction (Pill 10) and Bloom filters exist to reduce.

```rust
pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
    if let Some(rec) = self.mem.get(key) { return Ok(value_of(&rec.value)); }
    for sst in &self.sstables {              // newest first
        if let Some(rec) = sst.get(key)? { return Ok(value_of(&rec.value)); }
    }
    Ok(None)
}
```

### Pill 9: Flush — and the Durability Ordering That Makes It Safe

**Flush** moves the memtable to disk: collect its (already sorted) records, write them as a new SSTable, and now the memtable can be cleared. But the *order* of operations is where crash safety lives, and it's easy to get fatally wrong. The correct sequence:

1. Write the records to a **temp file**, `fsync` it, then **rename** it onto the real `NNN.sst` path, then `fsync` the directory. The rename is **atomic** — a crash leaves either no file or a complete one, never a half-written SSTable a reader could trip over. *Now the data is durable on disk.*
2. Add the new SSTable to the live set and clear the memtable.
3. **Only now** reset (truncate) the WAL.

The rule: **the data must be durable in its new home before you delete the old copy.** The WAL frames are the *only* durable record of those writes until step 1 completes; if you reset the WAL before the SSTable is safely fsync'd and a crash hits in between, those writes are gone from *both* places — a silent data-loss bug. Do it in the right order and a crash at any instant is survivable: either the WAL still has the writes (crash before step 3) or the SSTable does (crash after). This same "make the new thing durable, *then* drop the old thing" discipline governs compaction next.

### Pill 10: Compaction — Merging Away the Cruft

Flushes keep minting SSTables, so a key accumulates stale versions scattered across files: an old `Put` in table 3, a newer `Put` in table 5, a `Delete` in table 7. Reads stay correct (newest `seq` wins) but get **slower** (more tables to probe) and disk fills with dead data. **Compaction** fixes both: merge tables into a new one that keeps only the **newest record per key**, then delete the inputs.

The merge is a **k-way merge by key, keeping the highest `seq`** — which, because `seq` encodes global write order, is correct no matter what order you visit the tables in (a `BTreeMap<key, Record>` updated only when the incoming `seq` is higher gives you the dedup *and* the final sorted order in one shot; [`Db::scan`](./src/db.rs) is a worked example of exactly this). The subtle part is **tombstones**: normally you must *carry a tombstone forward*, because dropping it could un-shadow an older `Put` in a table you didn't touch. But this store does a **full compaction** — it merges *all* SSTables at once — so no older table survives, and a surviving tombstone can be **physically dropped**. (In a leveled store like RocksDB, you only drop tombstones when compacting the bottom level, for precisely this reason.) The durability ordering mirrors flush: write and `fsync` the merged SSTable, swap it into the live set, **then** delete the old files and `fsync` the directory — new thing durable before old thing gone.

### Pill 11: Crash Recovery — `open` Replays the Log

Recovery is where all the pieces pay off, and it's the feature that separates a database from a hash map you serialized. On `open` ([`Db::open_with`](./src/db.rs)):

1. **Load the SSTables.** Scan the directory for `*.sst` files, open each (validating the footer magic), and order them newest-first. This is the durable, flushed data.
2. **Replay the WAL.** `Wal::replay` returns every intact record the log holds — these are the writes that were in the memtable but hadn't been flushed when the process stopped. Insert them into a fresh memtable; newest-wins dedup is automatic.
3. **Restore the sequence high-water mark.** Set the next `seq` to **one past the maximum** seen anywhere — across both the replayed WAL records *and* every SSTable's `max_seq` (from the footer). Miss the SSTables and, right after a flush-then-crash where the WAL is empty, you'd hand out sequence numbers that collide with flushed data — silently corrupting the newest-wins ordering.

The elegance is that recovery is just "load what's durable, replay what's logged" — the **same** operations as normal running, so it's **idempotent**: replaying a WAL twice yields the same state, because inserts are overwrites keyed by `(key)` with `seq` breaking ties. You'll prove it in the tests by writing 50 keys, dropping the `Db` with no flush (a simulated crash), reopening, and finding all 50 — data that lived *only* in RAM and the log, reconstructed.

### Pill 12: LSM vs B-Tree, and the Page Cache

Why an LSM and not a B-tree? It comes down to the three **amplifications** every storage engine trades between. A **B-tree** (Postgres, MySQL/InnoDB, LMDB) updates in place: great **read** amplification (a lookup is one root-to-leaf walk, O(log n) page reads) and low space amplification, but every write is a **random** page write, and a small update can dirty a whole page — bad **write** amplification, punishing on spinning disks and wearing on SSDs. An **LSM** inverts it: writes are **sequential appends** (superb write amplification, the reason it's chosen for write-heavy and ingest workloads), paid for with worse **read** amplification (check several SSTables) and **space** amplification (stale versions until compaction). Compaction is the continuous cost that keeps reads and space bounded. Rule of thumb: **write-heavy or flash-friendly → LSM; read-heavy with in-place updates → B-tree.**

Underneath both sits the **page cache**: the OS keeps recently-read file pages in RAM, so your `pread` of an SSTable record usually hits memory, not the device. This is why a database often *doesn't* build its own buffer pool for reads — the kernel already is one — and why `fsync` is about the *write* path (forcing dirty pages out), not reads. It's also why benchmarking storage is treacherous: the second run reads from cache and looks 100× faster. Serious engines (Postgres, RocksDB) *do* manage their own cache for control over eviction and to avoid double-caching, but for this project the OS page cache is your buffer pool, for free.

### Pill 13: MVCC and Snapshots, in Miniature

You've already built the hardest part of **multi-version concurrency control (MVCC)** without naming it: the per-write **sequence number**. MVCC is how real databases let readers and writers not block each other — instead of overwriting a value, every write creates a **new version** stamped with a monotonic number, and a reader takes a **snapshot** (a `seq` boundary) and simply ignores every version newer than its snapshot. Readers never see a writer's in-flight changes and never take a lock; the "old" versions are exactly the stale records compaction would otherwise remove.

In this store, `get` today returns the single newest version. To turn it into a snapshot read, you'd thread a `snapshot_seq` through the read path and, at each source, pick the highest-`seq` record **that is ≤ the snapshot** — skipping versions written after the snapshot was taken. Compaction then can't drop a version any live snapshot might still need (real engines track the oldest active snapshot to decide what's safe to collect). That's the whole idea behind Postgres's MVCC, RocksDB's sequence-numbered snapshots, and how a database gives you a consistent `SELECT` over a table that's being written the entire time you read it. It's a stretch goal here — but notice the foundation is already load-bearing: `seq` isn't bookkeeping, it's the version axis the entire store is organized around.

## Project: `lsmkv` — a crash-safe LSM key-value store

Build the storage engine described above. The crate compiles today (`cargo check --all-targets` is clean); the given code — the types, CRC, memtable, the read/flush/compaction *wiring*, the CLI, and the tests — is complete, and the interesting parts are `todo!()` stubs, each backed by a test that fails until you implement it.

### Requirements

1. A working record codec (`Record::encode`/`decode`) that round-trips and is self-delimiting.
2. A write-ahead log that appends framed, CRC-guarded records and **replays cleanly past a torn tail**.
3. SSTables: an atomic, fsync'd `write`; an `open` that validates the footer and loads the index; a `get` that binary-searches + `pread`s a single record; and an `iter` for merges.
4. A `Db` that does the durability dance on `put`/`delete`, the newest-wins `get`, a crash-safe `flush`, and — the headline — a `open` that **replays the WAL to recover** the unflushed memtable.
5. Compaction that merges all SSTables newest-wins and drops tombstones.
6. The full test suite is green (`cargo test`), and the `kv` CLI survives being killed between commands.

### Starter files

- `src/encoding.rs` — TODO: `Record::encode` / `Record::decode` (Step 1). `crc32`, `sync_dir`, the types are given.
- `src/wal.rs` — TODO: `append`, `sync`, `replay` (Step 2). `open`, `reset` given.
- `src/sstable.rs` — TODO: `write`, `open`, `get`, `iter` (Steps 3–4). Path helpers, `IndexEntry`, `read_at` given.
- `src/db.rs` — TODO: `open_with` recovery, `get`, `put`, `delete`, `flush` (Steps 5–6). `compact` wiring, `scan`, `stats`, the directory scan given.
- `src/compaction.rs` — TODO: `compact` (Step 7). `newest_per_key` helper given.
- `src/memtable.rs`, `src/error.rs`, `src/lib.rs`, `src/bin/kv.rs`, `tests/integration.rs`, `benches/kv.rs` — all given.

### Your task

1. **The record codec (`encoding.rs`, Step 1).** `encode`/`decode` for `[seq][kind][klen][key][vlen][value]`; `decode` returns `(Record, bytes_consumed)` and `None` on a short buffer. `cargo test --lib encoding` checks round-trip, back-to-back walking, and short-tail rejection.
2. **The WAL (`wal.rs`, Step 2).** Frame as `[crc][len][payload]`; `append` writes, `sync` fsyncs, `replay` walks frames and stops at the first torn/corrupt one.
3. **The SSTable (`sstable.rs`, Steps 3–4).** `write` (data + index + footer to a temp file, fsync, rename, fsync dir), `open` (validate footer, load index), `get` (binary search + `pread`), `iter` (walk the data region).
4. **The store (`db.rs`, Steps 5–6).** `open_with` recovery (replay WAL, restore `seq` from WAL **and** SSTable `max_seq`), `get` (newest-wins), `put`/`delete` (seq → WAL append+sync → memtable → maybe_flush), `flush` (SSTable durable **then** WAL reset).
5. **Compaction (`compaction.rs`, Step 7).** Gather all records, keep highest-`seq` per key, drop surviving tombstones, `SsTable::write`.
6. **Run it.** `cargo test` all green, then drive the `kv` CLI and kill it between commands to watch recovery work.

### Hints

<details>
<summary>Hint for Step 1 (Record::encode / decode)</summary>

`encode` is a straight append of little-endian fields; a tombstone writes `vlen = 0` and no value bytes:

```rust
pub fn encode(&self) -> Vec<u8> {
    let (kind, value): (u8, &[u8]) = match &self.value {
        ValueKind::Put(v) => (KIND_PUT, v),
        ValueKind::Delete => (KIND_DELETE, &[]),
    };
    let mut out = Vec::new();
    out.extend_from_slice(&self.seq.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(self.key.len() as u32).to_le_bytes());
    out.extend_from_slice(&self.key);
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
    out
}
```

`decode` mirrors it using `buf.get(range)?` everywhere so a short buffer is a clean `None`, and returns how far it read:

```rust
let seq = u64::from_le_bytes(buf.get(0..8)?.try_into().ok()?);
// ...read kind, klen, key, vlen, value with the same `.get(..)?` guard...
Some((Record { seq, key, value }, pos))   // pos = total bytes consumed
```
</details>

<details>
<summary>Hint for Step 2 (WAL append / replay)</summary>

`append` builds one frame and writes it in one call; **don't** fsync here (that's `sync`):

```rust
let payload = record.encode();
let crc = crate::encoding::crc32(&payload);
let mut frame = Vec::new();
frame.extend_from_slice(&crc.to_le_bytes());
frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
frame.extend_from_slice(&payload);
self.file.write_all(&frame)?;   // needs `use std::io::Write;`
```

`replay` reads the whole file and walks frames, `break`ing (not erroring) at the first short or bad-CRC frame:

```rust
while pos + 8 <= buf.len() {
    let crc = u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap());
    let len = u32::from_le_bytes(buf[pos+4..pos+8].try_into().unwrap()) as usize;
    let (s, e) = (pos + 8, pos + 8 + len);
    if e > buf.len() { break; }                      // torn
    if crate::encoding::crc32(&buf[s..e]) != crc { break; }  // corrupt
    let (rec, _) = Record::decode(&buf[s..e]).ok_or... ; records.push(rec);
    pos = e;
}
```
</details>

<details>
<summary>Hint for Step 3 (SSTable write / open)</summary>

`write`: build the whole file in a `Vec<u8>` — data first (recording each record's `offset`/`len` into an `IndexEntry` and tracking `max_seq`), then the index region, then the fixed footer — and write it to `id.sst.tmp`, `sync_all`, `rename` onto `id.sst`, `sync_dir`. The rename is what makes it atomic. `index_offset` is just `data.len()` at the moment you finish the data region.

`open`: read the last `FOOTER_SIZE` bytes with `read_exact_at`, parse the five fields, check `magic == SST_MAGIC`, then `read_exact_at(index_offset, index_len)` and parse `[klen][key][offset][len]` entries into a `Vec<IndexEntry>` (already key-sorted on disk). Store `data_len = index_offset` so `iter` knows where the records end.
</details>

<details>
<summary>Hint for Step 4 (SSTable get)</summary>

The index is sorted by key, so binary-search it, then one positioned read of exactly that record:

```rust
let idx = match self.index.binary_search_by(|e| e.key.as_slice().cmp(key)) {
    Ok(i) => i,
    Err(_) => return Ok(None),
};
let e = &self.index[idx];
let bytes = self.read_at(e.offset, e.len as usize)?;   // given pread helper
let (rec, _) = Record::decode(&bytes).ok_or_else(|| Error::Corrupt("..".into()))?;
Ok(Some(rec))
```
`iter` is the same decode loop as WAL replay, over `self.read_at(0, self.data_len as usize)?`.
</details>

<details>
<summary>Hint for Step 5 (open / recovery)</summary>

The SSTable loading is given above the `todo!`. Your part:

```rust
let replayed = Wal::replay(&wal_path)?;
let mut max_seq = sstables.iter().map(|t| t.max_seq()).max().unwrap_or(0);
let mut mem = MemTable::new();
for rec in replayed { max_seq = max_seq.max(rec.seq); mem.insert(rec); }
let wal = Wal::open(&wal_path)?;
Ok(Db { dir, wal, mem, sstables, seq: max_seq + 1, next_sst_id, opts })
```
The `max` over **both** the replayed records and the SSTables' `max_seq` is the part that's easy to miss and breaks silently after a flush-then-crash.
</details>

<details>
<summary>Hint for Step 6 (put / flush)</summary>

`put`/`delete` are the durability dance — WAL first, and **synced**, before the memtable:

```rust
let seq = self.seq; self.seq += 1;
let record = Record::put(seq, key.to_vec(), value.to_vec());  // or ::tombstone
self.wal.append(&record)?;
self.wal.sync()?;              // durable now
self.mem.insert(record);
self.maybe_flush()
```

`flush` — SSTable durable **before** WAL reset:

```rust
if self.mem.is_empty() { return Ok(()); }
let records: Vec<Record> = self.mem.records().cloned().collect();  // already sorted
let path = SsTable::write(&self.dir, self.next_sst_id, &records)?; // fsync + rename inside
self.sstables.insert(0, SsTable::open(&path, self.next_sst_id)?);  // newest first
self.next_sst_id += 1;
self.mem.clear();
self.wal.reset()?;             // only now — data is safe on disk
```
</details>

<details>
<summary>Hint for Step 7 (compaction)</summary>

Full compaction lets you drop tombstones outright:

```rust
let mut all = Vec::new();
for t in tables { all.extend(t.iter()?); }
let live: Vec<Record> = newest_per_key(all)              // given helper: highest seq per key, sorted
    .into_iter()
    .filter(|r| !matches!(r.value, ValueKind::Delete))   // safe only because this is a FULL merge
    .collect();
SsTable::write(dir, new_id, &live)
```
`Db::compact` (given) handles making it durable, swapping it in, and deleting the old files in the safe order.
</details>

## Stretch goals

- **A Bloom filter per SSTable.** Drop your module 9 filter into each SSTable's footer; in `get`, skip a table whose filter says the key is absent. Measure the `get`-miss speedup with the benchmark — this is the single biggest real-world LSM read optimization.
- **Leveled compaction.** Instead of one full merge, keep size-tiered levels (L0, L1, …) and compact a table into the next level, only dropping tombstones at the bottom level. This is what makes compaction incremental instead of a stop-the-world rewrite.
- **Snapshot reads (real MVCC).** Add `snapshot() -> Seq` and a `get_at(key, seq)` that returns the newest version `≤ seq`. Prove a snapshot taken before a write doesn't see it.
- **Block-based SSTables + compression.** Group records into fixed-size blocks with a sparse index (one index entry per block, not per key), and compress each block. This is how a real SSTable keeps the index small enough to stay in memory for terabytes of data.
- **Group commit.** Batch several `put`s into one WAL `append` + single `fsync` and measure the throughput jump — the classic latency-vs-throughput trade every database exposes as a tunable.
- **Fault injection.** Write a test that truncates `wal.log` to a random length (a torn write) and asserts `open` recovers every record before the cut and no garbage after it.

## Key questions

- Your `put` fsyncs the WAL before touching the memtable. Walk through exactly what's lost if you reversed them and the machine died in between — and why the current order loses nothing.
- Why must `flush` fsync the new SSTable *before* it resets the WAL? Describe the crash window the wrong order opens, and what data disappears from *both* places.
- A `get` for a key that was deleted finds a tombstone in a recent SSTable but the original `Put` still sits in an older one. Why does returning "not found" require *stopping* at the tombstone rather than reading on — and why is it then safe for compaction to physically drop that tombstone here but not in a leveled store?
- On recovery you set the next `seq` to one past the max across the WAL *and* the SSTables. Construct the exact sequence of operations (put, flush, crash, put) where using only the WAL's max silently corrupts the store.
- Replaying the WAL twice must produce the same state. What property of the memtable insert makes recovery idempotent, and why does that let `open` blindly re-apply the whole log?
- The same workload runs faster on an LSM than a B-tree for writes but slower for reads. Explain both directions in terms of sequential-vs-random I/O and read amplification, and name the workload where you'd pick the B-tree anyway.

## Resources

- [Designing Data-Intensive Applications](https://dataintensive.net/), ch. 3 (Kleppmann) — the clearest treatment of LSM-trees vs B-trees, storage, and retrieval. Read it if you read nothing else.
- [The Log-Structured Merge-Tree](https://www.cs.umb.edu/~poneil/lsmtree.pdf) (O'Neil et al., 1996) — the original paper.
- [LevelDB implementation notes](https://github.com/google/leveldb/blob/main/doc/impl.md) and its [file format](https://github.com/google/leveldb/blob/main/doc/table_format.md) — this project is a stripped-down LevelDB; the real SSTable/table format is worth reading against yours.
- [RocksDB Wiki](https://github.com/facebook/rocksdb/wiki) — leveled compaction, WAL, MVCC/snapshots as they exist in a production engine.
- ["Files are hard"](https://danluu.com/file-consistency/) (Dan Luu) and ["Files are fraught with peril"](https://www.usenix.org/system/files/login/articles/login_winter16_08_pillai.pdf) — why fsync, ordering, and directory syncs are the real, gnarly problem, with the research to back it.
- [`std::os::unix::fs::FileExt`](https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html) (`read_exact_at` = `pread`) and [`File::sync_all`](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all) — the syscalls this project leans on, with their contracts spelled out.
- [Bitcask paper](https://riak.com/assets/bitcask-intro.pdf) — the simplest log-structured KV design (an in-memory index over an append-only log); a good contrast to the LSM here.
