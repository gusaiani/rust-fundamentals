# logcrunch

A parallel server-log analyzer CLI: map-reduce, channels, and rayon, benchmarked.

`logcrunch` ingests a large server-log file and prints a traffic report — totals, status-code distribution, top-K paths and IPs, error rate, and request-time percentiles. The same analysis is implemented four ways (sequential, scoped-thread map-reduce, crossbeam channel pipeline, and rayon) so you can compare them on the same input, with a `criterion` benchmark to put numbers on the difference.

## What it does

Reads a fixed space-delimited log format, one record per line, six fields:

```
<ip> <status> <bytes> <request_time_ms> <method> <path>
```

Malformed lines are counted and skipped, never fatal. The aggregate combines through an associative, commutative `Merge`, so every execution mode produces an identical report.

## Features

- Four interchangeable execution modes: `seq`, `par`, `pipeline`, `rayon`.
- Zero-copy, newline-aligned file chunking — no record straddles a thread boundary.
- Per-thread local accumulators merged once at the end — no `Mutex` in the hot loop.
- Bounded crossbeam channel in the pipeline mode for backpressure.
- Configurable thread count and top-K leaderboard size.
- `criterion` benchmark comparing the modes across thread counts.
- No CLI/arg-parsing dependencies; runtime deps are just `crossbeam-channel` and `rayon`.

## Example

```console
$ logcrunch tests/fixtures/sample.log --top 3 --mode seq
=== logcrunch report ===
requests:   9
malformed:  1
bytes:      18161
error rate: 22.22%

status codes:
  200  5
  301  1
  404  1
  500  1
  503  1

top paths:
         3  /api/users
         2  /api/checkout
         1  /

top IPs:
         4  10.0.0.5
         2  172.16.0.2
         2  192.168.1.9

request time:
  p50  9.3ms
  p95  250.5ms
  p99  250.5ms
```

## Running it

```bash
# Build
cargo build --release

# Run on the bundled fixture (mode defaults to par, top to 10,
# threads to available parallelism)
cargo run --release --bin logcrunch -- tests/fixtures/sample.log

# Pick a mode
cargo run --release --bin logcrunch -- tests/fixtures/sample.log --mode seq
cargo run --release --bin logcrunch -- tests/fixtures/sample.log --mode par
cargo run --release --bin logcrunch -- tests/fixtures/sample.log --mode pipeline
cargo run --release --bin logcrunch -- tests/fixtures/sample.log --mode rayon

# Tune thread count and leaderboard size
cargo run --release --bin logcrunch -- tests/fixtures/sample.log --mode par --threads 4 --top 5
```

CLI: `logcrunch <FILE> [--threads N] [--top K] [--mode seq|par|pipeline|rayon]`.
Defaults: `--threads` = available parallelism, `--top` = 10, `--mode` = `par`.

Generate a large input to actually exercise the parallelism (the example takes a line count, default 1,000,000, and writes to stdout):

```bash
cargo run --release --example gen_log -- 2000000 > big.log
cargo run --release --bin logcrunch -- big.log --mode par --threads 8
```

Tests and benchmark:

```bash
cargo test
cargo bench          # criterion bench "throughput", baseline vs each mode
```

## How it works

Four modes, one shared `Stats` aggregate and associative `Merge`:

- **`seq`** — single-threaded baseline; the correctness oracle and the speedup denominator.
- **`par`** — `std::thread::scope` over newline-aligned `&[u8]` chunks; each thread builds a local `Stats`, all merged once at the end.
- **`pipeline`** — a reader feeds line batches into a bounded crossbeam channel; N worker threads consume, aggregate locally, and their results are merged.
- **`rayon`** — the same map-reduce as `par_iter().fold(...).reduce(...)` on rayon's work-stealing pool.

The aggregate stores counts and per-key maps (status, path, IP) plus the raw request-time samples — percentiles don't compose, so they're computed by sorting at the end. The merge is associative and commutative, which is what lets nondeterministic thread finish order still yield a deterministic report.

## Project layout

```
src/
  bin/logcrunch.rs   CLI: arg parsing, dispatch on --mode, print report
  lib.rs             module wiring + re-exports
  parser.rs          LogEntry, ParseError, parse_line
  stats.rs           Stats, Merge trait, percentile, Report
  sequential.rs      analyze_sequential
  parallel.rs        split_into_chunks + analyze_parallel
  pipeline.rs        analyze_pipeline
  rayon_impl.rs      analyze_rayon
examples/gen_log.rs  synthetic-log generator
benches/throughput.rs   criterion: baseline vs each mode
tests/               parser, merge associativity, parallel == sequential
```

## Status

Implemented and runnable; built as a teaching exercise for the concurrency module.

The concept pills and the step-by-step build that produced this — covering threads and scoped threads, `Send`/`Sync`, channels and backpressure, work-stealing, rayon, and benchmarking with criterion — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
