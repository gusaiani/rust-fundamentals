//! Speedup benchmark — the required deliverable for this module.
//!
//! Run: `cargo bench`
//!
//! This compiles against the stubbed library, so it will *panic* until you've
//! implemented the analyze functions — that's expected. Once they work, read
//! the scaling curve criterion prints: near-linear up to physical cores, then
//! flat. Where it flattens is your Amdahl ceiling (Pill 15).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use logcrunch::parallel::analyze_parallel;
use logcrunch::pipeline::analyze_pipeline;
use logcrunch::rayon_impl::analyze_rayon;
use logcrunch::sequential::analyze_sequential;

/// Build an in-memory log once, outside the measured loop.
fn make_log(lines: usize) -> Vec<u8> {
    let mut s = String::with_capacity(lines * 40);
    let ips = ["10.0.0.5", "192.168.1.9", "172.16.0.2"];
    let paths = ["/", "/api/users", "/favicon.ico", "/health"];
    for i in 0..lines {
        let ip = ips[i % ips.len()];
        let status = if i % 20 == 0 { 500 } else { 200 };
        let path = paths[i % paths.len()];
        s.push_str(&format!(
            "{ip} {status} {} {}.{} GET {path}\n",
            (i * 7) % 50000,
            i % 200,
            i % 10
        ));
    }
    s.into_bytes()
}

fn bench(c: &mut Criterion) {
    let data = make_log(500_000);
    let mut group = c.benchmark_group("analyze");

    group.bench_function("sequential", |b| {
        b.iter(|| analyze_sequential(black_box(&data)))
    });

    for n in [1usize, 2, 4, 8] {
        group.bench_with_input(BenchmarkId::new("parallel", n), &n, |b, &n| {
            b.iter(|| analyze_parallel(black_box(&data), n))
        });
        group.bench_with_input(BenchmarkId::new("pipeline", n), &n, |b, &n| {
            b.iter(|| analyze_pipeline(black_box(&data), n))
        });
    }

    group.bench_function("rayon", |b| {
        b.iter(|| analyze_rayon(black_box(&data)))
    });

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
