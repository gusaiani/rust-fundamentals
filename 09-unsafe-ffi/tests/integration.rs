//! End-to-end tests over the public Rust API and the `extern "C"` boundary.
//!
//! The unit tests in each module cover pieces; these exercise the whole thing
//! the way a real consumer does, including the FFI surface in-process. They
//! `panic` (`not yet implemented`) until the stubs in `bloom.rs`, `hash.rs`, and
//! `ffi.rs` are filled in — that's the signal you're done. Run under
//! `cargo +nightly miri test` once green to check the `unsafe` for UB.

use cbloom::ffi::*;
use cbloom::Bloom;

#[test]
fn safe_api_holds_the_bloom_guarantee() {
    let mut b = Bloom::new(10_000, 0.001);
    let present: Vec<String> = (0..10_000).map(|i| format!("user:{i}")).collect();
    for p in &present {
        b.add(p.as_bytes());
    }
    // No false negatives, ever.
    for p in &present {
        assert!(b.contains(p.as_bytes()));
    }
    // False positives stay near the configured 0.1%.
    let fps = (0..10_000)
        .filter(|i| b.contains(format!("ghost:{i}").as_bytes()))
        .count();
    assert!(fps < 100, "fp rate {fps}/10000 exceeds ~1%");
}

#[test]
fn ffi_matches_safe_api() {
    // The C entry points must behave identically to the safe core they wrap.
    unsafe {
        let bf = cbloom_new(1000, 0.01);
        assert!(!bf.is_null());

        let mut reference = Bloom::new(1000, 0.01);
        for i in 0..1000u32 {
            let k = format!("key-{i}");
            cbloom_add(bf, k.as_ptr(), k.len());
            reference.add(k.as_bytes());
        }

        for i in 0..1000u32 {
            let k = format!("key-{i}");
            assert_eq!(
                cbloom_contains(bf, k.as_ptr(), k.len()),
                reference.contains(k.as_bytes()),
            );
        }

        let stats = cbloom_get_stats(bf);
        assert_eq!(stats.num_bits, reference.num_bits());
        assert_eq!(stats.num_hashes, reference.num_hashes());
        assert_eq!(stats.approx_items, 1000);

        cbloom_free(bf);
    }
}

#[test]
fn ffi_serialize_survives_a_full_roundtrip() {
    unsafe {
        let bf = cbloom_new(2000, 0.005);
        for i in 0..2000u32 {
            let k = i.to_le_bytes();
            cbloom_add(bf, k.as_ptr(), k.len());
        }

        // Rust serialize -> Rust deserialize, but through the C buffer dance:
        // the buffer is Rust-owned memory we must return to cbloom_buffer_free.
        let buf = cbloom_serialize(bf);
        assert!(!buf.data.is_null() && buf.len > 0);

        let restored = cbloom_deserialize(buf.data, buf.len);
        assert!(!restored.is_null());
        for i in 0..2000u32 {
            let k = i.to_le_bytes();
            assert!(cbloom_contains(restored, k.as_ptr(), k.len()));
        }

        // A serialized buffer also feeds the safe `from_bytes`: one wire format.
        let bytes = std::slice::from_raw_parts(buf.data, buf.len);
        assert!(Bloom::from_bytes(bytes).is_some());

        cbloom_buffer_free(buf);
        cbloom_free(restored);
        cbloom_free(bf);
    }
}

#[test]
fn deserialize_rejects_garbage_without_crashing() {
    unsafe {
        let junk = b"this is not a cbloom buffer";
        assert!(cbloom_deserialize(junk.as_ptr(), junk.len()).is_null());
        assert!(cbloom_deserialize(std::ptr::null(), 0).is_null());
    }
}
