# Unsafe Rust & FFI — in 5-Minute Pills

## Goal

Build a real, useful data structure — a **Bloom filter** — as a safe Rust core, then wrap it in a hand-written C ABI and ship it as a C library that a **C program** and a **Python program** both call without knowing Rust exists. Along the way you learn the two things this module is actually about: what `unsafe` *promises* (and the undefined behavior waiting if you break the promise), and how to move data — handles, byte buffers, strings, structs, owned memory — across the boundary between Rust's world and C's without leaking, double-freeing, or corrupting anything. By the end you can take any Rust crate and give it a C ABI other languages can consume, which is how Rust actually lands in existing C/C++/Python codebases.

## Time estimate

~1 day (13 pills × 5 min + project)

## What you'll learn

- **What `unsafe` actually is** — not "turn off safety" but "I'm upholding an invariant the compiler can't check"; the five superpowers it unlocks and nothing more
- **Undefined behavior as a contract** — the specific things (dangling derefs, aliasing `&mut`, invalid values, data races) that are UB, why UB is worse than a crash, and that *you* now own the proofs
- **Raw pointers** — `*const T` / `*mut T`, how they differ from references and `Box`, and the rules for dereferencing one soundly
- **The C ABI** — `extern "C"`, `#[no_mangle]`, calling conventions, and why a stable symbol is what makes cross-language linking possible
- **`#[repr(C)]`** — why Rust's default struct layout is deliberately unspecified, and how to pin it so C and Rust agree on field offsets
- **The opaque-handle pattern** — `Box::into_raw` / `Box::from_raw` to hand C an owning pointer to a Rust type it can't see inside
- **Buffers and strings across FFI** — rebuilding a `&[u8]` from `(ptr, len)`, and the `CStr` / `CString` dance for NUL-terminated C strings
- **Ownership transfer of memory** — who allocated it must free it; the paired-free-function pattern and why `free()` on a Rust pointer is UB
- **Panics and FFI** — unwinding across a C frame is UB; catching panics at the boundary with `catch_unwind`
- **Calling C *from* Rust** — `bindgen` + `cc` in a build script, and wrapping the generated `unsafe` in a safe API
- **Proving it** — generating the header with `cbindgen`, and checking the `unsafe` for UB with **Miri**

## Concepts

### Pill 1: The Workload — a Bloom Filter

The data structure is small enough to fit in your head and useful enough to ship. A **Bloom filter** is a probabilistic set: you `add` items and later ask `contains`, and it answers either *"definitely not present"* or *"probably present."* It never gives a false negative — anything you added always reports present — but it allows a tunable rate of false positives. In exchange it uses a few **bits** per element instead of storing elements at all, so a filter for a million items fits in ~1–2 MB regardless of how big the items are. That's why RocksDB, Cassandra, Bigtable, and CDNs put one in front of expensive lookups: "is this key *maybe* on disk?" — if the filter says no, skip the read entirely.

The mechanism is a bit array of `m` bits and `k` hash functions. To `add(x)`, hash `x` to `k` positions and set those bits. To query `contains(x)`, hash to the same `k` positions and answer "probably present" only if *all* of them are set — one clear bit proves absence (you'd have set it on insert). Sizing is a closed-form trade-off between `m`, the item count `n`, and the false-positive rate `p`:

```text
m = ceil( -(n * ln p) / (ln 2)^2 )      // bits needed
k = round( (m / n) * ln 2 )             // optimal number of probes
```

For 1000 items at 1% you get m ≈ 9586 bits (~1.2 KB) and k = 7. That's the whole algorithm; the rest of this module is about exposing it over a C boundary.

### Pill 2: What `unsafe` Actually Means

`unsafe` is the most misunderstood keyword in Rust. It does **not** turn off the borrow checker, the type system, or bounds checks on `Vec` indexing. It does exactly one thing: it unlocks **five extra abilities** that the compiler cannot verify are used correctly, and in exchange it asks *you* to guarantee they are. The five:

1. Dereference a raw pointer (`*const T`, `*mut T`).
2. Call an `unsafe` function (including every `extern "C"` function from another language).
3. Access or modify a mutable `static`.
4. Implement an `unsafe` trait (`Send`, `Sync`, ...).
5. Access the fields of a `union`.

That's the entire list. Everything else inside an `unsafe` block is checked exactly as rigorously as safe Rust. So `unsafe` is not "trust me, anything goes" — it's a precise, auditable marker that says *"a human checked the invariant the compiler couldn't."* The right mental model: `unsafe fn` is a function with a **precondition in its documentation** that the caller must uphold, and the `unsafe` block is you asserting "I've upheld it." This is why every `unsafe fn` in `ffi.rs` has a `# Safety` doc comment: it's the contract, written down.

### Pill 3: Undefined Behavior — the Contract You're Upholding

The reason `unsafe` is serious is **undefined behavior (UB)**. When you break the contract, the result isn't a defined error or a guaranteed crash — it's that the compiler was allowed to assume the bad thing never happens, so the program's meaning becomes *undefined*. The optimizer may have deleted code, reordered it, or assumed a pointer was non-null; with the assumption violated, you get miscompilation, silent corruption, "impossible" behavior, or a crash that reproduces only in release builds on Tuesdays. UB is worse than a crash because it doesn't announce itself.

The core list of what's UB in Rust (the [reference](https://doc.rust-lang.org/reference/behavior-considered-undefined.html) has the full set):

- Dereferencing a **dangling** or **misaligned** pointer, or one past the end of an allocation.
- Producing an invalid value: a `bool` that isn't 0/1, a reference that's null, a `char` out of range, an uninitialized integer read as if initialized.
- Breaking **aliasing**: having two `&mut` to the same data, or a `&mut` while a `&` exists. This one bites hardest at FFI: when you turn a raw pointer into `&mut *p`, you are *promising* nothing else aliases it for that reference's lifetime.
- **Data races**: unsynchronized concurrent access where at least one is a write.

Your job in this module is to write `unsafe` code where you can *prove* none of these occur, given the contract you document for your callers. "C passed me a valid, non-null, correctly-aligned pointer to `len` initialized bytes" is the kind of precondition you state and then rely on.

### Pill 4: Raw Pointers — `*const T` and `*mut T`

A raw pointer is an address with the type system's guarantees stripped off. Unlike `&T`/`&mut T`, a raw pointer can be null, can dangle, can be unaligned, can alias freely, and carries **no lifetime** — the compiler will not stop you from keeping one after the thing it points to is gone. Creating one is safe (`&x as *const T`, or `Box::into_raw`); **dereferencing** one is `unsafe`, because that's the moment the guarantees matter. The two flavors, `*const T` and `*mut T`, differ only by intent and which methods you get; you can cast between them.

Raw pointers are how C hands you data and how you hand data back. The disciplined pattern at every boundary is: **convert the raw pointer to a safe reference or slice as late as possible, use it for as short a time as possible, and never let it outlive the call.** `let r: &mut Bloom = unsafe { &mut *ptr };` is sound only if `ptr` is non-null, aligned, points to a live `Bloom`, and nothing else aliases it while `r` lives. You check the first three with a null guard and your documented contract; you ensure the fourth by not stashing the reference anywhere.

### Pill 5: The C ABI — `extern "C"` and `#[no_mangle]`

For C to call your Rust function, two things have to be true. First, the function must use the **C calling convention** — how arguments go in registers/stack, who cleans up — which you request with `extern "C" fn`. Rust's own ABI is unstable and unspecified; `extern "C"` opts into the platform's stable C ABI that every language's FFI speaks. Second, the symbol must have a **predictable name**. Rust *mangles* symbol names by default (encoding the module path and types), so `cbloom_new` would link as something like `_ZN6cbloom3ffi10cbloom_new17h...E`. `#[no_mangle]` turns mangling off, exporting the bare name `cbloom_new` that C's linker looks for. Together:

```rust
#[no_mangle]
pub extern "C" fn cbloom_new(expected_items: usize, fp_rate: f64) -> *mut Bloom { ... }
```

You also have to tell Cargo to emit a C-consumable artifact. `crate-type = ["cdylib"]` produces a shared library (`.so`/`.dylib`/`.dll`) for dynamic loading (what Python's `ctypes` opens); `["staticlib"]` produces a `.a` archive for static linking into a C program. Adding `"lib"` as well keeps the normal Rust rlib so your own tests and benches can still `use cbloom`. You can verify the exports with `nm -gU target/release/libcbloom.dylib | grep cbloom`.

### Pill 6: `#[repr(C)]` — a Layout You Can Rely On

When you return a struct *by value* to C — like `cbloom_stats` — both sides must agree on the exact byte layout: field order, offsets, padding. By default Rust uses `repr(Rust)`, which is **deliberately unspecified**: the compiler may reorder fields to minimize padding, and the order can change between compiler versions. C has no idea about that. `#[repr(C)]` forces the struct to use C's layout rules — fields in declaration order, C's padding and alignment — so the Rust `CBloomStats` and the C `cbloom_stats` are bit-for-bit identical:

```rust
#[repr(C)]
pub struct CBloomStats { pub num_bits: usize, pub num_hashes: u32, pub approx_items: u64 }
```

The matching C side declares the fields in the same order with the same types (`size_t`, `uint32_t`, `uint64_t`). Get this wrong — reorder a field, use `repr(Rust)`, mismatch a width — and C reads the right bytes as the wrong fields, with no error, just garbage. `repr(C)` is also what makes a struct safe to pass by pointer across the boundary, and it's required for any enum or struct you expose. Scalars (`usize`, `f64`, `*mut T`, `bool`) already have a well-defined C representation, so functions taking only those don't need it.

### Pill 7: The Opaque Handle — `Box::into_raw` / `Box::from_raw`

C must not see the inside of a `Bloom` — its fields are Rust types (`Vec<u64>`) with no C equivalent. The pattern is an **opaque handle**: C holds a `cbloom *` it can pass around but never dereference, and only your Rust functions know it's really a `*mut Bloom`. You mint one by moving a `Box<Bloom>` onto the heap and **leaking** it out of Rust's ownership:

```rust
let handle: *mut Bloom = Box::into_raw(Box::new(Bloom::new(n, p)));   // Rust forgets it owns this
```

`Box::into_raw` hands back a raw pointer and *suppresses the `Box`'s destructor* — Rust will no longer free it. Ownership has crossed into C's hands. C uses it, then gives it back to your free function, which reclaims ownership and drops it:

```rust
unsafe { drop(Box::from_raw(handle)); }   // Rust owns it again, and the Box drop frees it
```

The invariants you now own by hand: free it **exactly once** (double-free is UB), and never use it after free. Those used to be the borrow checker's job; across an opaque handle they're documented contract. A null handle should be a safe no-op in `free`, mirroring C's `free(NULL)`.

### Pill 8: Passing Buffers — `(ptr, len)` → `slice::from_raw_parts`

C has no `&[u8]`. A byte range is always a **pointer plus a length**, passed as two arguments. To work with it as a Rust slice for the duration of a call:

```rust
let bytes: &[u8] = unsafe { std::slice::from_raw_parts(data, len) };
```

This is `unsafe` because you are *asserting*, on C's behalf, the whole contract `from_raw_parts` requires: `data` is non-null and aligned, it points to `len` consecutive **initialized** `u8`s in a single allocation, and that memory won't be mutated for the slice's lifetime. None of that is checkable — it's your documented precondition ("`data` must point to at least `len` readable bytes"). The slice **borrows**; it does not own. You read from it during the call and let it drop — you must never free `data` (it's C's) and never keep the slice past the call. A null `data` is the one thing you *can* check: guard it and treat it as empty rather than calling `from_raw_parts` with null (which is instant UB). The write direction — handing C a buffer Rust owns — is Pill 10.

### Pill 9: Strings Across the Boundary — `CStr` and `CString`

A C string is a `*const c_char` pointing at bytes terminated by a NUL (`\0`), with no length and no encoding guarantee. Rust's `String`/`&str` are UTF-8 with an explicit length and **no** interior-NUL rule. Bridging them is `CStr` (a borrowed view of a C string) and `CString` (an owned, NUL-terminated buffer you build to pass *to* C):

```rust
let s: &CStr = unsafe { CStr::from_ptr(ptr) };   // borrow C's string; UB if ptr is null/not NUL-terminated
let bytes: &[u8] = s.to_bytes();                  // the bytes, without the NUL — feed straight to Bloom::add
```

`CStr::from_ptr` walks memory until it finds a NUL, so the contract is "valid pointer to an actually-NUL-terminated string" — a missing terminator reads off into UB. Note `to_bytes()` gives you raw bytes with no UTF-8 check, which is exactly right for hashing (a Bloom filter doesn't care about encoding). If you instead needed a `&str` you'd call `.to_str()` and handle the `Utf8Error`. Going the other way, `CString::new(bytes)` allocates a NUL-terminated copy you can pass to C — and you must keep it alive as long as C holds the pointer.

### Pill 10: Returning Owned Memory — Who Frees?

The single most common FFI memory bug: Rust allocates a buffer, hands C the pointer, and C calls `free()` on it. That's **UB**, because Rust's allocator and C's `malloc`/`free` are different allocators — `free` has no idea about the allocation Rust made. The rule is absolute: **whoever allocated it must free it.** So when `cbloom_serialize` returns a heap buffer, you also ship `cbloom_buffer_free`, and the header documents that C must use it and never `free()`.

The mechanics: a `Vec<u8>` has three parts — pointer, length, *capacity* — and to free it you need all three. But the C side only carries pointer + length. The clean fix is to first shrink the allocation to exactly fit, by converting to a boxed slice, so capacity == length and ptr+len is enough to reconstruct it:

```rust
// serialize: leak an exactly-sized allocation out to C
let boxed: Box<[u8]> = bf.to_bytes().into_boxed_slice();
let len = boxed.len();
let data = Box::into_raw(boxed) as *mut u8;        // ptr; Rust forgets it
CBloomBuffer { data, len }

// free: rebuild the exact same Box<[u8]> and drop it
let slice = std::slice::from_raw_parts_mut(buf.data, buf.len);
drop(Box::from_raw(slice as *mut [u8]));
```

`into_boxed_slice` is what makes `(ptr, len)` round-trippable — no separate capacity to smuggle across. Return `{ null, 0 }` to signal failure, and make `cbloom_buffer_free` a no-op on a null pointer.

### Pill 11: Panics Must Not Unwind Into C

If Rust code panics and the panic **unwinds** out of an `extern "C"` function into the C caller, that is undefined behavior: C frames have no unwinding tables and the runtime can't walk them. Modern Rust makes `extern "C"` abort the process on an escaping panic rather than unwind, which is *safe* but still a hard crash you usually don't want at a library boundary. The disciplined fix is to **catch the panic** and convert it into a normal error value the C contract already allows (a null pointer, `false`, a zeroed struct):

```rust
let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
    // ... the real work that might panic (e.g. an allocation failure) ...
}));
result.unwrap_or(std::ptr::null_mut())   // panic -> null, never unwinds into C
```

`catch_unwind` stops an unwinding panic at this frame and hands you `Err`. `AssertUnwindSafe` is your assertion that it's fine to observe state after a panic here (the raw pointers don't have safe-Rust invariants to violate). This is also why this crate's release profile does **not** set `panic = "abort"`: `catch_unwind` only works with unwinding panics. Wrap every `extern "C"` entry point's body this way.

### Pill 12: Calling C *from* Rust — `bindgen`

FFI runs both directions. To call a C function *from* Rust you need a declaration of it — an `extern "C"` block naming the symbol and its signature — and the compiled C linked in. Writing those declarations by hand is tedious and error-prone for real headers, so **`bindgen`** generates them: point it at a `.h` file and it emits the Rust `extern "C"` block (and any `repr(C)` structs) automatically. You drive it from a **build script** (`build.rs`) so it runs at compile time, alongside the **`cc`** crate which compiles the C source into the crate:

```rust
// build.rs (under --features cffi)
cc::Build::new().file("cbits/fnv.c").include("cbits").compile("cfnv");
let bindings = bindgen::Builder::default().header("cbits/fnv.h")
    .allowlist_function("cbloom_fnv1a64").generate().unwrap();
bindings.write_to_file(out_dir.join("fnv_bindings.rs")).unwrap();
```

bindgen emits **raw, unsafe** bindings (`pub fn cbloom_fnv1a64(data: *const u8, len: usize) -> u64;`). The idiomatic move is to `include!` them in a `sys` module and wrap each in a small **safe** function that encodes the C contract — here, "a `&[u8]` always yields a valid `(ptr, len)`," so `fnv1a_64(&[u8]) -> u64` can be a safe API over an unsafe call. (bindgen needs `libclang` installed; that's why this crate gates it behind a feature so the default build needs no extra tooling.)

### Pill 13: Generating the Header (`cbindgen`) and Proving It (Miri)

Two finishing tools. **`cbindgen`** is bindgen in reverse: it reads your Rust `extern "C"` surface and generates the **C header** (`cbloom.h`) for consumers — so the header can't drift out of sync with the code. You run it as `cbindgen --output include/cbloom.h` (or from a build script). This project ships a hand-written header so it reads as documentation, but on a real crate you'd generate it.

**Miri** is how you check that your `unsafe` is actually sound. It's an interpreter for Rust's mid-level IR that executes your tests while watching for UB the normal compiler can't catch at runtime: out-of-bounds raw accesses, use-after-free, invalid values, and aliasing violations (via Stacked/Tree Borrows). Because this crate's FFI entry points are exercised by in-process `#[test]`s (the same `unsafe` calls C makes, but runnable under `cargo test`), you can run them under Miri:

```bash
rustup +nightly component add miri
cargo +nightly miri test
```

A green Miri run on your boundary tests is strong evidence your pointer handling, slice reconstruction, and Box round-trips don't have UB — the closest thing to a proof you'll get without one. (Miri can't cross into the actual C code, so it runs the pure-Rust and FFI-shim tests, not the `cffi` C-hash path.)

## Project: `cbloom` — a Bloom filter with a C ABI

Build a Bloom filter as a safe Rust core, wrap it in a hand-written C ABI, and drive it from C and Python. The crate compiles today (`cargo check --all-targets` is clean); the library logic and the entire FFI boundary are `todo!()` stubs, each backed by tests that fail until you implement it.

### Requirements

1. A working safe Bloom filter: correct sizing, `add`, `contains` with **no false negatives** and a false-positive rate near the configured target, plus serialize/deserialize.
2. A complete `extern "C"` surface (the 11 functions in `include/cbloom.h`) over the safe core: opaque handle lifecycle, byte/string membership, a by-value stats struct, and an owned-buffer serialize/free pair.
3. Every FFI entry point is null-safe and panic-safe (no unwinding into C).
4. The in-crate tests pass (`cargo test`), and both consumers run green (`./consumers/run.sh`).
5. (Stretch) The `--features cffi` path builds and `cargo test --features cffi` proves the Rust and C hashes agree.

### Starter files

- `src/hash.rs` — TODO: `fnv1a`, `hash_pair`, `bit_index` (the double-hashing scheme).
- `src/bloom.rs` — TODO: `Bloom::new` (sizing), `add`, `contains`, `to_bytes`, `from_bytes`. `with_params`, `set_bit`, `get_bit` are given.
- `src/ffi.rs` — TODO: all 11 `extern "C"` functions. The `#[repr(C)]` structs and the in-process boundary tests are given.
- `src/sys.rs` — TODO (stretch): the safe wrapper `fnv1a_64` over the bindgen'd C hash. Only built with `--features cffi`.
- `include/cbloom.h`, `cbits/fnv.c`, `consumers/consumer.c`, `consumers/consumer.py`, `consumers/run.sh`, `build.rs`, `benches/bloom.rs`, `tests/integration.rs` — all given.

### Your task

1. **The hasher (`hash.rs`, step 2 in the stubs).** Implement FNV-1a and the `(h1 + i·h2) mod m` index scheme. `cargo test hash` checks it against published vectors.
2. **The safe filter (`bloom.rs`, step 1).** Size `m`/`k` in `new`, then `add`/`contains` over `k` bits. `cargo test bloom` checks the no-false-negatives guarantee and the FP rate. (Do steps 1 & 2 together — the filter calls the hasher.)
3. **Serialization (`bloom.rs`, step 3).** `to_bytes`/`from_bytes` with a magic + version header that rejects garbage.
4. **Handle lifecycle (`ffi.rs`, step 4).** `cbloom_new` / `cbloom_free` via `Box::into_raw`/`from_raw`, panic-caught and null-safe.
5. **Membership (`ffi.rs`, step 5).** `cbloom_add` / `cbloom_contains` and the `_str` variants — slices from `(ptr, len)`, bytes from `CStr`.
6. **Stats & serialization (`ffi.rs`, step 6).** `cbloom_get_stats` (by-value struct), `cbloom_serialize` / `cbloom_buffer_free` (owned buffer), `cbloom_deserialize`.
7. **Run the consumers.** `cargo test`, then `./consumers/run.sh` — both the C and Python programs should print all `[ok]`.
8. **(Stretch) Call C from Rust (`sys.rs`, step 7).** Implement `fnv1a_64` and `cargo test --features cffi`.

### Hints

<details>
<summary>Hint for step 2 (hash.rs)</summary>

FNV-1a is a fold: start from the basis, and for each byte xor-then-multiply.

```rust
fn fnv1a(data: &[u8], basis: u64) -> u64 {
    let mut hash = basis;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
```

For the pair, run it from two different bases so the two hashes are independent, and force the second odd so `i * h2` keeps stepping across the array:

```rust
pub fn hash_pair(item: &[u8]) -> (u64, u64) {
    (fnv1a(item, FNV_OFFSET), fnv1a(item, 0x100_0000_01b3) | 1)
}
pub fn bit_index(h1: u64, h2: u64, i: u32, num_bits: usize) -> usize {
    (h1.wrapping_add((i as u64).wrapping_mul(h2)) % num_bits as u64) as usize
}
```
</details>

<details>
<summary>Hint for step 1 (bloom.rs new / add / contains)</summary>

Translate the sizing formulas directly, clamp, and hand off to `with_params`:

```rust
pub fn new(expected_items: usize, fp_rate: f64) -> Bloom {
    let n = expected_items.max(1) as f64;
    let m = (-(n * fp_rate.ln()) / (2.0_f64.ln().powi(2))).ceil() as usize;
    let k = ((m as f64 / n) * 2.0_f64.ln()).round() as u32;
    Self::with_params(m.max(1), k.max(1))
}
```

`add` and `contains` are the same loop, one writing, one reading:

```rust
pub fn add(&mut self, item: &[u8]) {
    let (h1, h2) = hash::hash_pair(item);
    for i in 0..self.num_hashes {
        let idx = hash::bit_index(h1, h2, i, self.num_bits);
        self.set_bit(idx);
    }
    self.inserted += 1;
}
pub fn contains(&self, item: &[u8]) -> bool {
    let (h1, h2) = hash::hash_pair(item);
    (0..self.num_hashes).all(|i| self.get_bit(hash::bit_index(h1, h2, i, self.num_bits)))
}
```

`.all()` short-circuits on the first clear bit — the fast "definitely absent" path.
</details>

<details>
<summary>Hint for step 3 (to_bytes / from_bytes)</summary>

Write a self-describing header, then the words, all little-endian:

```rust
pub fn to_bytes(&self) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&self.num_hashes.to_le_bytes());
    out.extend_from_slice(&self.inserted.to_le_bytes());
    out.extend_from_slice(&(self.num_bits as u64).to_le_bytes());
    for w in &self.bits { out.extend_from_slice(&w.to_le_bytes()); }
    out
}
```

`from_bytes` validates *before* trusting any length: check the 4 magic bytes and the version, read the header fields with `u64::from_le_bytes(slice.try_into().ok()?)`, confirm the remaining byte count equals `(num_bits / 64) * 8`, then read the words. Any mismatch → `None`. Use `?` on the `try_into`s so a short buffer can't panic.
</details>

<details>
<summary>Hint for step 4 (cbloom_new / cbloom_free)</summary>

```rust
#[no_mangle]
pub extern "C" fn cbloom_new(expected_items: usize, fp_rate: f64) -> *mut Bloom {
    std::panic::catch_unwind(|| Box::into_raw(Box::new(Bloom::new(expected_items, fp_rate))))
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn cbloom_free(bf: *mut Bloom) {
    if bf.is_null() { return; }
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(bf))));
}
```

`Box::into_raw` leaks the box out of Rust's ownership; `Box::from_raw` claims it back so its drop runs. The null guard makes `free(NULL)` a no-op.
</details>

<details>
<summary>Hint for step 5 (add / contains, bytes and strings)</summary>

Null-check first, then build the safe view as late as possible and use it for just this call:

```rust
#[no_mangle]
pub unsafe extern "C" fn cbloom_add(bf: *mut Bloom, data: *const u8, len: usize) {
    if bf.is_null() || data.is_null() { return; }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let bytes = std::slice::from_raw_parts(data, len);
        (*bf).add(bytes);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn cbloom_contains_str(bf: *const Bloom, s: *const c_char) -> bool {
    if bf.is_null() || s.is_null() { return false; }
    catch_unwind(AssertUnwindSafe(|| (*bf).contains(CStr::from_ptr(s).to_bytes())))
        .unwrap_or(false)
}
```

`CStr::from_ptr(...).to_bytes()` is the string's bytes without the NUL — exactly what `add`/`contains` hash. The byte and string variants differ only in how they get the `&[u8]`.
</details>

<details>
<summary>Hint for step 6 (stats, serialize, buffer_free, deserialize)</summary>

Stats is plain field copying into the `repr(C)` struct (zeroed on null). The buffer pair is the ownership-transfer dance from Pill 10:

```rust
#[no_mangle]
pub unsafe extern "C" fn cbloom_serialize(bf: *const Bloom) -> CBloomBuffer {
    if bf.is_null() { return CBloomBuffer { data: std::ptr::null_mut(), len: 0 }; }
    catch_unwind(AssertUnwindSafe(|| {
        let boxed = (*bf).to_bytes().into_boxed_slice();   // exact-size alloc
        let len = boxed.len();
        CBloomBuffer { data: Box::into_raw(boxed) as *mut u8, len }
    })).unwrap_or(CBloomBuffer { data: std::ptr::null_mut(), len: 0 })
}

#[no_mangle]
pub unsafe extern "C" fn cbloom_buffer_free(buf: CBloomBuffer) {
    if buf.data.is_null() { return; }
    let slice = std::slice::from_raw_parts_mut(buf.data, buf.len);
    drop(Box::from_raw(slice as *mut [u8]));               // rebuild the exact Box<[u8]>
}
```

`into_boxed_slice` is what lets `(ptr, len)` round-trip with no capacity to smuggle. `cbloom_deserialize` is the mirror of `cbloom_new`: borrow the input slice, run `Bloom::from_bytes`, `Box::into_raw` on `Some`, null on `None`.
</details>

<details>
<summary>Hint for step 7 (sys.rs, the bindgen wrapper)</summary>

The whole point of bindgen is that the raw declaration already exists; you just wrap it. A `&[u8]` is always a valid `(ptr, len)`, so the wrapper is sound:

```rust
pub fn fnv1a_64(data: &[u8]) -> u64 {
    unsafe { cbloom_fnv1a64(data.as_ptr(), data.len()) }
}
```

Run it with `cargo test --features cffi` (needs a C compiler and `libclang`). The test asserts this equals the `h1` arm of your Rust `hash_pair` — proof the binding works and your FNV is correct.
</details>

## Stretch goals

- **Generate the header.** `cargo install cbindgen`, then `cbindgen --lang c --output include/cbloom.generated.h` and diff it against the hand-written `cbloom.h`. Wire it into `build.rs` so it can never drift.
- **Run Miri on the boundary.** `cargo +nightly miri test` over the `ffi` and integration tests. Then deliberately introduce a use-after-free (free a handle, then query it) and watch Miri catch it.
- **A scalable/counting variant.** Add `cbloom_union` (OR two filters' bit arrays — only valid when `m` and `k` match; return an error code if not) or a counting Bloom filter that supports `remove`.
- **A third consumer.** Call the same `.so` from Node (`ffi-napi`) or Go (`cgo`) to prove the ABI is genuinely language-agnostic.
- **Property test the FP rate.** With `proptest`, insert N random items and assert the measured false-positive rate stays within a factor of the theoretical `(1 - e^(-kn/m))^k`.

## Key questions

- Why is *dereferencing* a raw pointer `unsafe` but *creating* one safe? What invariants can only be checked at the dereference?
- Your `cbloom_add` turns `*mut Bloom` into `&mut *bf`. What exactly are you promising about aliasing when you do that, and how does the single-threaded C contract let you keep that promise?
- Why can't C call `free()` on the pointer from `cbloom_serialize`? What specifically goes wrong, and why does `into_boxed_slice` make the paired free function correct?
- What happens *without* `catch_unwind` if `Bloom::new` panics inside `cbloom_new` — in current Rust, and in older Rust? Why is unwinding into C UB?
- `#[repr(C)]` on `CBloomStats` but not on the scalar-only functions — why is the struct special and the scalars fine?
- A Bloom filter never has false negatives but allows false positives. Which direction of error would be catastrophic for a database's "is this key on disk?" check, and why is the filter's guarantee the right way round?

## Resources

- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) — the definitive guide to unsafe Rust and FFI; read "Meet Safe and Unsafe" and the FFI chapter.
- [Reference: Behavior considered undefined](https://doc.rust-lang.org/reference/behavior-considered-undefined.html) — the authoritative UB list.
- [`std::ptr`](https://doc.rust-lang.org/std/ptr/index.html), [`std::slice::from_raw_parts`](https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html), [`std::ffi`](https://doc.rust-lang.org/std/ffi/index.html) — the safety contracts are written in the docs; read them.
- [The bindgen book](https://rust-lang.github.io/rust-bindgen/) and [cbindgen](https://github.com/mozilla/cbindgen) — the two directions of binding generation.
- [Miri](https://github.com/rust-lang/miri) — the UB detector.
- [Bloom filter sizing calculator](https://hur.st/bloomfilter/) — sanity-check your `m`/`k` math interactively.
- Kirsch & Mitzenmacher, ["Less Hashing, Same Performance"](https://www.eecs.harvard.edu/~michaelm/postscripts/rsa2008.pdf) — why two hashes suffice for `k` probes.
