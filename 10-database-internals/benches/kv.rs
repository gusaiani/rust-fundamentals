//! Criterion benchmarks for the store. **Given.**
//!
//! Run with `cargo bench` once the stubs are implemented. Three things worth
//! measuring on a write-optimized engine:
//!
//! - **`put` throughput** — dominated by the per-write `fsync` on the WAL. This
//!   is the number that shows why LSM trees exist: writes are a sequential
//!   append, not a random in-place update.
//! - **`get` hit** vs **`get` miss** — a hit binary-searches an SSTable index and
//!   does one positioned read; a miss may probe every table. Comparing them shows
//!   the read-amplification a Bloom filter (stretch goal) would cut down.
//!
//! Each benchmark builds a fresh store in a temp dir so runs are independent.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use lsmkv::{Db, Options};
use tempfile::tempdir;

/// A store with a large flush threshold, so the benchmark isn't dominated by
/// flush timing (we're measuring the write path, not disk flush cadence).
fn bench_db(dir: &std::path::Path) -> Db {
    Db::open_with(dir, Options { memtable_flush_bytes: 64 << 20 }).unwrap()
}

fn bench_put(c: &mut Criterion) {
    let mut group = c.benchmark_group("put");
    group.throughput(Throughput::Elements(1));
    group.bench_function("wal_synced_put", |b| {
        let dir = tempdir().unwrap();
        let mut db = bench_db(dir.path());
        let mut i = 0u64;
        b.iter(|| {
            let key = format!("key{i}");
            db.put(black_box(key.as_bytes()), black_box(b"some-value")).unwrap();
            i += 1;
        });
    });
    group.finish();
}

fn bench_get(c: &mut Criterion) {
    // Populate a store, flush to SSTables, then measure hits and misses.
    let dir = tempdir().unwrap();
    let mut db = bench_db(dir.path());
    let n = 50_000u64;
    for i in 0..n {
        db.put(format!("key{i}").as_bytes(), b"value").unwrap();
    }
    db.flush().unwrap();

    let mut group = c.benchmark_group("get");
    group.throughput(Throughput::Elements(1));
    group.bench_function("hit", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let key = format!("key{}", i % n);
            black_box(db.get(black_box(key.as_bytes())).unwrap());
            i += 1;
        });
    });
    group.bench_function("miss", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let key = format!("absent{i}");
            black_box(db.get(black_box(key.as_bytes())).unwrap());
            i += 1;
        });
    });
    group.finish();
}

criterion_group!(benches, bench_put, bench_get);
criterion_main!(benches);
