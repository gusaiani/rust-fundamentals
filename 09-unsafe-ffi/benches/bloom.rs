//! Criterion benchmarks for the hot operations: `add` and `contains`.
//!
//! A Bloom filter's whole value proposition is speed-and-space, so the numbers
//! matter: each op is `k` hashes plus `k` bit probes, and that should land in
//! tens of nanoseconds. `black_box` keeps the optimizer from folding the work
//! away. These compile against the stubs and run once they're implemented
//! (`cargo bench`).

use cbloom::Bloom;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

fn bench_add(c: &mut Criterion) {
    let keys: Vec<String> = (0..10_000).map(|i| format!("session-token-{i}")).collect();
    c.bench_function("add_10k_keys", |b| {
        b.iter_batched(
            || Bloom::new(10_000, 0.01),
            |mut filter| {
                for k in &keys {
                    filter.add(black_box(k.as_bytes()));
                }
                filter
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_contains(c: &mut Criterion) {
    let mut filter = Bloom::new(10_000, 0.01);
    for i in 0..10_000 {
        filter.add(format!("session-token-{i}").as_bytes());
    }

    // Present keys: all k bits set, so the loop runs the full k probes (worst
    // case for a hit). Absent keys: usually short-circuit on the first clear bit.
    c.bench_function("contains_hit", |b| {
        let key = b"session-token-5000";
        b.iter(|| black_box(filter.contains(black_box(key))));
    });
    c.bench_function("contains_miss", |b| {
        let key = b"definitely-not-present";
        b.iter(|| black_box(filter.contains(black_box(key))));
    });
}

criterion_group!(benches, bench_add, bench_contains);
criterion_main!(benches);
