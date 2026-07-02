/* FNV-1a 64-bit — see fnv.h. Public-domain algorithm by Fowler/Noll/Vo. */
#include "fnv.h"

uint64_t cbloom_fnv1a64(const uint8_t *data, size_t len) {
    uint64_t hash = 0xcbf29ce484222325ULL; /* offset basis */
    for (size_t i = 0; i < len; i++) {
        hash ^= (uint64_t)data[i];
        hash *= 0x00000100000001b3ULL; /* FNV prime */
    }
    return hash;
}
