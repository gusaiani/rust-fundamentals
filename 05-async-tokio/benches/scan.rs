//! Scaling benchmark — the required deliverable for this module.
//!
//! Run: `cargo bench`
//!
//! It compiles against the stubbed library, so it will *panic* until you've
//! implemented `scan` / `scan_sequential` — that's expected. Once they work,
//! read the curve: sequential pays every closed/filtered port's latency in
//! series; concurrency 16→64→256 overlaps that wait. The win here is *not*
//! more cores (it's the same 2-ish runtime threads) — it's overlapping I/O
//! wait. That's the entire async thesis, on a graph.
//!
//! We bench against `127.0.0.1` over a port range that is almost entirely
//! closed (nothing listening). Closed ports RST fast, so to make the latency
//! visible we set a generous timeout and a wide range — the sequential sum of
//! many small connect latencies still dwarfs the concurrent overlap.

use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime::Runtime;

use aprobe::scanner::{scan, scan_sequential, ScanConfig};
use aprobe::target::Target;

/// A range of localhost ports that are (almost certainly) all closed.
fn make_target() -> Target {
    Target {
        host: "127.0.0.1".to_string(),
        ports: (20_000u16..20_500).collect(),
    }
}

fn bench(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let target = make_target();
    let timeout = Duration::from_millis(200);

    let mut group = c.benchmark_group("scan");
    // These probes mostly RST instantly; keep sample sizes modest so the
    // whole bench finishes in a sane time.
    group.sample_size(20);

    group.bench_function("sequential", |b| {
        let cfg = ScanConfig { concurrency: 1, timeout };
        b.to_async(&rt)
            .iter(|| async { scan_sequential(&target, &cfg).await });
    });

    for n in [16usize, 64, 256] {
        group.bench_with_input(BenchmarkId::new("concurrent", n), &n, |b, &n| {
            let cfg = ScanConfig { concurrency: n, timeout };
            b.to_async(&rt).iter(|| async { scan(&target, &cfg).await });
        });
    }

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
