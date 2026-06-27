# aprobe

An async network probe: a concurrent TCP port scanner and a graceful TCP proxy, built on tokio.

One CLI, two subcommands sharing the same async toolkit — bounded concurrency, timeouts-as-cancellation, and graceful shutdown. The point is *concurrency without parallelism*: thousands of sockets parked at their `.await` points, interleaved by a couple of OS threads, not one thread per socket.

## What it does

- **`scan`** opens up to `--concurrency` TCP connections at once (capped by a `Semaphore`), each `connect` wrapped in a `timeout`, and classifies every port `Open` / `Closed` / `Filtered`.
- **`proxy`** accepts connections, dials an upstream, and shuttles bytes both ways with `copy_bidirectional`, draining in-flight connections on Ctrl-C instead of dropping them mid-write.

## Features

- Hand-rolled arg parsing (no clap) — keeps the dependency list honest.
- Target spec `host:ports`, where `ports` is a comma-separated list of single ports and inclusive ranges (`80-90,443,8080`). Parsing is pure; DNS resolution happens at scan time.
- Three-way port classification: `Open` (connected), `Closed` (RST/refused), `Filtered` (timed out — the connect future is dropped, which *is* the cancel).
- Bounded concurrency via `Semaphore` + `JoinSet`; the `acquire_owned().await` in the spawn loop is the backpressure, so a `/1-65535` scan never queues 65k tasks.
- Graceful, *bounded* proxy shutdown: stop accepting, drain in-flight up to a 1s deadline, then abort stragglers.
- Scaling benchmark (`criterion` with `async_tokio`): sequential vs concurrent at 16/64/256.

## Commands

### scan

```console
$ aprobe scan 127.0.0.1:8995-9005 --timeout-ms 300
scanning 11 port(s) on 127.0.0.1 (concurrency 256, timeout 300ms)
  9000/tcp  open      (332.167µs)
done: 1 open, 0 filtered, 11 total
```

Defaults: `--concurrency 256`, `--timeout-ms 500`. Only open ports are printed to stdout; the progress and summary lines go to stderr. Closed/filtered ports are the boring majority and are counted, not listed.

### proxy

```console
$ aprobe proxy --listen 127.0.0.1:8080 --upstream 127.0.0.1:9000
aprobe proxy: 127.0.0.1:8080 -> 127.0.0.1:9000 (Ctrl-C to drain & exit)
^C
aprobe proxy: shutdown requested, draining…
```

Both `--listen` and `--upstream` are required.

## Running it

```bash
# build
cargo build --release

# scan a local range (start the echo server first for an open port)
cargo run --example echo_server -- 127.0.0.1:9000          # terminal 1
cargo run -- scan 127.0.0.1:8990-9010                      # terminal 2 — 9000 shows open

# scan a remote host
cargo run -- scan scanme.nmap.org:20-1024 --concurrency 512 --timeout-ms 300

# run the proxy in front of the echo server (Ctrl-C to drain & exit)
cargo run -- proxy --listen 127.0.0.1:8080 --upstream 127.0.0.1:9000
printf 'hello\n' | nc 127.0.0.1 8080                       # round-trips through the proxy

# tests and the scaling benchmark
cargo test
cargo bench
```

## How it works

`#[tokio::main]` builds the multi-threaded work-stealing runtime and `block_on`s `main`'s future — the one place the sync world meets the async one.

- **Scanner** (`src/scanner.rs`): `scan` shares an `Arc<Semaphore>` across a `JoinSet`. The spawn loop `acquire_owned().await`s a permit *before* spawning, so it parks once `concurrency` probes are live (backpressure). Each task runs `probe_port` — a `TcpStream::connect` wrapped in `tokio::time::timeout` — then drops its permit. Outcomes drain via `join_next().await` and are sorted by port. `scan_sequential` is the one-at-a-time baseline and correctness oracle.
- **Proxy** (`src/proxy.rs`): `run_proxy` is a `select!` accept loop racing `listener.accept()` against `shutdown.cancelled()`. Each connection is spawned bare into a `JoinSet` (not awaited inline — that would serialize them). On the shutdown arm the loop breaks and drains the `JoinSet` under a `DRAIN_TIMEOUT`, aborting only stragglers. `handle_conn` dials upstream and pumps both directions with `copy_bidirectional`.
- **Shutdown** (`src/shutdown.rs`): a clonable handle over `tokio::sync::watch<bool>` — `trigger` flips the flag, `cancelled().await` resolves when it does. The CLI wires `tokio::signal::ctrl_c()` to `trigger`.

The win is overlapping I/O wait, not extra cores: 256 in-flight connects are 256 parked futures on the same ~2 runtime threads. `cargo bench` measures it.

## Project layout

```text
src/
  bin/aprobe.rs   CLI: subcommand dispatch, arg parsing, Ctrl-C wiring, report printing
  lib.rs          module declarations + re-exports
  target.rs       Target / PortState / ScanOutcome / TargetError; pure parse_target
  scanner.rs      ScanConfig, probe_port, scan (Semaphore + JoinSet), scan_sequential
  proxy.rs        run_proxy (select! accept loop), handle_conn (copy_bidirectional)
  shutdown.rs     Shutdown — a watch-based cancellation handle
examples/echo_server.rs   a fully-written async echo server to scan/proxy against
benches/scan.rs           criterion: sequential vs concurrent (16/64/256)
tests/integration.rs      parser, scan open/closed/filtered, proxy forwards-then-drains
```

## Status

Implemented and runnable — built as a teaching exercise for async Rust and tokio.

The concept pills and the step-by-step build that produced this — covering futures, the tokio runtime, `Pin`/`Waker`, `select!`, cancellation, `Semaphore`/`JoinSet`, and graceful shutdown — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
