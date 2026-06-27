//! The deliverable benchmark (Pill 13): how fast can we frame the wire protocol
//! and serve the keyspace?
//!
//! This file is **given**. Run it *after* you implement the parser and store:
//!
//! ```bash
//! cargo bench
//! ```
//!
//! Two questions it answers:
//!   - **RESP parse throughput** — how many commands/sec one core can frame.
//!     This is the ceiling the event loop works under: parsing is the per-command
//!     CPU cost on the read path.
//!   - **Store GET/SET** — the in-memory op cost, which should be a rounding
//!     error next to the syscalls around it. That contrast *is* the lesson: for
//!     an in-memory store the bottleneck is I/O and framing, not the data
//!     structure — which is exactly why real Redis is single-threaded.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use rudis::resp::Resp;
use rudis::store::Store;

/// Build a buffer of `n` back-to-back `SET key<i> val<i>` requests.
fn build_requests(n: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    for i in 0..n {
        let key = format!("key{i}");
        let val = format!("val{i}");
        buf.extend_from_slice(format!("*3\r\n$3\r\nSET\r\n").as_bytes());
        buf.extend_from_slice(format!("${}\r\n{}\r\n", key.len(), key).as_bytes());
        buf.extend_from_slice(format!("${}\r\n{}\r\n", val.len(), val).as_bytes());
    }
    buf
}

fn bench_parse(c: &mut Criterion) {
    let n = 1_000;
    let buf = build_requests(n);

    let mut group = c.benchmark_group("resp_parse");
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function("parse_1k_set_commands", |b| {
        b.iter(|| {
            let mut rest: &[u8] = black_box(&buf);
            let mut count = 0usize;
            while let Some((value, consumed)) = Resp::parse(rest).unwrap() {
                black_box(&value);
                rest = &rest[consumed..];
                count += 1;
            }
            assert_eq!(count, n);
        });
    });
    group.finish();
}

fn bench_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("store");
    group.bench_function("set", |b| {
        let mut store = Store::new(None);
        let mut i = 0u64;
        b.iter(|| {
            let key = format!("k{}", i % 10_000);
            store.set(key.into_bytes(), b"value".to_vec(), None);
            i += 1;
        });
    });
    group.bench_function("get_hit", |b| {
        let mut store = Store::new(None);
        store.set(b"k".to_vec(), b"value".to_vec(), None);
        b.iter(|| {
            black_box(store.get(black_box(b"k")));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parse, bench_store);
criterion_main!(benches);
