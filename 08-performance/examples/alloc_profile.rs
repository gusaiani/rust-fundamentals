//! Allocation profiling (Pill 13). **Given.**
//!
//! Proves the optimized hot loop allocates `O(stations x threads)`, not
//! `O(rows)`. `dhat` installs itself as the global allocator and counts every
//! allocation; on exit it prints totals and writes `dhat-heap.json` (viewable at
//! <https://nnethercote.github.io/dh_view/dh_view.html>).
//!
//! ```bash
//! cargo run --release --example alloc_profile               # default 1M rows
//! cargo run --release --example alloc_profile -- 4000000    # 4x the rows
//! ```
//!
//! The test: run it at two row counts. Total blocks allocated should be ~flat —
//! it should **not** ~4x when the rows ~4x. If it scales with rows, you have a
//! hidden `to_vec`/`format!`/`collect` in the hot path; the `dhat` output (and
//! the JSON, in the viewer) points at the call site.

use std::hint::black_box;

use brc::runner::run_sequential;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Build an in-memory sample of `rows` lines (same shape as the real file).
fn sample(rows: usize) -> Vec<u8> {
    const STATIONS: &[&str] = &["Hamburg", "Bulawayo", "Palembang", "St. John's", "Zürich"];
    const TEMPS: &[&str] = &["12.0", "-3.4", "38.8", "0.0", "-99.9", "99.9", "5.5", "-0.5"];
    let mut buf = Vec::new();
    for i in 0..rows {
        buf.extend_from_slice(STATIONS[i % STATIONS.len()].as_bytes());
        buf.push(b';');
        buf.extend_from_slice(TEMPS[i % TEMPS.len()].as_bytes());
        buf.push(b'\n');
    }
    buf
}

fn main() {
    let rows: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    // Build the input *before* starting the profiler so its allocation doesn't
    // pollute the measurement — we only want the aggregation's allocations.
    let data = sample(rows);

    let _profiler = dhat::Profiler::new_heap();
    let result = run_sequential(black_box(&data));
    black_box(&result);
    // `_profiler` drops here and prints the allocation summary.

    eprintln!("aggregated {rows} rows into {} stations", result.len());
}
