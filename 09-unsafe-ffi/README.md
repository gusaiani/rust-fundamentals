# cbloom

A **Bloom filter with a C ABI** — a safe Rust core wrapped in a hand-written `extern "C"` boundary, shipped as a C shared/static library and consumed from C and Python. This crate is a **build-it-yourself FFI lab**: a scaffold with the safe data structure and the entire unsafe boundary left as `todo!()` stubs for you to implement, each backed by a test.

A Bloom filter is a small probabilistic set: `add` items, then ask `contains` and get back *"definitely not present"* or *"probably present"* — never a false negative, a tunable false-positive rate, a few bits per element. It's the data structure every database puts in front of expensive disk reads, and a near-perfect FFI specimen: its operations exercise every hard part of a C boundary — opaque handles, byte buffers, C strings, structs by value, and returning owned memory.

## What it does

- Implements a Bloom filter (`new`/`add`/`contains`/serialize) as plain, `unsafe`-free Rust in `src/bloom.rs` + `src/hash.rs`.
- Exposes it over a stable C ABI (`src/ffi.rs`, declared in `include/cbloom.h`) and builds as a `cdylib` (`libcbloom.so`/`.dylib`) and `staticlib` (`libcbloom.a`).
- Ships three consumers that call the same library with no Rust in sight: a C program (`consumers/consumer.c`), a Python program via `ctypes` (`consumers/consumer.py`), and a Node.js program via `koffi` (`consumers/consumer.js`).
- Optionally (`--features cffi`) calls a bundled C hash *from* Rust via `bindgen` — FFI in the other direction.

## What you'll build

- **The safe core** (`bloom.rs`, `hash.rs`): optimal `m`/`k` sizing from a target false-positive rate, the `k`-probe `add`/`contains` over a packed bit array, FNV-1a double hashing, and a self-describing serialize/deserialize format.
- **The C ABI** (`ffi.rs`) — the only `unsafe` in the crate: the 11 functions of `cbloom.h`, covering the opaque-handle lifecycle (`Box::into_raw`/`from_raw`), byte/string membership (`slice::from_raw_parts`, `CStr`), a `#[repr(C)]` stats struct returned by value, and an owned-buffer serialize/free pair (the ownership-transfer pattern). Every entry point is null-safe and wraps its body in `catch_unwind` so no panic unwinds into C.
- **The reverse binding** (`sys.rs`, stretch): a safe wrapper over a `bindgen`-generated C hash.

## Running it

The safe core, the C ABI, and the reverse binding are all implemented — `cargo test` and `cargo test --features cffi` are green. The workflow:

```bash
cargo test                  # unit + integration + in-process FFI tests — all pass
cargo bench                 # criterion: add throughput, contains hit/miss
./consumers/run.sh          # build the cdylib, then run the C and Python consumers against it
```

`run.sh` compiles and dynamically links the C program and loads the same library from Python; both print all `[ok]`.

The Node.js consumer uses [`koffi`](https://koffi.dev), a pure-FFI library that `dlopen`s the same `.dylib` and marshals `#[repr(C)]` structs by value (needed for `cbloom_get_stats` / `cbloom_serialize`). It needs its one dependency installed first:

```bash
npm install --prefix consumers          # installs koffi into consumers/node_modules
cargo build --release                   # produce target/release/libcbloom.{dylib,so}
CBLOOM_LIB=target/release/libcbloom.dylib node consumers/consumer.js   # all [ok]
```

The reverse-FFI (bindgen) path is gated so the default build needs no extra tooling:

```bash
cargo test --features cffi  # compiles cbits/fnv.c with `cc`, binds it with `bindgen` (needs libclang),
                            # and asserts the C and Rust hashes agree
```

Verify the C exports any time with `nm -gU target/release/libcbloom.dylib | grep cbloom` (or `objdump -T libcbloom.so` on Linux).

## How it works

The crate is two layers, and the split *is* the lesson:

- **Safe first.** The whole filter is debugged in ordinary safe Rust — no raw pointers, no `unsafe` — where the borrow checker still has your back. You only wrap it once it's correct.
- **The boundary owns the invariants.** `ffi.rs` upholds by hand what the compiler normally proves: an opaque `*mut Bloom` is freed exactly once (`Box::into_raw`/`from_raw`); incoming `(ptr, len)` pairs and C strings are rebuilt into short-lived borrows (`slice::from_raw_parts`, `CStr::from_ptr`) and never outlive the call; a serialized buffer is Rust-allocated, so C must return it to `cbloom_buffer_free` (never `free()`), made round-trippable by `into_boxed_slice`; and a panic is caught and turned into a null/`false`/zeroed-struct error rather than unwinding into a C frame.
- **One ABI, many callers.** Because the surface is `extern "C"` with `#[no_mangle]` symbols and `#[repr(C)]` structs, the identical `.so` is called from C (compiled + linked), Python (`ctypes`), and Node.js (`koffi`) — the proof that a C ABI makes Rust consumable from any language with an FFI.

Depth — what `unsafe` actually unlocks, the precise UB you're contracting against, `bindgen`/`cbindgen`, and verifying the boundary with Miri — lives in the learn file.

## Project layout

| File | Status |
| --- | --- |
| `Cargo.toml` | Given — `crate-type = [lib, cdylib, staticlib]`, the `cffi` feature, release profile (LTO, unwinding kept on purpose). |
| `src/lib.rs` | Given — module map and crate docs. |
| `src/hash.rs` | Done — `fnv1a`, `hash_pair`, `bit_index` (double hashing). |
| `src/bloom.rs` | Done — `new` (sizing), `add`, `contains`, `to_bytes`, `from_bytes`. `with_params`/`set_bit`/`get_bit` given. |
| `src/ffi.rs` | Done — all 11 `extern "C"` functions. The `#[repr(C)]` structs and in-process boundary tests are given. |
| `src/sys.rs` | Done (stretch, `--features cffi`) — safe wrapper over the bindgen'd C hash. |
| `include/cbloom.h` | Given — the C-side contract for `ffi.rs`. |
| `cbits/fnv.c`, `cbits/fnv.h` | Given — a C FNV-1a hash, bound from Rust under `cffi`. |
| `build.rs` | Given — no-op by default; compiles + binds the C under `cffi`. |
| `consumers/consumer.c` | Given — a C program using the library through `cbloom.h`. |
| `consumers/consumer.py` | Given — the same program in Python via `ctypes`. |
| `consumers/consumer.js` | Added — the same program in Node.js via `koffi` (`npm install --prefix consumers`). |
| `consumers/run.sh` | Given — build the lib, then run both consumers. |
| `tests/integration.rs` | Given — safe-API and FFI-boundary tests, incl. a serialize round-trip. |
| `benches/bloom.rs` | Given — criterion benchmarks for `add` and `contains`. |

## Status

Complete. `src/hash.rs`, `src/bloom.rs`, `src/ffi.rs`, and the stretch `src/sys.rs` are all implemented; `cargo test` and `cargo test --features cffi` are green, and the C, Python, and Node.js consumers all print `[ok]` against the same `.dylib`. The concept pills and the step-by-step build — covering `unsafe`, undefined behavior, raw pointers, the C ABI, `repr(C)`, opaque handles, buffers and strings across FFI, ownership transfer, panics at the boundary, and `bindgen`/`cbindgen`/Miri — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [MIT license](https://opensource.org/licenses/MIT) or [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
