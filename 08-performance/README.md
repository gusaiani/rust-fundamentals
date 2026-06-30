# brc

A One Billion Row Challenge solver in Rust — naive baseline to 10x+, every win proven by a benchmark.

Read a text file of `station;temperature` lines and print the min, mean, and max temperature per station, sorted alphabetically. The workload is trivial on purpose: the only variable is how fast you execute it. This crate is a **build-it-yourself performance lab** — a scaffold with the hard parts left as `todo!()` stubs for you to implement, measure, and optimize.

## What it does

- Reads a 1BRC measurements file (`station;temperature`, one per line) and prints `{Station=min/mean/max, ...}`, sorted, rounded to one decimal.
- Ships a deterministic, seeded data generator (`gen`) so you can produce input files of any size.
- Ships a criterion benchmark, a dhat allocation profile, and integration tests to drive and verify the optimization work.

## What you'll build

- **Zero-copy parsing** (`parse.rs`): split each line into a `&[u8]` name and temperature, parse the temperature as a fixed-point `i32` of tenths — no `String`, no `f64` in the hot loop.
- **An integer accumulator** (`aggregate.rs`): `Stats` keeping min/max/sum/count, plus merge and output formatting.
- **A fast hasher** (`hash.rs`): an FxHash-style `Hasher` and a `FastMap` alias, replacing std's SipHash.
- **Newline-aligned chunking** (`io.rs`): `split_chunks` tiles the buffer into per-core ranges that never cut a line.
- **The drivers** (`runner.rs`): `run_sequential` (single core) and `run_parallel` (`thread::scope`, one worker per chunk, merge the maps).
- **The CLI** (`bin/brc.rs`): wire args to a run, time it, print the result.

## Running it

Generate data first — `gen` is fully implemented and works today:

```bash
# gen <count> [path]   (path defaults to measurements.txt)
cargo run --release --bin gen -- 1000000 measurements.txt      # 1M rows
cargo run --release --bin gen -- 1000000000 measurements.txt   # the full ~13 GB
```

Run the solver. **This panics (`not yet implemented`) until you fill in the stubs** — `parse`, `aggregate`, `hash`, `io::split_chunks`, and `runner`:

```bash
# brc <file> [--threads N]   (defaults to one worker per available core)
cargo run --release --bin brc -- measurements.txt
cargo run --release --bin brc -- measurements.txt --threads 1   # force the single-core path
```

The rest also depend on the implementation and **fail/panic until the stubs are done**:

```bash
cargo test                                      # unit + integration tests
cargo bench                                     # criterion: parse_temp vs f64, aggregate throughput
cargo run --release --example alloc_profile     # dhat: prove the hot loop is allocation-flat
```

What works now without any implementation: `cargo check --all-targets`, and `cargo run --release --bin gen`.

## How it works

The intended optimization path, from baseline to 10x+:

- **mmap** the file (`memmap2`) so there's no `read` syscall in the loop and no copy into a userspace buffer — you parse straight out of the page cache as a `&[u8]`.
- **Zero-copy keys**: the station name is a slice into the mmap, never a heap `String`; owned keys are cloned only on the ~400 misses, never per row.
- **Integer temperatures**: parse the fixed `-?dd.d` format as an `i32` count of tenths — no float parsing, no float accumulation drift.
- **Fast hashing**: an FxHash-style hasher (multiply + xor per word) instead of DoS-resistant SipHash, safe here because the keys come from a file you control.
- **SIMD scanning**: `memchr` vectorizes finding `;` and `\n` with no intrinsics of your own.
- **Hand-rolled parallelism**: `split_chunks` cuts the buffer on newline boundaries, `thread::scope` fans one worker per core (each borrowing the mmap with no `Arc`), and a cheap merge folds the per-thread maps.

Depth — flamegraphs, branch prediction, the memory hierarchy, hand-written SIMD — lives in the learn file.

## Project layout

| File | Status |
| --- | --- |
| `Cargo.toml` | Given — deps, both bins, the bench, the release profile (`lto`, `codegen-units = 1`, `panic = "abort"`). |
| `src/lib.rs` | Given — module map and crate docs. |
| `src/bin/gen.rs` | Given — seeded, deterministic data generator. Works. |
| `src/io.rs` | `map_file` given; `split_chunks` is a TODO stub. |
| `src/parse.rs` | TODO — `split_line`, `parse_temp`. |
| `src/aggregate.rs` | TODO — `Stats::record`/`merge`/`mean`, `format_results` (`into_sorted` given). |
| `src/hash.rs` | TODO — `FxHasher::add`/`write` (the `FastMap` alias is wired). |
| `src/runner.rs` | TODO — `aggregate`, `run_sequential`, `run_parallel`. |
| `src/bin/brc.rs` | Arg parsing and timing given; the run/format/print is a TODO stub. |
| `benches/aggregate.rs` | Given — criterion bench. |
| `examples/alloc_profile.rs` | Given — dhat allocation profile. |
| `tests/integration.rs` | Given — correctness + sequential-vs-parallel agreement. |

## Status

Complete. Every library stub is implemented — zero-copy parsing, integer temperatures, the FxHash hasher, newline-aligned chunking, and both the sequential and `thread::scope` parallel drivers. `cargo test` passes all suites (unit + integration, including the parallel-vs-sequential agreement check).

## Results

The same binary, measured two ways on a 10-core machine (32 GB RAM). `v4` is the single-core stack (mmap + zero-copy + integer parse + FxHash + memchr, run with `--threads 1`); `v5` adds parallelism across all cores.

| Version | Change | 1M rows (warm cache) | 1B rows (~13 GB) |
| --- | --- | --- | --- |
| v0 | naive `BufReader` + `String` + `f64` + SipHash | 71 ms — 1× | 74.7 s — 1× |
| v4 | mmap + zero-copy + integer temps + FxHash + memchr | 21 ms — 3.3× | 39.3 s — 1.9× |
| v5 | + parallel (10 threads) | 4.0 ms — 17.8× | 10.2 s — 7.3× |

The two columns tell different stories because the bottleneck moves with the input size. On 1M rows the 12.6 MB file lives in cache, so the work is CPU-bound and every compute optimization pays full freight (17.8×). On 1B rows the 12.6 GB file no longer fits comfortably in cache, so the scan becomes memory-bandwidth and page-fault bound — parallel ran at ~285% CPU, not ~1000%, and the curve compresses to 7.3×. A speedup is not a property of the code alone; it is a property of the code *and* the regime it runs in. The 1B figures are an indicative floor (measured on a busy machine), not a tuned best.

The concept pills and the step-by-step build — covering criterion, flamegraphs, mmap, zero-copy parsing, hashing, branch prediction, SIMD, and parallelism — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [MIT license](https://opensource.org/licenses/MIT) or [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
</content>
</invoke>
