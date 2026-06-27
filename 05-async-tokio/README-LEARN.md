# Async Rust & Tokio in 5-Minute Pills

## Goal

Build a network tool that holds *thousands* of TCP connections open at once on a couple of OS threads — a concurrent port scanner and a graceful TCP proxy — and understand exactly why that works: futures, the runtime that polls them, `select!`, and cancellation. Then prove the concurrency win with a benchmark instead of asserting it.

## Time estimate

~1 day (15 pills × 5 min + project)

## What you'll learn

- What a `Future` actually is — a poll-able state machine that does *nothing* until awaited
- How `async`/`.await` desugars, why futures are lazy, and who calls `poll`
- The tokio runtime: the multi-threaded work-stealing executor, `#[tokio::main]`, and the cardinal sin of blocking it
- `Pin` and the `Waker` — the two pieces of machinery that make "park this socket and wake me when it's readable" sound
- Concurrency vs. parallelism in async — `join!`, `select!`, and why one task can drive thousands of connections
- Cancellation as *dropping a future*: `tokio::time::timeout`, and what "cancellation safety" means
- Bounded concurrency with `Semaphore`, result funnelling with `mpsc`, and structured concurrency with `JoinSet`
- Graceful shutdown — racing real work against a shutdown signal so the program *drains* instead of dying mid-write

## Concepts

### Pill 1: What a `Future` Is

A `Future` is a value that represents *a computation that isn't done yet*. The whole trait, stripped down:

```rust
trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

enum Poll<T> { Ready(T), Pending }
```

Something calls `poll`. The future either returns `Ready(value)` (done — here's your `Output`) or `Pending` (not yet — I've arranged to be woken when there's progress). That's it. A `Future` is not a thread, not a green thread, not a callback — it's a struct with a `poll` method that gets called repeatedly until it says `Ready`.

The consequence that trips up everyone: **futures are lazy**. `async { do_thing() }` runs *nothing*. It builds a future. The body executes only when something polls it to completion — which is what `.await` (Pill 2) and the runtime (Pill 3) arrange. A future you build and drop without awaiting never ran. (Compare a thread: `spawn` runs immediately.)

### Pill 2: `async`/`.await` — Desugaring to a State Machine

`async fn` and `async {}` are compiler sugar. The compiler rewrites the body into an anonymous struct implementing `Future`, where each `.await` is a *suspension point* — a state the machine can pause at and resume from:

```rust
async fn fetch() -> u64 {
    let a = step_one().await;   // <- suspension point 1
    let b = step_two(a).await;  // <- suspension point 2
    a + b
}
```

becomes (morally) an enum: `Start → AwaitingOne → AwaitingTwo → Done`. Each `poll` advances as far as it can, then returns `Pending` at the next `.await` whose inner future isn't ready. The local variables that must survive across an `.await` (`a`, here) get stored *in the generated struct* — which is why the struct can be self-referential, which is why Pill 4 exists.

`.await` itself means: poll the inner future; if `Ready`, take the value and continue; if `Pending`, return `Pending` from *this* future too (propagating the suspension up to the runtime). It's the async analogue of `?` — short-circuit upward, but on "not ready" instead of "error."

### Pill 3: The Runtime — Who Actually Calls `poll`

Futures are inert. Something has to poll them: the **runtime** (a.k.a. executor). `tokio` is the dominant one. `#[tokio::main]` is sugar for "build a multi-threaded runtime and `block_on` this future":

```rust
#[tokio::main]
async fn main() { /* ... */ }

// expands to roughly:
fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()                 // wire up the I/O + timer drivers
        .build().unwrap()
        .block_on(async { /* ... */ })
}
```

The runtime has two halves: a **scheduler** (a pool of worker threads that poll ready tasks) and **reactors/drivers** (epoll/kqueue/IOCP for sockets, a timer wheel for `sleep`). When a socket's future returns `Pending`, the reactor registers interest in that file descriptor; when the OS says "readable," the reactor wakes the task and the scheduler re-polls it. `block_on` is the bridge from the synchronous world (`main`) into the async one — the single place a real program turns the crank.

### Pill 4: `Pin` — Why Futures Can't Move

The generated state machine (Pill 2) stores variables that live across `.await`. Some of those are *borrows of other locals in the same struct* — the future is **self-referential**: a field holds a pointer into another field. If you moved that struct in memory, the pointer would dangle. So the runtime needs a guarantee: *once I start polling a future, it won't move.*

`Pin<P>` is that guarantee in the type system. `Pin<&mut F>` means "a mutable reference to an `F` that promises `F` won't be moved out." That's why `poll`'s receiver is `self: Pin<&mut Self>`. You rarely construct `Pin` by hand — `tokio::spawn`, `.await`, and `Box::pin` do it for you. You *feel* `Pin` mainly when a compiler error says a future "cannot be unpinned" or you need `tokio::pin!(fut)` to `.await` a future in a `select!` loop. Knowing *why* (self-referential state machines) turns that error from mysterious to obvious.

### Pill 5: The `Waker` — No Busy-Looping

If `poll` returns `Pending`, how does the runtime know when to try again? It does **not** spin-loop. The `Context` passed to `poll` carries a `Waker`. The future (really, the reactor it registered with) stashes the waker and, when the underlying resource is ready, calls `waker.wake()` — which tells the scheduler "this task can make progress, queue it for polling."

```text
poll() -> Pending          (socket not readable; reactor saves the Waker)
   ... task sleeps, zero CPU ...
OS: fd 7 readable -> reactor calls waker.wake()
scheduler re-queues the task -> poll() -> Ready(bytes)
```

This is the entire efficiency story of async I/O: a `Pending` task consumes **no CPU and no thread** while it waits. Ten thousand connections blocked on `read` are ten thousand parked futures and ~zero running threads — versus ten thousand OS threads (8 MB of stack each, scheduler thrash) in the thread-per-connection model. That asymmetry is why you're learning this.

### Pill 6: `tokio::spawn` — Tasks

A **task** is a top-level future the runtime owns and drives independently — the async analogue of a thread. `tokio::spawn` hands a future to the scheduler and returns a `JoinHandle<T>` you can `.await` for its output:

```rust
let handle = tokio::spawn(async move {
    expensive_async_work().await        // runs concurrently with the spawner
});
let result = handle.join_handle_value().await; // i.e. handle.await -> Result<T, JoinError>
```

Spawned futures must be `Send + 'static` — same reason as `thread::spawn` in Module 4: the task can be moved between worker threads and can outlive the spawner, so it must own its captures (`move`) and contain nothing thread-local. `spawn` is how you get *parallelism* (tasks on different worker threads), as opposed to the in-task *concurrency* of `join!`/`select!` (Pill 8). Awaiting a `JoinHandle` yields `Result<T, JoinError>` — the `Err` is how a panicked task reports in, instead of unwinding into you.

### Pill 7: Never Block the Executor

Each runtime worker thread polls many tasks. A `poll` is supposed to be *quick* — make progress, hit the next `.await`, yield. If one task does something **synchronously blocking** — `std::thread::sleep`, a blocking file read, a CPU-bound 200 ms loop, `std::sync::Mutex` held across `.await` — that worker thread can't poll anything else. A handful of blocked workers and your whole runtime stalls; thousands of "concurrent" connections go silent.

Rules:
- Use the **async** version of everything in async code: `tokio::time::sleep`, `tokio::fs`, `tokio::sync::Mutex` (when held across `.await`).
- For unavoidable blocking or heavy CPU work, hand it to `tokio::task::spawn_blocking(|| ...)`, which runs it on a separate pool reserved for exactly this, keeping the async workers free.

"Don't block the executor" is the #1 async footgun. The symptom — mysterious latency spikes under load — is miserable to debug, so internalize the rule now.

### Pill 8: Concurrency ≠ Parallelism

These are different axes, and async makes the difference concrete:

- **`join!`** drives multiple futures **concurrently on one task** (one thread). It polls them in turn; while one is `Pending` the others make progress. No parallelism — but the *waits overlap*. Two 500 ms network calls under `join!` finish in ~500 ms, not 1000.

```rust
let (a, b) = tokio::join!(fetch_user(id), fetch_orders(id)); // overlapped, same task
```

- **`tokio::spawn`** (Pill 6) puts futures on *different worker threads* — actual parallelism, when the work is CPU-bound *and* there are cores free.

The scanner exploits the first: thousands of `connect`s on one task, all parked at their `.await`s, their latencies overlapping. It barely uses extra cores — the win is *overlapping I/O wait*, not crunching. Internalizing "concurrency is about dealing with many things at once; parallelism is about doing many things at once" (Rob Pike) is the conceptual core of this module.

### Pill 9: `select!` — Racing Futures

`tokio::select!` polls several futures and acts on **whichever finishes first**, dropping the rest:

```rust
tokio::select! {
    res = listener.accept() => handle_new_connection(res),
    _   = shutdown.cancelled() => break,   // stop accepting; drain
}
```

The first arm to be `Ready` runs its handler; the other futures are **dropped immediately**. This is *the* async control-flow primitive: timeouts (race work against a timer), shutdown (race work against a signal — exactly the proxy's accept loop), and first-of-N (race three replicas, take the fastest). Two gotchas: (1) the dropped futures are *cancelled* (Pill 10) — make sure that's safe; (2) a future awaited in a `select!` loop usually needs `tokio::pin!` so the same future is resumed across iterations rather than restarted.

### Pill 10: Cancellation Is Just Dropping a Future

There is no "cancel" method. You cancel a future by **dropping it** — stop polling it, run its destructors, done. `select!` cancels its losers by dropping them. `tokio::time::timeout` is built entirely on this:

```rust
match tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(addr)).await {
    Ok(Ok(stream)) => { /* connected */ }
    Ok(Err(e))     => { /* refused (RST) */ }
    Err(_elapsed)  => { /* timed out: the connect future was dropped — that drop IS the cancel */ }
}
```

When the timer wins, the `connect` future is dropped mid-flight; tokio tears down the half-open socket. This is the scanner's whole engine: a `Filtered` port is one where the timer beat the connect.

**Cancellation safety** is the subtlety: a future can be dropped at *any* `.await` point. If your future has read half a message into a buffer and is then cancelled, that half-message is lost. A future is "cancellation-safe" if dropping it mid-poll leaves no broken invariant. `tokio`'s docs label which methods are safe to use in `select!`. For the scanner this is moot (connect-or-die); for anything stateful in a `select!` loop, check the label.

### Pill 11: `Semaphore` — Bounded Concurrency

Spawning one task per port for a `/1-65535` scan = 65k simultaneous `connect`s = file-descriptor exhaustion (`EMFILE`), SYN floods, and your laptop falling over. You need a *cap* on in-flight work. `tokio::sync::Semaphore` is N permits; acquiring blocks (async-ly) until one is free:

```rust
let sem = Arc::new(Semaphore::new(concurrency));     // e.g. 256 permits
for port in ports {
    let permit = sem.clone().acquire_owned().await.unwrap(); // waits if 256 in flight
    let cfg = cfg.clone();
    set.spawn(async move {
        let out = probe_port(host, port, &cfg).await;
        drop(permit);                                 // free the slot
        out
    });
}
```

The `acquire_owned().await` in the *spawn loop* is the backpressure: it parks the loop once `concurrency` probes are live, so you never build a 65k-task backlog. `acquire_owned` (vs `acquire`) yields a permit that owns its count and is `'static`, so it can move into the spawned task and free the slot exactly when that task ends. This is the single most important pattern in the scanner.

### Pill 12: Async Channels — Funnelling Results with Backpressure

`tokio::sync::mpsc` is the async multi-producer/single-consumer channel — the async sibling of Module 4's `std::mpsc`. Many tasks `tx.send(x).await`; one consumer `rx.recv().await`s them. **Bounded** gives you backpressure for free: `send` parks when the buffer is full, throttling fast producers to the consumer's speed (Module 4, Pill 10 — same lesson, async flavor):

```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanOutcome>(1024);
// producers: for each probe, tx.send(outcome).await.unwrap();
// drop every tx clone when done, then:
while let Some(outcome) = rx.recv().await { collect(outcome); }  // ends when all tx dropped
```

`recv()` returns `None` when every `Sender` is dropped — the shutdown signal, same as Module 4. Channels and `JoinSet` (Pill 13) are two ways to collect results from many tasks; you'll build both in the scanner and notice `JoinSet` is tidier when each task returns exactly one value.

### Pill 13: `JoinSet` — Structured Concurrency

`tokio::task::JoinSet` is a *dynamic, owned set of tasks* you can spawn into and drain results from as they finish — structured concurrency in one type:

```rust
let mut set = JoinSet::new();
for port in ports { set.spawn(probe(port)); }     // spawn N
while let Some(res) = set.join_next().await {      // drain as they complete
    if let Ok(outcome) = res { results.push(outcome); }
}
```

Two properties that matter: results arrive in **completion order** (sort if you need stable output), and **dropping the `JoinSet` aborts every task still in it**. That second one is the "structured" part — the child tasks can't outlive the scope that owns them, so you can't leak a runaway scan. Combined with the `Semaphore` (Pill 11), `JoinSet` is the scanner's spine: bounded spawn-in, drain-out, auto-cleanup.

### Pill 14: Graceful Shutdown

Killing a server with `process::exit` drops connections mid-write and corrupts in-flight work. *Graceful* shutdown means: stop accepting new work, let in-flight work finish, then exit. The pattern is `select!` (Pill 9) racing real work against a **shutdown signal** that fans out to every task. Build the signal on `tokio::sync::watch` (a broadcast of one latest value) — or use the production-ready `tokio_util::sync::CancellationToken`, which *is* this pattern packaged:

```rust
loop {
    tokio::select! {
        conn = listener.accept() => spawn(handle(conn?)),   // serve
        _ = shutdown.cancelled() => break,                  // stop accepting
    }
}
// then await the in-flight tasks (a JoinSet drains cleanly) before returning
```

Ctrl-C wiring: `tokio::spawn`(a task that `tokio::signal::ctrl_c().await`s, then flips the shutdown flag). The accept loop's `shutdown.cancelled()` arm wins, the loop breaks, the spawned connection tasks finish their current transfer, and `run_proxy` returns. The proxy in this module does exactly this — and the integration test asserts it returns after `trigger()`.

### Pill 15: Async I/O and the Proxy — `copy_bidirectional`, Benchmarking

Sockets implement `AsyncRead`/`AsyncWrite` — the async analogues of `Read`/`Write`. `AsyncReadExt`/`AsyncWriteExt` add `.read().await` / `.write_all().await`. For a proxy you need to pump bytes *both ways at once* until either side closes; tokio ships the whole thing:

```rust
let (to_up, to_client) = tokio::io::copy_bidirectional(&mut inbound, &mut upstream).await?;
```

It drives both directions concurrently with proper backpressure and returns the byte counts at EOF — a TCP proxy core in one line. Each proxied connection is a spawned task (Pill 6) parked on `copy_bidirectional`, so thousands coexist on a couple of threads (Pill 5's payoff).

**Benchmarking async** (the deliverable, every module from 4 on): `criterion` with the `async_tokio` feature lets `b.to_async(&rt).iter(|| async { ... })` measure futures on a real runtime. Bench the scanner: `scan_sequential` (concurrency 1) vs `scan` at 16/64/256 over a range of mostly-closed localhost ports. You'll see concurrency crush sequential — and crucially, *not* because of more cores (it's the same ~2 runtime threads) but because the per-port connect latencies **overlap**. That graph is the async thesis, measured. Where it stops improving (kernel accept queue, localhost RST speed, permit count) is the analogue of Module 4's Amdahl ceiling.

## Project: `aprobe` — an async port scanner + TCP proxy

A single CLI with two subcommands that share one async toolkit:

```text
aprobe scan  127.0.0.1:1-1024 --concurrency 512 --timeout-ms 300
aprobe proxy --listen 127.0.0.1:8080 --upstream 127.0.0.1:9000
```

- **`scan`** opens up to `--concurrency` TCP connections at once (capped by a `Semaphore`), each `connect` wrapped in a `timeout`, and classifies every port `Open` / `Closed` / `Filtered`. It's the concurrency-without-parallelism showcase: thousands of probes overlapping their I/O waits on a couple of threads.
- **`proxy`** accepts connections, dials an upstream, and shuttles bytes both ways with `copy_bidirectional`, with **graceful shutdown** on Ctrl-C — stop accepting, drain in-flight, exit clean.

Why it's a good vehicle for this module:

- **The async payoff is the *whole point*, and it's measurable.** Sequential scanning pays every port's latency in series; concurrent overlaps them. The benchmark turns "async is faster for I/O" from a slogan into a curve on *your* machine.
- **It forces every core primitive.** Bounded concurrency (`Semaphore`), result collection (`mpsc` and `JoinSet`), racing (`select!`), cancellation (`timeout` = dropping a future), and graceful shutdown (`watch`) — each is load-bearing, not decorative.
- **Portfolio-real.** A port scanner and a TCP proxy are genuine infra/security tools. "Handles 10k concurrent connections on 2 threads, with a benchmark proving the scaling" is an interview-grade sentence.
- **Hermetically testable.** Everything binds ephemeral localhost sockets (`:0`), so the tests are deterministic — no fixed ports, no external network, no flakes.

### Target spec format

`host:ports`, where `ports` is a comma-separated list of single ports and/or inclusive ranges:

```text
127.0.0.1:443
example.com:22,80,443
10.0.0.1:80-90,443,8080
```

Parsing is **pure** (no DNS, no I/O) so it unit-tests trivially; DNS resolution happens later in the scanner, where it can be `.await`ed.

### Requirements

1. **CLI**: `aprobe scan <host:ports> [--concurrency N] [--timeout-ms T]` and `aprobe proxy --listen <addr> --upstream <addr>`. Hand-rolled arg parsing (no clap — keep async the star). Defaults: concurrency 256, timeout 500 ms.
2. **Target parser** (`target.rs`): `parse_target(&str) -> Result<Target, TargetError>`. Expands ranges, de-dups, sorts; rejects empty/backwards ranges and non-numeric ports. Pure.
3. **Single probe** (`scanner.rs`): `probe_port` = `connect` wrapped in `timeout`, classified `Open` (`Ok(Ok)`) / `Closed` (`Ok(Err)`) / `Filtered` (`Err(elapsed)`). Never errors — a failed connect is *data*.
4. **Bounded concurrent scan** (`scanner.rs`): `scan` uses `Arc<Semaphore>` + `JoinSet`; the `acquire_owned().await` in the spawn loop is the backpressure. Sort outcomes by port before returning.
5. **Sequential baseline** (`scanner.rs`): `scan_sequential` probes strictly one at a time — the speedup denominator *and* a correctness oracle (must find the same open ports as `scan`).
6. **Graceful shutdown** (`shutdown.rs`): a `watch`-based `Shutdown` handle — `trigger`, `is_triggered`, `cancelled().await`. Clonable, fans one signal out to all tasks.
7. **TCP proxy** (`proxy.rs`): `run_proxy` = a `select!` accept loop racing `accept()` against `shutdown.cancelled()`; spawn one task per connection; `handle_conn` dials upstream and `copy_bidirectional`s. Drain in-flight on shutdown.
8. **`criterion` benchmark** (`benches/scan.rs`): `scan_sequential` vs `scan` at concurrency 16/64/256 over a mostly-closed localhost range, on a tokio runtime via `to_async`. Required deliverable.
9. **Tests** (`tests/integration.rs`): parser expansion; `scan` reports a live port `Open` and a dead one `Closed`; an unreachable host times out `Filtered`; the proxy forwards bytes end-to-end and *returns* after `trigger()` (graceful drain). All on ephemeral ports.

### Starter files

- `Cargo.toml` — `tokio` with an explicit, minimal feature list; `criterion` (with `async_tokio`) dev-dep; `[[bin]]` and `[[bench]] harness = false` wired; full `[package]` metadata.
- `src/lib.rs` — module declarations + re-exports.
- `src/target.rs` — `Target`, `PortState`, `ScanOutcome`, `TargetError` (given); `parse_target` (stubbed) + its unit tests (given).
- `src/scanner.rs` — `ScanConfig` (given); `probe_port`, `scan`, `scan_sequential` (stubbed) with the exact recipe in the doc comments.
- `src/shutdown.rs` — `Shutdown` skeleton on `watch` (stubbed): `new`, `trigger`, `is_triggered`, `cancelled`.
- `src/proxy.rs` — `Transferred` (given); `run_proxy`, `handle_conn` (stubbed) with the `select!`-loop recipe.
- `src/bin/aprobe.rs` — CLI: `#[tokio::main]`, subcommand dispatch, arg parsing, Ctrl-C → shutdown wiring, report printing. **Fully written** — read it as the worked CLI shell.
- `examples/echo_server.rs` — a fully-written async echo server to scan and proxy against (your local test target).
- `benches/scan.rs` — criterion harness, sequential vs concurrent, on a tokio runtime.
- `tests/integration.rs` — parser, scan-open/closed/filtered, and proxy-forwards-then-drains, all on ephemeral sockets.

### Your task

1. **Parser (`target.rs`)**: implement `parse_target` — `rsplit_once(':')`, then per comma token `split_once('-')` for ranges else a single parse; validate `start <= end`; `sort_unstable` + `dedup`. Make the unit tests green first.
2. **Probe (`scanner.rs`)**: implement `probe_port` — `timeout(cfg.timeout, TcpStream::connect((host, port)))`, match the nested `Result` into `Open`/`Closed`/`Filtered`, record `rtt`.
3. **Sequential (`scanner.rs`)**: `scan_sequential` — the dead-simple `for port in &target.ports` loop. Your oracle and denominator.
4. **Bounded scan (`scanner.rs`)**: `scan` — `Arc<Semaphore>`, `acquire_owned().await` in the spawn loop, `JoinSet::spawn` the probe (drop the permit at task end), `join_next().await` to drain, sort by port.
5. **Shutdown (`shutdown.rs`)**: build `Shutdown` on `watch::channel(false)` — `trigger` sends `true`, `is_triggered` borrows, `cancelled` loops `changed().await` until the flag is `true`.
6. **Proxy (`proxy.rs`)**: `handle_conn` (connect upstream + `copy_bidirectional`), then `run_proxy` (the `select!` accept loop, spawn-per-connection, drain on shutdown).
7. **CLI**: already written — once the library compiles, `cargo run -- scan 127.0.0.1:1-100` should work.
8. **Benchmark (`benches/scan.rs`)**: already written — `cargo bench` once `scan`/`scan_sequential` exist; read the scaling curve.
9. **Tests (`tests/integration.rs`)**: already written — `cargo test` drives every public function. Green = done.

### Hints

<details>
<summary>Hint for step 2 (classifying the connect)</summary>

`tokio::time::timeout` returns `Result<T, Elapsed>` where `T` is the *inner* future's output — here `io::Result<TcpStream>`. So you match two layers:

```rust
let started = std::time::Instant::now();
let state = match tokio::time::timeout(cfg.timeout, TcpStream::connect((host, port))).await {
    Ok(Ok(_stream)) => PortState::Open,     // connected; drop the stream
    Ok(Err(_))      => PortState::Closed,    // RST / refused
    Err(_)          => PortState::Filtered,  // timer won; connect future dropped
};
ScanOutcome { port, state, rtt: started.elapsed() }
```

`TcpStream::connect((host, port))` takes anything `ToSocketAddrs` — a `(&str, u16)` resolves the host for you. Dropping `_stream` closes the probe socket immediately; a real scanner might banner-grab first (stretch goal).
</details>

<details>
<summary>Hint for step 4 (the Semaphore + JoinSet spawn loop)</summary>

The permit must move *into* the task so the slot frees exactly when the probe ends — use `acquire_owned` (gives a `'static` `OwnedSemaphorePermit`), not `acquire`:

```rust
let sem = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
let mut set = JoinSet::new();
for &port in &target.ports {
    let permit = sem.clone().acquire_owned().await.unwrap(); // backpressure lives here
    let host = target.host.clone();
    let cfg = cfg.clone();
    set.spawn(async move {
        let out = probe_port(&host, port, &cfg).await;
        drop(permit);    // explicit; it'd drop at task end anyway
        out
    });
}
let mut outcomes = Vec::with_capacity(target.ports.len());
while let Some(res) = set.join_next().await {
    if let Ok(o) = res { outcomes.push(o); }  // res is Result<ScanOutcome, JoinError>
}
outcomes.sort_by_key(|o| o.port);            // completion order is nondeterministic
outcomes
```

Why `acquire_owned` in the *loop* and not inside the task: acquiring before spawn throttles the **spawn rate**, so you never queue 65k tasks. Acquiring inside the task would spawn them all first (the backlog you're trying to avoid).
</details>

<details>
<summary>Hint for step 5 (Shutdown on watch)</summary>

`watch::channel(false)` gives `(Sender, Receiver)`; the receiver always sees the latest value and `changed().await` resolves on each new send. `Sender` isn't `Clone`, so wrap it in `Arc` to keep `Shutdown` clonable:

```rust
pub fn new() -> Self {
    let (tx, rx) = watch::channel(false);
    Shutdown { _tx: Arc::new(tx), _rx: rx }
}
pub fn trigger(&self) { let _ = self._tx.send(true); }
pub fn is_triggered(&self) -> bool { *self._rx.borrow() }
pub async fn cancelled(&self) {
    let mut rx = self._rx.clone();
    if *rx.borrow() { return; }
    while rx.changed().await.is_ok() {
        if *rx.borrow() { return; }
    }
}
```

Clone the receiver inside `cancelled` so a shared `&Shutdown` works across many tasks without `&mut`.
</details>

<details>
<summary>Hint for step 6 (the graceful accept loop)</summary>

Spawn the connection handler — **don't** `.await` it in the loop, or you serialize connections (one at a time, defeating the point). Collect handles in a `JoinSet` so you can drain them after the loop breaks:

```rust
let mut conns = JoinSet::new();
loop {
    tokio::select! {
        accepted = listener.accept() => {
            let (inbound, _peer) = accepted?;
            let up = upstream.to_string();
            conns.spawn(async move { let _ = handle_conn(inbound, &up).await; });
        }
        _ = shutdown.cancelled() => break,   // stop accepting
    }
}
while conns.join_next().await.is_some() {}    // drain in-flight, then return
Ok(())
```

`handle_conn` is the easy half: `let mut up = TcpStream::connect(upstream).await?; let (a, b) = copy_bidirectional(&mut inbound, &mut up).await?; Ok(Transferred { client_to_upstream: a, upstream_to_client: b })`.
</details>

<details>
<summary>Hint for the benchmark (async criterion)</summary>

`b.to_async(&rt)` needs the runtime to outlive the closure, so build it once at the top of `bench`. `iter` takes a closure returning a future:

```rust
let rt = tokio::runtime::Runtime::new().unwrap();
group.bench_with_input(BenchmarkId::new("concurrent", n), &n, |b, &n| {
    let cfg = ScanConfig { concurrency: n, timeout };
    b.to_async(&rt).iter(|| async { scan(&target, &cfg).await });
});
```

`criterion = { version = "0.5", features = ["async_tokio"] }` is what unlocks `to_async`. Keep `sample_size` modest — each sample does hundreds of real connects.
</details>

## Stretch goals

- **Resolve DNS once.** `probe_port` currently re-resolves the host every call. Resolve with `tokio::net::lookup_host` up front into `Vec<SocketAddr>` and probe addresses directly — measurable on a big scan against a hostname.
- **Banner grab.** On `Open`, `read` with a short timeout and capture the first line (SSH/HTTP banners). Now `probe_port` is stateful and you must think about cancellation safety (Pill 10).
- **`CancellationToken`.** Swap the hand-rolled `watch` `Shutdown` for `tokio_util::sync::CancellationToken` and its `cancelled()`/child-token tree. Notice it's the same idea, hardened.
- **Connection metrics in the proxy.** Track live connection count and total bytes with an `Arc<AtomicU64>`/`AtomicUsize`; log a line per closed connection and a summary on shutdown.
- **`select!` per-connection idle timeout.** In `handle_conn`, race `copy_bidirectional` against an idle timer so a hung peer can't pin a task forever.
- **UDP scan / SYN scan.** A `connect`-scan is the easy mode. A half-open SYN scan needs raw sockets (and root) — a deep rabbit hole, but the real `nmap` core.

## Key questions

- A `Future` does nothing until polled. Walk the chain from `#[tokio::main]` to a `TcpStream::connect` future actually being driven — who calls `poll`, and what wakes it when the socket connects?
- Why must a future be `Pin`ned before it's polled to completion? What specifically in the generated state machine breaks if it moves?
- Your scan of `1-65535` with no `Semaphore` dies with "too many open files." Explain mechanically what the semaphore changes about *when* tasks are created, not just how many run.
- `timeout` cancels a `connect` by dropping its future. For the scanner that's harmless. Give a future where being dropped mid-`.await` corrupts state, and how you'd make it cancellation-safe.
- You benchmark concurrency 256 at ~50× the sequential time on a range of filtered ports, but only ~3× on a range of instantly-closed ports. Why does the *kind* of port change the speedup so much?
- Concurrency 256 barely uses more CPU than concurrency 1, yet finishes far faster. Reconcile that with "more work in less time" — where did the time go, if not to extra cores?
- Graceful shutdown drains in-flight connections; a `process::exit` doesn't. Name a concrete corruption an abrupt exit causes that the drain prevents.

## Resources

- [The Tokio Tutorial](https://tokio.rs/tokio/tutorial) — the canonical starting point; do the "Spawning" and "Select" chapters
- [`std::future::Future` docs](https://doc.rust-lang.org/std/future/trait.Future.html) — the trait, in the source language
- [Asynchronous Programming in Rust (the async book)](https://rust-lang.github.io/async-book/) — `Pin`, executors, and the desugaring, in depth
- [Jon Gjengset — *The What and How of Futures and async/await*](https://www.youtube.com/watch?v=9_3krAQtD2k) — builds an executor from scratch; the single best way to demystify `poll`/`Waker`
- [`tokio::select!` docs](https://docs.rs/tokio/latest/tokio/macro.select.html) and the [cancellation-safety notes](https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety)
- [`tokio::sync::Semaphore`](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html) and [`task::JoinSet`](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)
- [`copy_bidirectional`](https://docs.rs/tokio/latest/tokio/io/fn.copy_bidirectional.html) — the proxy core
- [Tokio's graceful-shutdown guide](https://tokio.rs/tokio/topics/shutdown) — the `watch`/`CancellationToken` pattern, official version
- [Without Boats — *Why async Rust?*](https://without.boats/blog/why-async-rust/) — the design rationale, once the mechanics click
