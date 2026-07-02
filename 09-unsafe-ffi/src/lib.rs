//! `cbloom` — a Bloom filter with a C ABI, built as an unsafe + FFI lab.
//!
//! A Bloom filter is a small probabilistic set: `add` an item, later ask
//! `contains` and get back *"definitely not in the set"* or *"probably in the
//! set"*. It never returns a false negative and only rarely a false positive,
//! using a few bits per element instead of storing the elements — which is why
//! every serious database (Cassandra, RocksDB, Bigtable) puts one in front of
//! its on-disk reads. It's the perfect FFI specimen: tiny, useful, and its
//! operations (`new`, `add`, `contains`, serialize) exercise *every* hard part
//! of a C boundary — opaque handles, byte slices, C strings, returning owned
//! memory, and structs by value.
//!
//! The crate is two layers, and the split is the whole lesson:
//!
//! - **The safe core** — [`bloom::Bloom`] and [`hash`] — is ordinary, totally
//!   safe Rust with no `unsafe` anywhere (Pills 1–3, 10). [`bloom::Bloom::new`]
//!   sizes the bit array from an expected item count and target false-positive
//!   rate; [`bloom::Bloom::add`]/[`bloom::Bloom::contains`] flip and probe `k`
//!   bits chosen by [`hash::bit_index`]; [`bloom::Bloom::to_bytes`] /
//!   [`bloom::Bloom::from_bytes`] round-trip it to a flat buffer.
//!
//! - **The `extern "C"` boundary** — [`ffi`] — is the only `unsafe` code in the
//!   crate (Pills 4–9). It wraps the safe core in `#[no_mangle] pub extern "C"`
//!   functions over raw pointers: [`ffi::cbloom_new`] hands C an opaque
//!   `*mut Bloom` via `Box::into_raw`, [`ffi::cbloom_add`] rebuilds a `&[u8]`
//!   from `(ptr, len)`, [`ffi::cbloom_serialize`] returns a heap buffer C must
//!   hand back to [`ffi::cbloom_buffer_free`], and every entry point catches
//!   panics so none unwind across the C frame.
//!
//! The same surface drives three consumers: the Rust tests/bench in this crate,
//! `consumers/consumer.c` (static-linked, via `include/cbloom.h`), and
//! `consumers/consumer.py` (dynamic, via `ctypes`). With `--features cffi` the
//! [`sys`] module flips the arrow around and calls a C hash *from* Rust through
//! `bindgen` (Pill 11).

pub mod bloom;
pub mod ffi;
pub mod hash;

/// Calling C from Rust via `bindgen` — only built with `--features cffi`,
/// because it needs a C compiler and libclang. See Pill 11.
#[cfg(feature = "cffi")]
pub mod sys;

pub use bloom::Bloom;
