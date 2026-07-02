//! Calling C *from* Rust — the inbound FFI direction (Pill 11).
//!
//! Only compiled with `--features cffi`. `build.rs` ran `bindgen` over
//! `cbits/fnv.h` and wrote raw bindings to `$OUT_DIR/fnv_bindings.rs`; we
//! `include!` them below. bindgen emits *raw, unsafe* declarations — an
//! `extern "C"` block and `unsafe fn` — and the idiomatic move is to wrap each
//! one in a small **safe** function that encodes the C contract (here: "the
//! pointer and length describe a valid byte range"), so the rest of the program
//! never touches `unsafe`.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]

// The generated bindings: `pub fn cbloom_fnv1a64(data: *const u8, len: usize) ->
// u64;` inside an `extern "C"` block. (In a default build this file isn't
// compiled, so the missing OUT_DIR file never matters.)
include!(concat!(env!("OUT_DIR"), "/fnv_bindings.rs"));

/// Safe wrapper over the C `cbloom_fnv1a64`: hash a byte slice by calling into
/// the bundled C library.
///
/// This is the payoff of the inbound direction — a one-line safe API backed by
/// C. The `unsafe` is justified because a `&[u8]` *always* gives a valid
/// `(ptr, len)` for its whole length, which is exactly the C function's
/// precondition.
///
/// TODO (stretch / step 7): call the bound `cbloom_fnv1a64` inside an `unsafe`
/// block, passing `data.as_ptr()` and `data.len()`. See the Pill 11 hint.
pub fn fnv1a_64(data: &[u8]) -> u64 {
    let _ = data;
    todo!("sys.rs: in an unsafe block, call cbloom_fnv1a64(data.as_ptr(), data.len())")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C hash and the Rust reimplementation in `hash.rs` must agree — proof
    /// that the binding works *and* that your Rust FNV is correct. Run with
    /// `cargo test --features cffi`.
    #[test]
    fn c_and_rust_fnv_agree() {
        for input in [&b""[..], b"a", b"hello world", b"\x00\xff\x10"] {
            // `hash.rs` keeps `fnv1a` private; its public `hash_pair` starts from
            // the same offset basis for `h1`, so the C hash must equal that arm.
            let (rust_h1, _) = crate::hash::hash_pair(input);
            assert_eq!(fnv1a_64(input), rust_h1, "mismatch on {input:?}");
        }
    }
}
