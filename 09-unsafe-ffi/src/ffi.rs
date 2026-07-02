//! The C ABI — the only `unsafe` in the crate (Pills 4–9).
//!
//! Every function here is `#[no_mangle] pub extern "C"`: a stable, un-mangled
//! symbol using the C calling convention, so a C or Python program can call it
//! by name. The job of this layer is narrow and dangerous: translate between
//! C's world (raw pointers, lengths, null, C strings, structs by value) and the
//! safe [`Bloom`] core, upholding by hand every invariant the borrow checker
//! normally proves for you.
//!
//! The five rules this module lives by:
//!  1. **The opaque handle.** C never sees a `Bloom`; it sees a `*mut Bloom`
//!     minted by `Box::into_raw` and destroyed by `Box::from_raw`. (Pill 5)
//!  2. **Null is always possible.** Any incoming pointer may be null; check
//!     before deref. (Pill 6)
//!  3. **Borrow, don't free, what C owns.** `(ptr, len)` becomes a `&[u8]` for
//!     the duration of the call and nothing more. (Pill 7)
//!  4. **Memory crosses the boundary with an owner.** If Rust allocates a buffer
//!     and hands it to C, only Rust may free it — via a paired free function.
//!     (Pill 8)
//!  5. **Panics must not unwind into C.** Every body runs inside
//!     `catch_unwind`; a panic becomes a safe error value, never UB. (Pill 9)
//!
//! `include/cbloom.h` is the C-side declaration of exactly these functions and
//! types; keep the two in lockstep.

use crate::bloom::Bloom;
use std::os::raw::c_char;
// When you implement these, you'll also reach for `std::ffi::CStr` (C strings),
// `std::panic::catch_unwind` + `AssertUnwindSafe` (Pill 9), `std::slice` and
// `std::ptr`. They're named fully-qualified in the hints so the scaffold stays
// warning-clean until you add the `use`s.

/// Filter statistics, returned **by value** to C. `#[repr(C)]` pins the field
/// order and layout so it matches the `cbloom_stats` struct in `cbloom.h` (Pill
/// 4). Plain-old-data like this is the easy case of crossing the boundary.
#[repr(C)]
pub struct CBloomStats {
    pub num_bits: usize,
    pub num_hashes: u32,
    pub approx_items: u64,
}

/// An owned byte buffer handed to C: a `(pointer, length)` pair. Rust allocated
/// `data`, so C must return this exact struct to [`cbloom_buffer_free`] — it may
/// not call `free()` on `data` itself (Rust's allocator is not C's). A null
/// `data` with `len == 0` signals "allocation/serialize failed" (Pill 8).
#[repr(C)]
pub struct CBloomBuffer {
    pub data: *mut u8,
    pub len: usize,
}

/// Create a filter sized for `expected_items` at `fp_rate`; returns an opaque
/// handle, or null if construction panics.
///
/// The handle is a leaked `Box<Bloom>`: `Box::into_raw` transfers ownership out
/// of Rust's bookkeeping and into C's hands. C must give it back to
/// [`cbloom_free`] or the memory leaks.
///
/// TODO (step 4): inside `catch_unwind`, build a `Bloom`, `Box` it, and return
/// `Box::into_raw`. On a caught panic return `std::ptr::null_mut()`. See the
/// step-4 hint.
#[no_mangle]
pub extern "C" fn cbloom_new(expected_items: usize, fp_rate: f64) -> *mut Bloom {
    let _ = (expected_items, fp_rate);
    todo!("ffi.rs: inside std::panic::catch_unwind, Box::into_raw a boxed Bloom::new(..); null_mut on a caught panic")
}

/// Destroy a filter created by [`cbloom_new`]. Null is a safe no-op (matching
/// C's `free(NULL)`). Calling it twice on the same pointer is undefined — that's
/// the caller's contract, documented in the header.
///
/// # Safety
/// `bf` must be null or a pointer returned by [`cbloom_new`] and not yet freed.
///
/// TODO (step 4): if non-null, reclaim ownership with `Box::from_raw` and let
/// the `Box` drop. Wrap in `catch_unwind` so a `Drop` panic can't escape.
#[no_mangle]
pub unsafe extern "C" fn cbloom_free(bf: *mut Bloom) {
    let _ = bf;
    todo!("ffi.rs: when bf is non-null, reclaim it with Box::from_raw and let the Box drop; wrap in catch_unwind")
}

/// Add the `len` bytes at `data` to the filter. Null `bf` or null `data` is a
/// no-op.
///
/// # Safety
/// `bf` must be a live handle; `data` must point to at least `len` readable
/// bytes (or be null). The slice is borrowed only for this call.
///
/// TODO (step 5): null-check both pointers, rebuild the handle as `&mut *bf` and
/// the input as `std::slice::from_raw_parts(data, len)`, then call `Bloom::add`.
/// All inside `catch_unwind` (`AssertUnwindSafe` over the raw pointers).
#[no_mangle]
pub unsafe extern "C" fn cbloom_add(bf: *mut Bloom, data: *const u8, len: usize) {
    let _ = (bf, data, len);
    todo!("ffi.rs: (&mut *bf).add(std::slice::from_raw_parts(data, len)) with null checks + catch_unwind")
}

/// Convenience: add a NUL-terminated C string (its bytes, without the NUL).
/// Null `bf` or `s` is a no-op.
///
/// # Safety
/// `s` must be null or a valid NUL-terminated C string.
///
/// TODO (step 5): wrap `s` with `CStr::from_ptr`, take `.to_bytes()`, and feed
/// that to `Bloom::add`.
#[no_mangle]
pub unsafe extern "C" fn cbloom_add_str(bf: *mut Bloom, s: *const c_char) {
    let _ = (bf, s);
    todo!("ffi.rs: (&mut *bf).add(std::ffi::CStr::from_ptr(s).to_bytes())")
}

/// Query the `len` bytes at `data`. Returns `true` if probably present, `false`
/// if definitely absent — or on null input (a filter that holds nothing
/// contains nothing).
///
/// # Safety
/// Same contract as [`cbloom_add`]: `data` must be readable for `len` bytes.
///
/// TODO (step 5): the read-only mirror of `cbloom_add` — build `&*bf` and the
/// `&[u8]`, return `Bloom::contains`. Return `false` from the panic/null paths.
#[no_mangle]
pub unsafe extern "C" fn cbloom_contains(bf: *const Bloom, data: *const u8, len: usize) -> bool {
    let _ = (bf, data, len);
    todo!("ffi.rs: (&*bf).contains(std::slice::from_raw_parts(data, len)), false on null/panic")
}

/// Query a C string. `true` if probably present, `false` if definitely absent
/// or on null input.
///
/// # Safety
/// `s` must be null or a valid NUL-terminated C string.
///
/// TODO (step 5): like [`cbloom_add_str`] but calling `Bloom::contains`.
#[no_mangle]
pub unsafe extern "C" fn cbloom_contains_str(bf: *const Bloom, s: *const c_char) -> bool {
    let _ = (bf, s);
    todo!("ffi.rs: (&*bf).contains(std::ffi::CStr::from_ptr(s).to_bytes())")
}

/// Read the filter's parameters into a by-value [`CBloomStats`]. On null input
/// returns an all-zero struct.
///
/// TODO (step 6): null-check, then read `num_bits` / `num_hashes` /
/// `approx_items` off `&*bf` into a `CBloomStats`.
#[no_mangle]
pub unsafe extern "C" fn cbloom_get_stats(bf: *const Bloom) -> CBloomStats {
    let _ = bf;
    todo!("ffi.rs: fill CBloomStats from &*bf; zeroed struct on null")
}

/// Serialize the filter to a freshly allocated buffer owned by the caller.
///
/// The returned [`CBloomBuffer`] points at memory **Rust** allocated; the caller
/// must hand it back to [`cbloom_buffer_free`] (not C's `free`). Returns
/// `{ null, 0 }` on null input or failure (Pill 8 — the ownership-transfer
/// pattern).
///
/// TODO (step 6): call `Bloom::to_bytes`, then release the `Vec` to C without
/// freeing it. Use `Vec::into_boxed_slice` + `Box::into_raw` to get a `(ptr,
/// len)` whose allocation is exactly `len` (no separate capacity to track). See
/// the step-6 hint — this pairs precisely with `cbloom_buffer_free`.
#[no_mangle]
pub unsafe extern "C" fn cbloom_serialize(bf: *const Bloom) -> CBloomBuffer {
    let _ = bf;
    todo!("ffi.rs: take (&*bf).to_bytes(), leak it as a Box<[u8]> into a CBloomBuffer of data + len")
}

/// Free a buffer returned by [`cbloom_serialize`]. Null `data` is a no-op.
/// Passing a buffer not produced by `cbloom_serialize`, or freeing twice, is
/// undefined.
///
/// # Safety
/// `buf` must be a value previously returned by [`cbloom_serialize`] and not yet
/// freed.
///
/// TODO (step 6): reconstruct the exact `Box<[u8]>` you leaked —
/// `Box::from_raw(slice::from_raw_parts_mut(buf.data, buf.len))` — and drop it.
/// This is why `cbloom_serialize` used `into_boxed_slice`: ptr + len is enough
/// to rebuild the box.
#[no_mangle]
pub unsafe extern "C" fn cbloom_buffer_free(buf: CBloomBuffer) {
    let _ = (buf.data, buf.len);
    todo!("ffi.rs: if buf.data is non-null, rebuild the Box<[u8]> from buf.data + buf.len and drop it")
}

/// Deserialize a filter from `len` bytes at `data`. Returns a new opaque handle
/// (caller frees with [`cbloom_free`]), or null if the bytes are invalid or
/// `data` is null.
///
/// # Safety
/// `data` must point to at least `len` readable bytes (or be null).
///
/// TODO (step 6): borrow the input as a `&[u8]`, run it through
/// `Bloom::from_bytes`, and `Box::into_raw` the `Some` case; null on `None`.
#[no_mangle]
pub unsafe extern "C" fn cbloom_deserialize(data: *const u8, len: usize) -> *mut Bloom {
    let _ = (data, len);
    todo!("ffi.rs: Bloom::from_bytes(std::slice::from_raw_parts(data, len)) -> Box::into_raw or null")
}

#[cfg(test)]
mod tests {
    //! These tests drive the C ABI *in-process* — same `unsafe` calls a C
    //! program makes, but runnable under `cargo test` (and Miri). They are the
    //! fastest way to find a boundary bug before the C/Python consumers do.
    use super::*;

    #[test]
    fn handle_lifecycle_and_membership() {
        unsafe {
            let bf = cbloom_new(1000, 0.01);
            assert!(!bf.is_null());

            let key = b"hello world";
            cbloom_add(bf, key.as_ptr(), key.len());
            assert!(cbloom_contains(bf, key.as_ptr(), key.len()));

            let other = b"not added";
            assert!(!cbloom_contains(bf, other.as_ptr(), other.len()));

            cbloom_free(bf);
        }
    }

    #[test]
    fn null_inputs_are_safe() {
        unsafe {
            // Null handle must never crash and must read as "empty".
            assert!(!cbloom_contains(std::ptr::null(), b"x".as_ptr(), 1));
            cbloom_add(std::ptr::null_mut(), b"x".as_ptr(), 1); // no-op
            cbloom_free(std::ptr::null_mut()); // no-op, like free(NULL)
            let stats = cbloom_get_stats(std::ptr::null());
            assert_eq!(stats.num_bits, 0);
        }
    }

    #[test]
    fn c_string_helpers() {
        unsafe {
            let bf = cbloom_new(100, 0.01);
            let s = std::ffi::CString::new("rust").unwrap();
            cbloom_add_str(bf, s.as_ptr());
            assert!(cbloom_contains_str(bf, s.as_ptr()));
            cbloom_free(bf);
        }
    }

    #[test]
    fn serialize_roundtrip_across_the_boundary() {
        unsafe {
            let bf = cbloom_new(200, 0.01);
            for i in 0..200u32 {
                let k = i.to_le_bytes();
                cbloom_add(bf, k.as_ptr(), k.len());
            }
            let buf = cbloom_serialize(bf);
            assert!(!buf.data.is_null() && buf.len > 0);

            let restored = cbloom_deserialize(buf.data, buf.len);
            assert!(!restored.is_null());
            for i in 0..200u32 {
                let k = i.to_le_bytes();
                assert!(cbloom_contains(restored, k.as_ptr(), k.len()));
            }

            cbloom_buffer_free(buf);
            cbloom_free(restored);
            cbloom_free(bf);
        }
    }

    #[test]
    fn stats_report_params() {
        unsafe {
            let bf = cbloom_new(1000, 0.01);
            let s = cbloom_get_stats(bf);
            assert_eq!(s.num_hashes, 7);
            assert!(s.num_bits >= 9585);
            assert_eq!(s.approx_items, 0);
            cbloom_free(bf);
        }
    }
}
