# Performance Engineering in Rust — in 5-Minute Pills

## Goal

Take one real, brutally simple workload — the **One Billion Row Challenge** (1BRC): read a ~13 GB text file of `station;temperature` lines and print the min, mean, and max temperature per station — and drive it from a naive baseline to **10×+ faster**, *proving every win with a benchmark*. You'll start with the obvious Rust (`BufReader`, `lines()`, `String::split`, `f64::parse`, `HashMap<String, _>`), measure it with `criterion`, then find out *where the time actually goes* with a flamegraph instead of guessing. From there you'll strip the program down to the metal: `mmap` the file, parse out of `&[u8]` with zero allocation, parse the temperature as a fixed-point integer instead of a float, replace SipHash with a fast hasher, find delimiters with SIMD (`memchr`), split the file into per-core chunks on newline boundaries and run one thread per core, and confirm with an allocation profiler that the hot loop allocates *nothing*. By the end you can do the thing that actually pays: take a number, make it smaller, and prove you did.

## Time estimate

~1–2 days (14 pills × 5 min + project)

## What you'll learn

- **Benchmarking that isn't a lie** — `criterion`, warm-up, statistical noise, `black_box`, and why `Instant::now()` around a loop usually measures the wrong thing
- **Profile before you optimize** — flamegraphs and `perf`/`samply`; the discipline of *finding* the hot spot instead of guessing, and why your guess is usually wrong
- **The memory hierarchy as the real cost model** — L1/L2/L3/RAM latencies, cache lines, and why "big-O" stops predicting wall-clock once you care about constants
- **Zero-copy parsing** — working in `&[u8]` instead of `String`, borrowing keys straight out of the mmap, and getting allocation out of the hot path entirely
- **Integer-domain tricks** — parsing the temperature as fixed-point tenths (`i32`) instead of an IEEE-754 float, and why that one change is worth a lot
- **Hashing is a choice** — std's DoS-resistant SipHash vs. a fast non-cryptographic hasher (FxHash), and how to avoid the double lookup with the entry API
- **Branch prediction** — what a mispredict costs (~15+ cycles), how to spot a data-dependent branch in the hot loop, and how to make it predictable or remove it
- **SIMD without intrinsics** — letting `memchr` vectorize delimiter scanning, plus when hand-written `std::simd` is worth it
- **Data parallelism done by hand** — splitting an mmap on newline boundaries, `thread::scope`, one worker per core, and a cheap merge — no `rayon`
- **Allocation profiling** — `dhat` to count allocations and prove the steady-state hot loop is allocation-free

## Concepts

### Pill 1: Performance Is Measured, Never Guessed

This module has exactly one law: **you may not claim a speedup you did not measure.** Performance intuition — yours, mine, a senior engineer's — is wrong often enough that acting on it without a benchmark is how codebases accumulate "optimizations" that do nothing or make things slower. The job is empirical: establish a baseline number, change one thing, measure again, keep the change only if the number moved. Everything below is in service of that loop.

The naive measurement — wrap the code in `Instant::now()` / `elapsed()` and print it — is a trap for small, fast functions. It measures one noisy sample, includes cold caches and CPU frequency ramp-up, and the optimizer may delete code whose result you never use. For the hot inner functions (parse a temperature, hash a key) you want a real harness. For the whole-file run, wall-clock `time ./brc measurements.txt` is exactly right — different tools for different scales, and knowing which is which is half of Pill 1.

### Pill 2: The Workload — One Billion Rows

The challenge ([1brc](https://github.com/gunnarmorling/1brc), originally a Java contest) is deliberately almost too simple to have anywhere to hide. The input is a text file, one measurement per line:

```
Hamburg;12.0
Bulawayo;8.9
Palembang;38.8
St. John's;15.2
Hamburg;-3.4
```

A station name (UTF-8, up to 100 bytes, may contain spaces and non-ASCII), a `;`, and a temperature in the range `-99.9..=99.9` with **exactly one decimal place**. There are ~400 distinct stations but a *billion* lines (~13 GB). The output is every station, sorted alphabetically, with min/mean/max rounded to one decimal:

```
{Abha=-23.0/18.0/59.2, Abidjan=-16.2/26.0/67.3, ...}
```

That's it. No parsing ambiguity, no I/O cleverness required by the spec, no concurrency required by the spec. Which is exactly why it's the perfect performance lab: the *only* variable is how well you execute. A naive correct solution is ~5 lines of `for` loop. The fast solutions are 100× faster doing identical work. Everything separating them is this module.

### Pill 3: Establish the Baseline

Write the obvious thing first, and make it *correct*, because every later version is checked against it:

```rust
use std::collections::HashMap;
use std::io::{BufRead, BufReader};

let file = std::fs::File::open(path)?;
let mut map: HashMap<String, (f64, f64, f64, u64)> = HashMap::new(); // min, max, sum, count
for line in BufReader::new(file).lines() {
    let line = line?;
    let (name, temp) = line.split_once(';').unwrap();
    let temp: f64 = temp.parse().unwrap();
    let e = map.entry(name.to_string()).or_insert((f64::MAX, f64::MIN, 0.0, 0));
    e.0 = e.0.min(temp);
    e.1 = e.1.max(temp);
    e.2 += temp;
    e.3 += 1;
}
```

This is fine. It is also slow, and now you can say *how* slow with a number. Every line here is a future optimization target: `.lines()` allocates a `String` per line, `name.to_string()` allocates a `String` per row (a billion allocations), `f64::parse` is a general-purpose float parser doing far more than this format needs, `HashMap`'s default hasher is SipHash (cryptographic, slow), and the whole thing is single-threaded. Resist fixing any of it yet. **Measure first.**

### Pill 4: Profile Before You Optimize — the Flamegraph

You have a baseline. The instinct now is to optimize the thing you *think* is slow. Don't — *look*. A **flamegraph** is a sampled call-stack profile: the x-axis is fraction of total time (wider = more time), the y-axis is stack depth. You read it by scanning for wide boxes; those are where the CPU actually is.

```bash
cargo install flamegraph         # wraps perf (Linux) / dtrace (macOS)
cargo flamegraph --bin brc -- measurements.txt
# opens flamegraph.svg — click to zoom
```

On the baseline you'll likely find time split across float parsing, hashing, the per-line `String` allocation, and `memmove`/free from dropping those strings — and the *proportions* will surprise you. That surprise is the entire point of Pill 4: the bottleneck is an empirical fact about *this* code on *this* CPU, not something you reason out from the source. On macOS, [`samply`](https://github.com/mstange/samply) (`samply record ./target/release/brc measurements.txt`) gives a great browser UI with no `perf`. Optimizing without having looked at a profile is the single most common waste of effort in this field.

### Pill 5: Getting the Bytes In — `read`, `BufReader`, `mmap`

The file is 13 GB; how you get it into the process matters. Three levels:

- **`read()` line by line** (naive): a syscall per chunk, plus `BufReader`'s copy into your buffer, plus a `String` per line. Lots of copying.
- **Read the whole file into one `Vec<u8>`**: one big allocation, one set of `read` syscalls, then parse the buffer in place. Simple and already much faster — no per-line allocation.
- **`mmap`** (memory-map): ask the kernel to map the file's pages into your address space. No `read` syscall in the loop, no copy into a userspace buffer — you get a `&[u8]` over the file and the kernel faults pages in from the page cache as you touch them. For a read-only sequential-ish scan this is the standard tool.

```rust
let file = std::fs::File::open(path)?;
let mmap = unsafe { memmap2::Mmap::map(&file)? }; // unsafe: the file must not be mutated underneath us
let data: &[u8] = &mmap;                          // the whole file as a byte slice
```

The `unsafe` is an honest contract (Module 7's lesson): a memory-mapped slice aliases the file, so if another process truncates or rewrites it while you hold the map, you have undefined behavior. For this workload (a file you generated, read once) that's a non-issue — but you state it, you don't hide it.

### Pill 6: Zero-Copy Parsing — `&[u8]`, Not `String`

The baseline allocates a `String` for the key on *every* row — a billion heap allocations, a billion frees, and a billion hashes of freshly-copied bytes. But the bytes are *already in memory*, sitting in the mmap. The key insight of zero-copy parsing: **the station name is a `&[u8]` slice into the file; you never need to copy it to look it up.**

```rust
// `line` is a &[u8] into the mmap. Find the ';' and split — no allocation.
let sep = memchr::memchr(b';', line).unwrap();
let name: &[u8] = &line[..sep];      // borrows the file, owns nothing
let temp: &[u8] = &line[sep + 1..];
```

Your map becomes `HashMap<&[u8], Stats>` whose keys borrow the mmap (so the map can't outlive `data` — the borrow checker enforces exactly the right thing). On a *miss* you do copy the name once to insert an owned key — but misses happen ~400 times total, not a billion. Work in raw bytes, drop down to `str`/`String` only at the very end when you format output. This single change — kill the per-row allocation — is usually the biggest baseline win.

### Pill 7: Parse the Temperature as an Integer

`f64::parse` is a marvel of correctness: it handles `1.5e-10`, `inf`, `NaN`, rounding modes, arbitrary precision. You need *none* of that. The format is rigid: an optional `-`, one or two digits, a `.`, exactly one digit. The value is always an integer number of *tenths*. So parse it as one:

```rust
// "-12.3" -> -123 ;  "4.5" -> 45   (units of 0.1°C, as i32)
fn parse_temp(b: &[u8]) -> i32 {
    let (neg, b) = if b[0] == b'-' { (true, &b[1..]) } else { (false, b) };
    let v = match b {
        [d1, b'.', d2]      => (d1 - b'0') as i32 * 10 + (d2 - b'0') as i32,
        [d1, d2, b'.', d3]  => (d1 - b'0') as i32 * 100 + (d2 - b'0') as i32 * 10 + (d3 - b'0') as i32,
        _ => unreachable!("malformed temperature"),
    };
    if neg { -v } else { v }
}
```

No floating point anywhere in the hot loop. `min`/`max`/`sum` are now `i32`/`i64` integer ops (faster, exact — no float-accumulation rounding drift over a billion adds), and you only divide once per station at the end to get the mean, converting to `f64` purely for display. Integer-domain reformulation is one of the highest-leverage moves in performance work: it shrinks the per-row cost *and* removes a class of correctness bugs.

### Pill 8: Hashing Is a Choice You're Making by Default

`std::collections::HashMap` defaults to **SipHash 1-3** — a keyed, DoS-resistant hash chosen so that a web service can't be made to degrade by an attacker sending colliding keys. That safety costs cycles, and here it buys you nothing: your keys are ~400 station names from a file you control. Swap in a fast non-cryptographic hasher:

```rust
// FxHash — the hasher rustc itself uses internally. A multiply + xor per word.
use rustc_hash::FxHashMap;          // or hand-roll it (see hash.rs / Pill 8 hint)
let mut map: FxHashMap<&[u8], Stats> = FxHashMap::default();
```

FxHash is a couple of arithmetic ops per 8 bytes versus SipHash's many rounds. For short keys hashed a billion times, that's a large fraction of total runtime reclaimed. The general lesson: the standard library makes the *safe* default choice, and "safe default" is frequently not "fast." Knowing *why* the default exists (so you know when it's safe to abandon it) is the senior move — don't cargo-cult `FxHashMap` into an internet-facing service.

### Pill 9: One Lookup, Not Two — the Entry API

A subtle hot-loop tax: "check if the key exists, then update it" hashes and probes the table **twice**.

```rust
if let Some(s) = map.get_mut(name) { s.record(t); }   // lookup #1
else { map.insert(name.to_vec(), Stats::from(t)); }   // lookup #2 on the miss
```

The `entry` API does it in one probe — and on the billion-row hot path, where it's almost always a *hit*, that halves the table work:

```rust
map.entry(name).or_insert_with(Stats::empty).record(t);   // one hash, one probe
```

Watch the allocation, though: `entry` on a `&[u8]`-keyed map needs an *owned* key only when inserting. The clean version uses `raw_entry` or a custom table to borrow on lookup and clone only on miss. For 400 misses it barely matters — but recognizing the double-lookup pattern, and that the *hit* path is what you're optimizing, is the Pill 9 skill.

### Pill 10: Branch Prediction — the Invisible Cost

A modern CPU is a deep pipeline speculatively executing ~15–20 instructions ahead. At every `if` it *predicts* which way you'll go and runs ahead on that guess. Predict right (and the predictor is ~95%+ on regular patterns): free. Predict wrong: the pipeline is flushed and refilled — **~15+ cycles wasted**, every time. A data-dependent branch in a billion-iteration loop, mispredicting even 10% of the time, is a real chunk of your runtime that *never shows up as a function in the profile* — it's smeared across the loop.

In this workload the branchy spots are: the `-` sign check, the 2-vs-3-digit temperature shape, and the line-length variation. Techniques: make the common case the fall-through, replace a branch with arithmetic (`let neg = (b[0] == b'-') as i32; ... v * (1 - 2*neg)` style), or use branchless `min`/`max` (which compile to `cmov` / conditional-select, no branch). Don't do this blind — `perf stat -e branch-misses ./brc ...` tells you whether you even have a misprediction problem before you contort code to fix one you don't.

### Pill 11: SIMD — One Instruction, Many Bytes

**SIMD** (Single Instruction, Multiple Data) processes 16/32/64 bytes per instruction with vector registers. The biggest SIMD win here needs *zero* intrinsics: finding the `;` and `\n` delimiters. `memchr` is a hyper-optimized, runtime-CPU-feature-detecting SIMD byte search — use it and you've vectorized your scanning for free:

```rust
// iterate newline-delimited lines over the whole buffer, SIMD under the hood
for line in memchr::memchr_iter(b'\n', data) { /* ... */ }
// or scan ';' and '\n' together with memchr2
```

When you want to go further, Rust's portable `std::simd` (nightly) or `std::arch` intrinsics let you, e.g., load 16 bytes, compare-against-`;` to get a mask, and `trailing_zeros` the mask to find the separator — parsing several lanes at once. That's where the contest's top solutions live, and it's a real cliff in complexity for the last 2×. Reach for `memchr` first; hand-rolled SIMD only when the flamegraph says scanning/parsing still dominates *after* everything else.

### Pill 12: Parallelism — Split the File, One Thread per Core

Everything so far made *one core* faster. Now use all of them. The data is embarrassingly parallel: partition the byte range into one chunk per core, aggregate each chunk into its *own* map on its own thread, then merge the ~N maps at the end. The only subtlety: chunk boundaries must land on `\n` so you never split a line. Pick `data.len()/n` cut points, then walk each forward to the next newline.

```rust
std::thread::scope(|s| {                       // scoped threads can borrow `data` — no Arc, no 'static
    let handles: Vec<_> = chunks.into_iter()
        .map(|chunk| s.spawn(move || aggregate_chunk(chunk)))   // each returns its own map
        .collect();
    for h in handles { merge_into(&mut global, h.join().unwrap()); }
});
```

`thread::scope` (Module 4) is the right tool: it guarantees the threads finish before `data` is dropped, so each worker can hold `&[u8]` slices into the mmap with no `Arc`/`clone`. The merge is cheap — ~400 stations × N threads. Expect near-linear scaling because the work is CPU-bound and shares no mutable state until the merge. This is usually the single largest multiplier in the whole module.

### Pill 13: Allocation Profiling — Prove the Hot Loop Is Quiet

You *believe* the optimized hot loop allocates nothing per row. Belief isn't measurement (Pill 1). `dhat` is a heap profiler that counts every allocation:

```rust
// examples/alloc_profile.rs — run: cargo run --release --example alloc_profile
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
fn main() {
    let _profiler = dhat::Profiler::new_heap();
    // ... run the aggregation over a sample file ...
}   // on drop, prints total blocks/bytes allocated
```

The target: total allocations should be ~`O(stations × threads)` — the maps and the owned keys on misses — and **flat as the row count grows**. If allocations scale with *rows*, you have a hidden `to_vec`/`format!`/`collect` in the hot path; dhat will point at the call site. "Allocations are constant in the input size" is a claim you can now *prove*, which is worth more than "I'm pretty sure it's zero-copy."

### Pill 14: Put It Together — and Show the Curve

The deliverable isn't just a fast program; it's the *story* of making it fast, told in numbers. Keep every version (or git-tag them) and record the wall-clock of each on the same machine and file:

| Version | Change | Time | Speedup |
|---|---|---|---|
| v0 | naive `BufReader` + `String` + `f64` + SipHash | 100% | 1× |
| v1 | mmap + `&[u8]` zero-copy keys | … | … |
| v2 | + integer temperature parse | … | … |
| v3 | + FxHash + entry API | … | … |
| v4 | + `memchr` line scanning | … | … |
| v5 | + parallel (N cores) | … | … |

That table *is* the portfolio artifact — it demonstrates the loop (measure → change → measure) that the job is actually about. A single end number ("it's fast!") proves nothing; the curve proves you can do performance engineering. Anyone can copy a fast 1BRC solution; the table shows you understand *why* each step worked.

## Project: `brc` — a 1BRC solver, baseline → 10×+

Build a command-line program that solves the One Billion Row Challenge, and — the actual deliverable — the **benchmark table and flamegraphs** that prove you took it from a naive baseline to 10× or more. You'll also write the data generator so you can produce input files of any size.

### Requirements

1. A `gen` binary that writes a valid `measurements.txt` of N rows (deterministic, seeded) — **given** for you.
2. A `brc` binary that reads a measurements file and prints the exact 1BRC output format, sorted, rounded to one decimal.
3. A correct, simple **baseline** path (`run_sequential`) and an **optimized parallel** path (`run_parallel`); the CLI picks based on a flag or thread count.
4. Zero-copy parsing: keys borrow the mmap; the temperature is parsed as fixed-point `i32` tenths; no per-row heap allocation.
5. A `criterion` benchmark (**given**) over a fixed sample file, plus a flamegraph and a wall-clock speedup table in your writeup.
6. An allocation-profile example (**given**) showing per-row allocations stay flat as N grows.
7. Output verified against the baseline (an integration test is **given**).

### Starter files

- `Cargo.toml` — deps wired (`memmap2`, `memchr`; dev: `criterion`, `dhat`). **Given.**
- `src/lib.rs` — module map and crate docs. **Given.**
- `src/bin/gen.rs` — the seeded data generator. **Given** — read it, then use it to make test data.
- `src/parse.rs` — `parse_temp` and line splitting. **TODO.**
- `src/aggregate.rs` — the `Stats` accumulator and output formatting. **TODO.**
- `src/hash.rs` — a fast FxHash-style hasher + `FastMap` alias. **TODO.**
- `src/io.rs` — `map_file` (given) and `split_chunks` on newline boundaries. **TODO** (`split_chunks`).
- `src/runner.rs` — `run_sequential` and `run_parallel`. **TODO.**
- `src/bin/brc.rs` — CLI: parse args, time the run, print results. **TODO.**
- `benches/aggregate.rs` — criterion bench of parse + aggregate. **Given.**
- `examples/alloc_profile.rs` — dhat allocation profile. **Given.**
- `tests/integration.rs` — correctness + sequential-vs-parallel agreement. **Given.**

### Your task

Generate a small data file first so every step is testable:

```bash
cargo run --release --bin gen -- 1000000 measurements.txt   # 1M rows to start
```

Then, in order:

1. **`parse.rs`** — implement `parse_temp` (fixed-point `i32` tenths) and `split_line` (`&[u8]` name + temp via `memchr`). Get the unit tests green.
2. **`aggregate.rs`** — implement `Stats::record`, `Stats::merge`, `Stats::mean`, and `format_results` (the `{Name=min/mean/max, ...}` line, sorted, one decimal).
3. **`hash.rs`** — implement the FxHash-style `Hasher` and the `FastMap` type alias.
4. **`runner.rs` (sequential)** — `run_sequential`: mmap → iterate lines (`memchr`) → split → parse → `entry().record()` into a `FastMap<&[u8], Stats>` → return a sorted result.
5. **`bin/brc.rs`** — wire the CLI: open the file, run, print, and `eprintln!` the elapsed time. You now have a correct, measurable solver.
6. **Baseline the slow way too** — quickly add a naive `BufReader`+`String`+`f64`+`HashMap` variant (a function or a feature flag) so you have a *real* v0 number to divide by.
7. **`io.rs`** — implement `split_chunks`: N ranges cut at `\n` boundaries.
8. **`runner.rs` (parallel)** — `run_parallel`: `thread::scope`, one worker per chunk, each builds its own map, then `merge`. Confirm it matches `run_sequential` (the test checks this).
9. **Measure the curve** — `cargo bench`, `cargo flamegraph`, and `time ./target/release/brc measurements.txt` at each stage. Fill in the speedup table in your writeup. Run the allocation profile and confirm it's flat.

### Hints

<details>
<summary>Hint for step 1 (parse_temp & split_line)</summary>

`parse_temp` is in Pill 7 nearly verbatim — match on the byte-slice shape after stripping an optional leading `-`. For `split_line`, `memchr::memchr(b';', line)` gives the separator index; everything before is the name, everything after (skipping the `;`) is the temperature. Don't `unwrap` blindly in the final version, but for the challenge's well-formed input it's acceptable — just be honest that you're trusting the format.

```rust
pub fn split_line(line: &[u8]) -> (&[u8], &[u8]) {
    let i = memchr::memchr(b';', line).expect("every line has a ';'");
    (&line[..i], &line[i + 1..])
}
```
</details>

<details>
<summary>Hint for step 2 (Stats)</summary>

Keep min/max as `i32` tenths, sum as `i64` (a billion values up to ±999 overflows `i32`), count as `u64`.

```rust
impl Stats {
    pub fn record(&mut self, t: i32) {
        self.min = self.min.min(t);
        self.max = self.max.max(t);
        self.sum += t as i64;
        self.count += 1;
    }
    pub fn merge(&mut self, o: &Stats) {
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
        self.sum += o.sum;
        self.count += o.count;
    }
    pub fn mean(&self) -> f64 { self.sum as f64 / 10.0 / self.count as f64 }
}
```

For formatting, min/max are `t as f64 / 10.0`; print all three with `{:.1}`. Sort by name (a `BTreeMap<&[u8], Stats>` or a `sort` on a `Vec`) and join with `, ` inside `{...}`. Watch `-0.0`: round carefully so a mean of `-0.04` doesn't print as `-0.0` if the reference says `0.0` (the official 1BRC uses round-half-up).
</details>

<details>
<summary>Hint for step 3 (FxHash)</summary>

FxHash multiplies each incoming word by a constant and rotate-xors it into the state. A minimal version:

```rust
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
#[derive(Default)]
pub struct FxHasher { hash: u64 }
impl FxHasher {
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}
impl std::hash::Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(buf));
        }
    }
    fn finish(&self) -> u64 { self.hash }
}
```

Then `type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;`. (In real code you'd pull in the `rustc-hash` crate; here you build it once to see there's no magic.)
</details>

<details>
<summary>Hint for step 4 & 8 (the aggregation loop and merge)</summary>

```rust
pub fn aggregate(data: &[u8]) -> FastMap<&[u8], Stats> {
    let mut map: FastMap<&[u8], Stats> = FastMap::default();
    let mut start = 0;
    for nl in memchr::memchr_iter(b'\n', data) {
        let line = &data[start..nl];
        start = nl + 1;
        let (name, temp) = split_line(line);
        map.entry(name).or_default().record(parse_temp(temp));
    }
    map
}
```

`run_sequential` calls `aggregate` and sorts the result. `run_parallel` runs `aggregate` per chunk in `thread::scope`, then folds each thread's map into a global one with `Stats::merge`. Because keys are `&[u8]` into `data`, every map shares the same lifetime — the merge is just `for (k, v) in chunk_map { global.entry(k).or_default().merge(&v); }`.
</details>

<details>
<summary>Hint for step 7 (split_chunks on newline boundaries)</summary>

Cut at `len/n`, then advance each cut to the next `\n` so no line is split. Return `&[u8]` sub-slices that exactly tile `data`.

```rust
pub fn split_chunks(data: &[u8], n: usize) -> Vec<&[u8]> {
    if data.is_empty() { return vec![]; }
    let mut chunks = Vec::with_capacity(n);
    let approx = data.len() / n;
    let mut start = 0;
    for _ in 0..n - 1 {
        let mut end = (start + approx).min(data.len());
        while end < data.len() && data[end] != b'\n' { end += 1; }
        if end < data.len() { end += 1; }       // include the newline
        chunks.push(&data[start..end]);
        start = end;
        if start >= data.len() { break; }
    }
    if start < data.len() { chunks.push(&data[start..]); }
    chunks
}
```
Use `std::thread::available_parallelism()` for `n`.
</details>

## Stretch goals

- **Hand-written SIMD parse (`std::simd` or `std::arch`).** Load 16 bytes, build a `;`-mask, find the separator with `trailing_zeros`; parse the temperature branchlessly from a fixed-width window. This is the top-solutions frontier and the last ~2×.
- **A custom open-addressing table.** Replace `HashMap` with a linear-probing table sized to ~400 entries, keyed by `&[u8]`, that borrows on lookup and clones only on insert — kills the entry-API owned-key allocation and the generic-map overhead.
- **Huge pages / `madvise`.** `madvise(MADV_SEQUENTIAL | MADV_WILLNEED)` on the mmap, or transparent huge pages, to cut TLB misses on the 13 GB scan. Measure whether it actually helps on your machine — often it does, sometimes it doesn't.
- **`perf stat` deep dive.** Report IPC, cache-miss rate, and branch-miss rate for v0 vs. final. Explain the numbers — *why* did IPC go up?
- **Generate and run the real billion.** `cargo run --release --bin gen -- 1000000000 measurements.txt` (~13 GB, ~3 GB if you trim stations) and report your honest wall-clock. Compare to the [1brc leaderboard](https://github.com/gunnarmorling/1brc) (Java) and the [Rust write-ups](https://github.com/gunnarmorling/1brc/discussions/categories/show-and-tell).
- **NUMA awareness.** On a multi-socket box, pin threads and partition the mmap so each socket touches local memory. The kind of thing that separates a 1.5× from a 2× on big iron.

## Key questions

- Your flamegraph on the baseline shows ~30% of time in `malloc`/`free`. Name the exact line of the baseline responsible and the change that removes it — and explain why that one change is worth more than rewriting the float parser.
- `f64::parse` is correct for every input. Give two distinct reasons parsing the temperature as an `i32` of tenths is *both* faster *and* more correct for this specific workload.
- You swapped `HashMap` for an `FxHashMap` and it got faster. State precisely what safety property you gave up, and describe an application where making that same swap would be a security bug.
- A mispredicted branch costs ~15+ cycles but never appears as its own box in a flamegraph. Explain why, and name the tool and counter that *does* reveal it.
- In `run_parallel`, each worker holds `&[u8]` slices into the mmap with no `Arc`. What exactly lets the borrow checker accept that, and what would break if you used `std::thread::spawn` instead of `thread::scope`?
- Your `split_chunks` cuts at `len/n`. Construct the bug that appears if you *don't* advance each cut to the next `\n`, and explain why it shows up as wrong *output* rather than a crash.
- Adding a second decimal of parallelism (8 → 16 threads) gave almost no speedup on an 8-core machine. Give the two most likely explanations and how you'd tell them apart.
- The allocation profile shows total allocations growing linearly with row count after all your work. Where is the leak most likely hiding, and which `dhat` output field points you at it?

## Resources

- [The One Billion Row Challenge](https://github.com/gunnarmorling/1brc) — the original (Java) contest, rules, and leaderboard; the [Show & Tell discussions](https://github.com/gunnarmorling/1brc/discussions/categories/show-and-tell) have detailed Rust write-ups
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/) (Nethercote) — the canonical, practical reference: benchmarking, profiling, allocations, hashing, the lot
- [`criterion` user guide](https://bheisler.github.io/criterion.rs/book/) — statistically rigorous benchmarking, `black_box`, regression detection
- [`cargo-flamegraph`](https://github.com/flamegraph-rs/flamegraph) and [`samply`](https://github.com/mstange/samply) — sampling profilers; flamegraphs on Linux/macOS
- [Brendan Gregg on flamegraphs](https://www.brendangregg.com/flamegraphs.html) — how to read them, from the person who invented them
- [`memchr` docs](https://docs.rs/memchr/latest/memchr/) — SIMD byte search and the iterator API you'll lean on
- [`dhat` docs](https://docs.rs/dhat/latest/dhat/) — heap allocation profiling in-process
- [`rustc-hash` (FxHash)](https://docs.rs/rustc-hash/latest/rustc_hash/) — the production version of the hasher you build in Pill 8
- [Agner Fog's microarchitecture manuals](https://www.agner.org/optimize/) — the deep reference on pipelines, branch prediction, and instruction latencies
- [`std::simd` (portable SIMD)](https://doc.rust-lang.org/std/simd/index.html) and [Rust SIMD guide](https://rust-lang.github.io/packed_simd/perf-guide/) — for the hand-written-SIMD stretch goal
- [What every programmer should know about memory](https://people.freebsd.org/~lstewart/articles/cpumemory.pdf) (Drepper) — the memory-hierarchy bible behind Pill 3's cost model
