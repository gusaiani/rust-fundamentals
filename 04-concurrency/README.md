# Concurrency & Parallelism in 5-Minute Pills

## Goal

Take a CPU-bound batch job — analyzing a multi-GB log file — and make it scale across cores correctly, then *prove* the speedup with a benchmark instead of guessing.

## Time estimate

~1 day (15 pills × 5 min + project)

## What you'll learn

- `std::thread`, `spawn`/`join`, and why closures must be `'static` (and how scoped threads escape that)
- `Send` and `Sync` — the two marker traits the borrow checker uses to make data races a compile error
- `Arc`, `Mutex`, `RwLock` — shared ownership and shared mutation, and their poisoning/contention costs
- Why `Arc<Mutex<HashMap>>` hammered by N threads is *slower* than one thread — and the map-reduce pattern that fixes it
- Channels: `std::sync::mpsc` vs `crossbeam-channel`, the pipeline pattern, and bounded queues for backpressure
- `rayon` work-stealing parallelism — when the parallel iterator is the right tool and when it lies to you
- Splitting a file across threads without corrupting lines, and why merges must be associative
- Benchmarking with `criterion`, reading a scaling curve, and hitting Amdahl's ceiling

## Concepts

### Pill 1: Threads — `spawn` and `join`

`std::thread::spawn` runs a closure on a new OS thread and returns a `JoinHandle<T>`. `.join()` blocks until that thread finishes and gives you its return value (or the panic payload):

```rust
let handle = std::thread::spawn(|| {
    expensive_work()          // runs on another OS thread
});
let result = handle.join().unwrap();   // wait for it, get the T back
```

The closure must own everything it touches — move it in with `move ||`. The closure (and its captures) must be `Send + 'static`, because the spawned thread can outlive the function that started it. That `'static` bound is the source of most beginner fights with `thread::spawn`. Pill 3 is the escape hatch.

### Pill 2: `Send` and `Sync` — the thread-safety markers

Two auto traits the compiler derives for you:

- **`Send`**: ownership of the value can be transferred *to* another thread. Almost everything is `Send`. `Rc<T>` is not (its refcount isn't atomic). Raw pointers aren't.
- **`Sync`**: `&T` can be shared *between* threads — equivalently, `T: Sync` iff `&T: Send`. `Mutex<T>` is `Sync` (that's its job); `Cell<T>`/`RefCell<T>` are not (non-atomic interior mutation).

You rarely implement these by hand. You *read* them in error messages: "`Rc<i32>` cannot be sent between threads safely" means you reached for the wrong smart pointer. Data-race freedom in Rust is exactly these two traits plus the borrow checker — there is no runtime race detector because there's no need for one.

### Pill 3: Scoped Threads — borrowing across thread boundaries

`thread::spawn` demands `'static` because it can't prove the thread dies before the borrowed data does. `std::thread::scope` *can* prove it — every thread spawned inside the scope is joined before `scope` returns, so borrowing stack data is sound:

```rust
let data = vec![1, 2, 3, 4];
let mid = data.len() / 2;
std::thread::scope(|s| {
    s.spawn(|| sum(&data[..mid]));     // borrows `data` — no Arc, no clone
    s.spawn(|| sum(&data[mid..]));
});                                     // both joined here
```

This is the workhorse for "split a slice, process the halves in parallel, no heap juggling." Stabilized in Rust 1.63. Use it before reaching for `Arc`.

### Pill 4: `Arc` — shared ownership across threads

When multiple threads each need to *own* a handle to the same heap value (and scoped borrowing doesn't fit — e.g. the threads outlive the scope, or you're moving into `spawn`), use `Arc<T>` — Atomically Reference-Counted. Clone bumps an atomic counter; the value drops when the last `Arc` does:

```rust
let shared = Arc::new(big_read_only_table());
for _ in 0..4 {
    let shared = Arc::clone(&shared);   // cheap: refcount++, no deep copy
    std::thread::spawn(move || use_table(&shared));
}
```

`Arc` gives shared *ownership*, not shared *mutation* — `Arc<T>` only hands out `&T`. To mutate, you need a lock inside it (Pill 5). `Rc` is the single-threaded version and won't compile across threads (not `Send`) — the compiler catches the mistake for free.

### Pill 5: `Mutex` and `RwLock` — shared mutation

`Arc<Mutex<T>>` is the canonical "many threads, shared mutable state." `.lock()` returns a RAII guard; the lock releases when the guard drops:

```rust
let counter = Arc::new(Mutex::new(0u64));
{
    let mut n = counter.lock().unwrap();   // blocks until acquired
    *n += 1;
}                                          // guard dropped → unlocked
```

`.lock()` returns a `Result` because of **poisoning**: if a thread panics while holding the lock, the `Mutex` is marked poisoned and subsequent `.lock()`s return `Err` — a signal that the protected data may be inconsistent. `RwLock` allows many concurrent readers *or* one writer; use it when reads vastly outnumber writes. Hold locks for the shortest possible span — the critical section is serialized, so it's pure Amdahl tax.

### Pill 6: Lock Contention — why naive sharing doesn't scale

Intuition says "8 threads updating `Arc<Mutex<HashMap>>` = 8× faster." Reality: often *slower than one thread*. Every `insert` serializes on the same lock, threads spend their time parked waking each other, and the cache line holding the lock ping-pongs between cores. You've parallelized the reading and serialized the only part that mattered.

The rule: **shared mutable state is the enemy of scaling.** The lock isn't the optimization — removing the need for it is. Which is Pill 7.

### Pill 7: Map-Reduce — per-thread accumulators, merge at the end

Give every thread its *own* local accumulator. No sharing during the hot loop. When threads finish, fold the locals into one result:

```text
thread 0:  chunk0 ─► local Stats0 ┐
thread 1:  chunk1 ─► local Stats1 ┼─► merge ─► final Stats
thread 2:  chunk2 ─► local Stats2 ┘
```

Zero lock contention in the inner loop; the only synchronization is N-1 merges at the very end. This is *the* pattern for parallel aggregation, and the project's core. It works only if the merge is associative and commutative (Pill 14).

### Pill 8: `std::sync::mpsc` — channels and the pipeline pattern

A channel is a typed queue: `Sender<T>` / `Receiver<T>`. `mpsc` = multi-producer, single-consumer. The receiver iterator ends when *all* senders are dropped — that's the shutdown signal, not a sentinel value:

```rust
let (tx, rx) = std::sync::mpsc::channel();
for line in lines { tx.send(line).unwrap(); }
drop(tx);                                  // close: no more senders
for line in rx { process(line); }          // ends when channel drained + closed
```

This enables the **pipeline**: a reader thread produces work, a pool of workers consumes it. Decouples I/O from CPU. `std::mpsc` is single-consumer though — for a worker *pool* you want Pill 9.

### Pill 9: `crossbeam-channel` — MPMC, bounded, `select!`

`crossbeam-channel` is the channel `std` should have shipped: multi-producer **multi-consumer**, faster, with `select!` over multiple channels and clean disconnect detection. Clone the receiver and hand it to N workers — they compete for items off one queue (a natural work queue):

```rust
let (tx, rx) = crossbeam_channel::bounded(1024);
let workers: Vec<_> = (0..n).map(|_| {
    let rx = rx.clone();                  // MPMC: every worker shares one queue
    std::thread::spawn(move || for job in rx { handle(job) })
}).collect();
```

`recv()`/`send()` return `Err` on disconnect instead of panicking — explicit, matchable shutdown.

### Pill 10: Backpressure — bounded channels on huge inputs

`channel()` (unbounded) on a 50 GB log is a memory bomb: the reader is faster than the parsers, the queue grows without limit, you OOM. `bounded(capacity)` makes `send` *block* when the queue is full — the fast producer is throttled to the speed of the slow consumers automatically. That blocking *is* backpressure: flow control for free. Choosing the capacity is a latency/throughput/memory knob; "a few thousand" is a fine default to start.

### Pill 11: `rayon` — work-stealing data parallelism

`rayon` turns `.iter()` into `.par_iter()` and parallelizes it across a work-stealing thread pool. Idle threads steal tasks from busy ones, so load stays balanced even with uneven work:

```rust
use rayon::prelude::*;
let total: u64 = lines.par_iter()
    .filter_map(|l| parse(l).ok())
    .map(|e| e.bytes)
    .sum();
```

For map-reduce specifically, `par_iter().fold(local_init, ...).reduce(global_init, merge)` *is* Pill 7 with the threading written for you. Rayon is the highest-leverage tool here — but it hides the mechanism, so you'll hand-roll the scoped-thread version first to understand what it's doing. Rayon also can't help when the bottleneck is I/O, not CPU (Pill 12).

### Pill 12: I/O-bound vs CPU-bound — where parallelism pays

Reading bytes off a disk/NVMe is **I/O-bound**: more threads don't make the device faster, and may make it slower (random seeks). Parsing and aggregating those bytes is **CPU-bound**: that scales with cores. So the winning shape is *read sequentially (or memory-map), parse/aggregate in parallel*. Memory-mapping (`memmap2`) hands the whole file to the OS as a `&[u8]` and lets the page cache do the I/O lazily — turning "read the file" into "slice the file," which is what makes Pill 13's chunking cheap.

### Pill 13: Chunking a File — newline-aligned byte ranges

Splitting a file into N equal byte ranges and handing one to each thread corrupts the records straddling the cuts — you'd parse half a line twice or lose it. Fix: pick the *approximate* split point, then **scan forward to the next `\n`** and cut there. Every chunk now starts at a record boundary and ends at one:

```text
|----chunk 0----\n|--------chunk 1--------\n|----chunk 2 ...
        ^ raw split landed mid-line; advance to the next \n
```

Off-by-one here is the classic parallel-file bug: a line double-counted or dropped silently inflates/deflates your stats with no error. Test it against the sequential oracle.

### Pill 14: Associative Merge — correctness under reordering

Map-reduce only works if combining partial results is **associative** (`(a⊕b)⊕c == a⊕(b⊕c)`) and **commutative** (`a⊕b == b⊕a`), because thread finish order is nondeterministic. Sums, counts, per-key `HashMap` count merges, and `max` all qualify. *Averages do not* — `avg(avg(a), avg(b)) ≠ avg(a∪b)`. Store `(sum, count)` and divide at the very end instead. Percentiles don't compose either: you must keep all samples (or a mergeable sketch — see stretch goals). Encode the merge as one `Merge` trait so every implementation reuses the same provably-correct combine.

### Pill 15: Benchmarking Speedup — `criterion`, Amdahl's law, scaling

From this module on, a project isn't done until a benchmark proves the claim. `criterion` runs statistically rigorous benchmarks (warm-up, many samples, outlier detection, regression vs. the last run):

```rust
c.bench_function("parallel_8", |b| b.iter(|| analyze_parallel(black_box(&data), 8)));
```

Benchmark the sequential baseline and each parallel variant at 1/2/4/8 threads. You will *not* see 8× at 8 threads — **Amdahl's law**: if a fraction `s` of the work is serial, max speedup is `1/(s + (1−s)/N)`. The serial parts here: reading the file, the final merge, allocator pressure. A good result is near-linear up to physical cores, then flat (hyperthreads and memory bandwidth). Measuring *where* it flattens tells you what to optimize next — that's the entire point of benchmarking over guessing.

## Project: `logcrunch` — a parallel log analyzer

A CLI that ingests a large server-log file and produces a traffic report — total requests, bytes served, status-code distribution, top-K paths and client IPs, error rate, and request-time percentiles (p50/p95/p99). The same analysis is implemented four ways — sequential baseline, hand-rolled scoped-thread map-reduce, a crossbeam channel pipeline, and a `rayon` one-liner — and a `criterion` benchmark puts numbers on which wins and by how much.

Why it's a good vehicle for this module:

- **Embarrassingly parallel, with a real catch.** The map is trivially parallel; the *merge* and the *file chunking* are where the actual concurrency reasoning lives.
- **Forces the contention lesson.** The naive `Arc<Mutex<HashMap>>` version is the obvious first instinct and is *slower*. Feeling that is the lesson.
- **Benchmark-driven.** "It's faster" is a hypothesis until `criterion` says so. You'll see Amdahl's ceiling on your own machine.
- **Portfolio-real.** Log analysis at scale is a genuine ops/infra task. A tool that crunches GB/s and shows a clean scaling curve is interview gold.

### Log format

To keep parsing out of the way (concurrency is the topic, not regex), `logcrunch` reads a fixed **space-delimited** format, one record per line, exactly six fields:

```
<ip> <status> <bytes> <request_time_ms> <method> <path>
```

Example:

```
10.0.0.5 200 1432 12.4 GET /api/users
192.168.1.9 404 512 3.1 GET /favicon.ico
10.0.0.5 500 0 88.7 POST /api/checkout
```

`path` never contains spaces in this synthetic format. (Parsing real nginx Combined Log Format with quoted fields is a stretch goal.)

### Requirements

1. **CLI**: `logcrunch <FILE> [--threads N] [--top K] [--mode seq|par|pipeline|rayon]`. Default threads = available parallelism, K = 10, mode = `par`.
2. **Parser**: `parse_line(&str) -> Result<LogEntry, ParseError>`. Malformed lines are *counted and skipped*, never panic.
3. **`Stats`** aggregate with a `Merge` trait whose `merge` is associative & commutative. Stores counts, per-key maps, and the request-time samples needed for percentiles.
4. **Sequential baseline** (`sequential.rs`) — the correctness oracle and the speedup denominator.
5. **Parallel map-reduce** (`parallel.rs`) — `std::thread::scope`, newline-aligned chunking, per-thread local `Stats`, one final merge. **No `Mutex` in the hot loop.**
6. **Channel pipeline** (`pipeline.rs`) — reader thread → **bounded** crossbeam channel → N worker threads → merged result.
7. **Rayon** (`rayon_impl.rs`) — the same analysis as a `par_iter().fold().reduce()`.
8. **`criterion` benchmark** (`benches/throughput.rs`) — baseline vs. each parallel mode at 1/2/4/8 threads on a generated multi-MB log. Required deliverable, not optional.
9. **Report**: pretty-printed totals, status distribution, top-K paths & IPs, error rate (`5xx / total`), and p50/p95/p99 request time.
10. **Tests**: parser unit tests (valid + each malformation), a `merge` associativity test, and `parallel == sequential` on a fixture (the chunking-correctness guard).

### Starter files

- `Cargo.toml` — `crossbeam-channel`, `rayon` deps; `criterion` dev-dep; `[[bench]]` wired with `harness = false`; full `[package]` metadata.
- `src/lib.rs` — module declarations + re-exports.
- `src/parser.rs` — `LogEntry`, `ParseError`, `parse_line` (stubbed).
- `src/stats.rs` — `Stats`, the `Merge` trait, `record`, `merge`, `percentile`, `Report` (stubbed).
- `src/sequential.rs` — `analyze_sequential` (stubbed).
- `src/parallel.rs` — `split_into_chunks` + `analyze_parallel` (stubbed).
- `src/pipeline.rs` — `analyze_pipeline` (stubbed).
- `src/rayon_impl.rs` — `analyze_rayon` (stubbed).
- `src/bin/logcrunch.rs` — CLI: arg parsing, dispatch on `--mode`, print the report.
- `benches/throughput.rs` — criterion harness comparing the modes.
- `examples/gen_log.rs` — synthetic-log generator so you can make a big file to bench against.
- `tests/integration.rs` — parser, associativity, and parallel-equals-sequential tests.
- `tests/fixtures/sample.log` — a tiny hand-checkable fixture.

### Your task

1. **Parser (`parser.rs`)**: define `LogEntry` and `ParseError`; implement `parse_line` with `split_whitespace`, validating field count and numeric parses.
2. **`Stats` + `Merge` (`stats.rs`)**: design the struct, implement `record(&mut self, &LogEntry)`, the `Merge` trait, `percentile`, and `into_report`.
3. **Sequential (`sequential.rs`)**: read the file line by line, `record` into one `Stats`. This is your oracle.
4. **Generator (`examples/gen_log.rs`)**: emit N random-ish valid lines so you have a real input to chunk and bench.
5. **Chunking (`parallel.rs`)**: `split_into_chunks(data: &[u8], n) -> Vec<&[u8]>` with newline-aligned boundaries. Unit-test it first.
6. **Parallel map-reduce (`parallel.rs`)**: `thread::scope`, one `Stats` per chunk, fold with `Merge`.
7. **Pipeline (`pipeline.rs`)**: bounded crossbeam channel, reader produces line batches, workers consume and locally aggregate, collect+merge their `Stats`.
8. **Rayon (`rayon_impl.rs`)**: `par_iter`/`par_bridge` + `fold` + `reduce`.
9. **CLI (`bin/logcrunch.rs`)**: parse args, dispatch on mode, print the `Report`.
10. **Benchmark (`benches/throughput.rs`)**: criterion group over modes × thread counts; eyeball the scaling curve.
11. **Tests (`tests/integration.rs`)**: parser cases, `(a.merge(b)).merge(c) == a.merge(b.merge(c))`, and parallel output equals sequential on the fixture.

### Hints

<details>
<summary>Hint for step 2 (percentiles don't merge — so what do you store?)</summary>

You can't merge two p95s into a combined p95. The simple, correct choice for this module: keep every request-time sample in a `Vec<f32>` on the `Stats`, `merge` by `extend`ing one vec with the other, and compute percentiles at the very end by sorting and indexing (`v[(p/100.0 * (len-1)) as usize]` after `sort_unstable_by`). Note the memory cost in a comment — on a 50 GB log that vec is huge. The mergeable-sketch fix (HdrHistogram / t-digest) is a stretch goal; don't reach for it yet.

</details>

<details>
<summary>Hint for step 5 (newline-aligned chunking)</summary>

Compute the raw split as `i * data.len() / n`. From each raw offset, advance while `data[end] != b'\n'` to land *just past* the next newline; that becomes both this chunk's end and the next chunk's start. The first chunk starts at 0; the last ends at `data.len()`. Return `&[u8]` sub-slices (zero-copy — no allocation, no `String`). Edge cases to test: a file with no trailing newline, a chunk that lands exactly on a `\n`, and `n` greater than the line count.

</details>

<details>
<summary>Hint for step 6 (scoped threads returning values)</summary>

`s.spawn(|| { ... ; local_stats })` returns a `ScopedJoinHandle<Stats>`. Collect the handles into a `Vec`, then after the closure `for h in handles { acc.merge(h.join().unwrap()) }`. The borrow of the chunk slices is sound because `scope` joins everything before returning — that's exactly the `'static`-escape from Pill 3. Don't wrap `Stats` in a `Mutex`; the whole point is that there's nothing shared to lock.

</details>

<details>
<summary>Hint for step 7 (pipeline shutdown)</summary>

The reader thread `send`s batches then **drops its `Sender`** (or lets it fall out of scope). Each worker loops `for batch in rx { ... }` (or `while let Ok(b) = rx.recv()`), which terminates when the channel is empty *and* all senders are gone. Send `Vec<String>` batches of ~1000 lines, not one line per message — per-message channel overhead dominates otherwise. Use `bounded`, not `unbounded` (Pill 10), or a big input will OOM.

</details>

<details>
<summary>Hint for step 8 (rayon fold vs map)</summary>

`.par_iter().fold(Stats::default, |mut acc, line| { if let Ok(e) = parse_line(line) { acc.record(&e) } acc }).reduce(Stats::default, |a, b| { let mut a = a; a.merge(b); a })`. `fold` makes per-thread-ish local accumulators (rayon picks the split), `reduce` combines them with your associative merge. Reading the file into `Vec<String>` first is fine for the bench; `par_bridge` over a `Lines` iterator avoids the allocation but parallelizes worse — try both and let the benchmark decide.

</details>

<details>
<summary>Hint for step 10 (criterion with a parameter)</summary>

Use `c.benchmark_group("analyze")` and `group.bench_with_input(BenchmarkId::new("parallel", n), &n, |b, &n| b.iter(|| analyze_parallel(black_box(&data), n)))` looping `n` over `[1,2,4,8]`. Generate the log once *outside* `b.iter` (setup must not be measured). `harness = false` in `Cargo.toml`'s `[[bench]]` so criterion's `main!` runs instead of libtest.

</details>

## Stretch goals

- **Mergeable percentiles:** swap the sample `Vec` for an HdrHistogram or a t-digest. Percentiles stay approximately correct, memory becomes O(1), and `merge` becomes histogram addition — the *right* answer for unbounded streams.
- **Memory-mapped input:** read the file with `memmap2` and chunk the `&[u8]` directly (Pill 12). Bench mmap vs `read_to_end` — the difference on a cold vs warm page cache is instructive.
- **Real Combined Log Format:** parse actual nginx/Apache logs (quoted fields, bracketed timestamps). Now the parser is the hard part — handle it without regex if you can.
- **Streaming / unbounded:** make `logcrunch` tail a growing file (`tail -f` semantics) and emit a rolling report every N seconds. Forces you to think about windowing and a long-lived pipeline.
- **`select!` shutdown:** add a Ctrl-C channel and `crossbeam::select!` it against the work channel so the pipeline drains and prints partial results on interrupt.

## Key questions

- Why does `thread::spawn` require `'static` but `thread::scope` doesn't? What does `scope` know that `spawn` can't?
- You measured the `Arc<Mutex<HashMap>>` version as slower than sequential. Mechanically, where did the time go?
- Map-reduce needs an associative *and* commutative merge. Give an aggregate that's associative but not commutative, and one that's neither — and how you'd restructure each to compose.
- Your parallel version hits 5.5× at 8 threads and won't go higher. Which serial fractions are you paying, and which one would you attack first?
- When is `rayon` the wrong choice even though the work is "parallel"?
- Why is an unbounded channel a correctness/operational hazard on large inputs, and what exactly does a bounded one trade away?

## Resources

- [The Rust Book, Ch. 16 — Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)
- [`std::thread::scope` docs](https://doc.rust-lang.org/std/thread/fn.scope.html)
- [`rayon` docs](https://docs.rs/rayon) and the [FAQ on when not to use it](https://github.com/rayon-rs/rayon/blob/main/FAQ.md)
- [`crossbeam-channel` docs](https://docs.rs/crossbeam-channel)
- [`criterion` user guide](https://bheisler.github.io/criterion.rs/book/)
- [Jon Gjengset — *Crust of Rust: Atomics and Memory Ordering*](https://www.youtube.com/watch?v=rMGWeSjctlY) (deeper than this module needs, worth it later)
- [Amdahl's law](https://en.wikipedia.org/wiki/Amdahl%27s_law) — the math behind the scaling ceiling you'll measure
