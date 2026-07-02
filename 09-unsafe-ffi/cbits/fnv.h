/* A canonical FNV-1a 64-bit hash, in C. Bound from Rust via `bindgen` when the
 * crate is built with `--features cffi` (Pill 11) — the "calling C from Rust"
 * direction. The Rust core reimplements the same hash in `src/hash.rs`; the
 * `cffi` test asserts the two agree byte-for-byte. */
#ifndef CBLOOM_FNV_H
#define CBLOOM_FNV_H

#include <stddef.h>
#include <stdint.h>

/* FNV-1a 64-bit hash of `len` bytes at `data`. */
uint64_t cbloom_fnv1a64(const uint8_t *data, size_t len);

#endif /* CBLOOM_FNV_H */
