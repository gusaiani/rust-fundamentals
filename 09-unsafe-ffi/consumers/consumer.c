/* A C program that uses libcbloom through its C ABI.
 *
 * This is the point of the whole module: a real C consumer that never sees a
 * line of Rust, only `cbloom.h` and the linked library. It builds a filter,
 * adds some words, queries hits and misses, prints stats, and round-trips the
 * filter through serialize/deserialize — exercising every ownership rule in the
 * header.
 *
 * Build & run via `consumers/run.sh` (it links against target/release). It will
 * link and run only once the Rust `extern "C"` stubs in src/ffi.rs are done;
 * before that the functions exist but panic, which aborts the process. */

#include "cbloom.h"
#include <stdio.h>
#include <string.h>

static void expect(const char *label, int cond) {
    printf("  [%s] %s\n", cond ? "ok" : "FAIL", label);
    if (!cond) {
        fprintf(stderr, "assertion failed: %s\n", label);
    }
}

int main(void) {
    printf("cbloom C consumer\n");

    /* 1. Create an opaque handle. */
    cbloom *bf = cbloom_new(1000, 0.01);
    if (!bf) {
        fprintf(stderr, "cbloom_new returned NULL\n");
        return 1;
    }

    /* 2. Add a few strings. */
    const char *words[] = {"alpha", "bravo", "charlie", "delta", "echo"};
    const size_t n = sizeof(words) / sizeof(words[0]);
    for (size_t i = 0; i < n; i++) {
        cbloom_add_str(bf, words[i]);
    }

    /* 3. Every inserted word must report present (no false negatives). */
    for (size_t i = 0; i < n; i++) {
        expect(words[i], cbloom_contains_str(bf, words[i]));
    }

    /* 4. A word we never added should (almost certainly) report absent. */
    expect("foxtrot absent", !cbloom_contains_str(bf, "foxtrot"));

    /* 5. Raw byte API: add and find a non-string key. */
    uint8_t key[] = {0x00, 0x01, 0x02, 0xff};
    cbloom_add(bf, key, sizeof(key));
    expect("byte key present", cbloom_contains(bf, key, sizeof(key)));

    /* 6. Stats come back by value. */
    cbloom_stats st = cbloom_get_stats(bf);
    printf("  stats: num_bits=%zu num_hashes=%u approx_items=%llu\n",
           st.num_bits, st.num_hashes, (unsigned long long)st.approx_items);
    expect("stats item count", st.approx_items == n + 1);

    /* 7. Serialize -> deserialize. The buffer is Rust-owned: free it with
     *    cbloom_buffer_free, never free(). */
    cbloom_buffer buf = cbloom_serialize(bf);
    expect("serialize produced bytes", buf.data != NULL && buf.len > 0);

    cbloom *restored = cbloom_deserialize(buf.data, buf.len);
    expect("deserialize succeeded", restored != NULL);
    expect("restored has alpha", cbloom_contains_str(restored, "alpha"));
    expect("restored has byte key", cbloom_contains(restored, key, sizeof(key)));

    cbloom_buffer_free(buf);

    /* 8. Clean up every handle. */
    cbloom_free(restored);
    cbloom_free(bf);

    printf("done.\n");
    return 0;
}
