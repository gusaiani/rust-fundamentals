//! The micro-benchmark (Pill 1). **Given.** Run after the stubs are implemented:
//!
//! ```bash
//! cargo bench
//! ```
//!
//! It compiles against the stubbed library, so it will *panic* (`not yet
//! implemented`) until you've filled in `parse_temp`, `split_line`, `Stats`, the
//! hasher, and `run_sequential` — that's expected.
//!
//! Three numbers, each isolating one cost:
//!   - **`parse/parse_temp`** — the per-temperature cost. The integer parse
//!     (Pill 7) should be a handful of nanoseconds. Compare it to `f64::parse`
//!     on the same input (also measured) to *see* why you abandoned the float.
//!   - **`parse/f64_parse`** — the same values through `f64::parse`, for contrast.
//!   - **`aggregate/run_sequential`** — the whole pipeline over an in-memory
//!     sample: mmap-free, pure CPU (parse + hash + record). This is the number
//!     the parallel path multiplies.
//!
//! `black_box` (Pill 1) stops the optimizer from folding away work whose result
//! is unused — without it these benches would measure nothing.

use std::hint::black_box;

use brc::parse::parse_temp;
use brc::runner::run_sequential;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

/// A fixed in-memory sample shaped like the real file: a handful of stations,
/// mixed signs, one- and two-digit magnitudes.
fn sample(rows: usize) -> Vec<u8> {
    const STATIONS: &[&str] = &["Hamburg", "Bulawayo", "Palembang", "St. John's", "Zürich"];
    // Cycle through a fixed set of temperatures so the data is deterministic.
    const TEMPS: &[&str] = &["12.0", "-3.4", "38.8", "0.0", "-99.9", "99.9", "5.5", "-0.5"];
    let mut buf = Vec::new();
    for i in 0..rows {
        let name = STATIONS[i % STATIONS.len()];
        let temp = TEMPS[i % TEMPS.len()];
        buf.extend_from_slice(name.as_bytes());
        buf.push(b';');
        buf.extend_from_slice(temp.as_bytes());
        buf.push(b'\n');
    }
    buf
}

fn bench_parse(c: &mut Criterion) {
    let inputs: &[&[u8]] = &[b"12.0", b"-3.4", b"38.8", b"0.0", b"-99.9", b"99.9", b"5.5", b"-0.5"];

    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Elements(inputs.len() as u64));

    group.bench_function("parse_temp", |b| {
        b.iter(|| {
            for &s in inputs {
                black_box(parse_temp(black_box(s)));
            }
        });
    });

    // The float parser doing the same job — the contrast Pill 7 is about.
    group.bench_function("f64_parse", |b| {
        b.iter(|| {
            for &s in inputs {
                let txt = std::str::from_utf8(black_box(s)).unwrap();
                black_box(txt.parse::<f64>().unwrap());
            }
        });
    });

    group.finish();
}

fn bench_aggregate(c: &mut Criterion) {
    let rows = 100_000;
    let data = sample(rows);

    let mut group = c.benchmark_group("aggregate");
    group.throughput(Throughput::Elements(rows as u64));
    group.bench_function("run_sequential", |b| {
        b.iter(|| {
            black_box(run_sequential(black_box(&data)));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parse, bench_aggregate);
criterion_main!(benches);
