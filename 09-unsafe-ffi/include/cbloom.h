/* cbloom — a Bloom filter with a C ABI.
 *
 * This header is the C-side contract for the `extern "C"` surface in
 * `src/ffi.rs`. Keep the two in lockstep: every function and struct here has an
 * exact counterpart there. In a real crate you'd generate this file from the
 * Rust source with `cbindgen` (see the README); it's hand-written here so it
 * reads as documentation of the boundary.
 *
 * Ownership rules (the whole game — Pills 5–8):
 *   - A `cbloom *` comes only from cbloom_new / cbloom_deserialize and must be
 *     released exactly once with cbloom_free. Using it after free, or freeing
 *     twice, is undefined behavior.
 *   - A cbloom_buffer from cbloom_serialize owns Rust-allocated memory: release
 *     it with cbloom_buffer_free, never with free().
 *   - Byte ranges you pass in (data, len) and C strings are only borrowed for
 *     the duration of the call; cbloom never takes ownership of them.
 *   - Every function tolerates NULL pointers without crashing.
 */
#ifndef CBLOOM_H
#define CBLOOM_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle. C never sees the layout of a filter — only this pointer. */
typedef struct cbloom cbloom;

/* Filter parameters, returned by value (repr(C) `CBloomStats`). */
typedef struct {
    size_t   num_bits;     /* m: size of the bit array            */
    uint32_t num_hashes;   /* k: probes per item                  */
    uint64_t approx_items; /* number of add() calls so far        */
} cbloom_stats;

/* An owned byte buffer handed back from cbloom_serialize. `data` is Rust-
 * allocated; free the pair with cbloom_buffer_free. {NULL, 0} means failure. */
typedef struct {
    uint8_t *data;
    size_t   len;
} cbloom_buffer;

/* ---- lifecycle ---------------------------------------------------------- */

/* Create a filter sized for `expected_items` insertions at false-positive
 * probability `fp_rate` (e.g. 0.01). Returns NULL on failure. */
cbloom *cbloom_new(size_t expected_items, double fp_rate);

/* Destroy a filter. NULL is a no-op. */
void cbloom_free(cbloom *bf);

/* ---- membership --------------------------------------------------------- */

/* Add `len` bytes at `data`. NULL filter or data is a no-op. */
void cbloom_add(cbloom *bf, const uint8_t *data, size_t len);

/* Add a NUL-terminated string (its bytes, excluding the NUL). */
void cbloom_add_str(cbloom *bf, const char *s);

/* Test membership. true = probably present, false = definitely absent. */
bool cbloom_contains(const cbloom *bf, const uint8_t *data, size_t len);

/* Test a NUL-terminated string. */
bool cbloom_contains_str(const cbloom *bf, const char *s);

/* ---- introspection ------------------------------------------------------ */

/* Read filter parameters. Returns a zeroed struct for a NULL filter. */
cbloom_stats cbloom_get_stats(const cbloom *bf);

/* ---- serialization ------------------------------------------------------ */

/* Serialize to a newly allocated buffer the caller owns. Free it with
 * cbloom_buffer_free (NOT free()). Returns {NULL, 0} on failure. */
cbloom_buffer cbloom_serialize(const cbloom *bf);

/* Free a buffer from cbloom_serialize. A {NULL, 0} buffer is a no-op. */
void cbloom_buffer_free(cbloom_buffer buf);

/* Rebuild a filter from cbloom_serialize bytes. Returns NULL if the bytes are
 * invalid. The result is a fresh handle; free it with cbloom_free. */
cbloom *cbloom_deserialize(const uint8_t *data, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* CBLOOM_H */
