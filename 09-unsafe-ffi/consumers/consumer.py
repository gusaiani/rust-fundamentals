#!/usr/bin/env python3
"""A Python program that uses libcbloom through its C ABI via ctypes.

Same library, same symbols as the C consumer — but loaded dynamically at
runtime with no compile step and no header. This is the payoff of shipping a C
ABI: every language with an FFI (Python, Ruby, Node, Go, ...) can call your Rust
for free. The one rule ctypes makes you obey: declare each function's argtypes
and restype, or it guesses `int` and silently corrupts pointers on 64-bit.

Run via `consumers/run.sh` (it builds the cdylib first), or point CBLOOM_LIB at
the built library and run this directly. It works once the Rust stubs in
src/ffi.rs are implemented; before that the calls panic and abort the process.
"""
import ctypes
import os
import sys
import platform


def library_path() -> str:
    """Locate the built cdylib (target/release), honoring CBLOOM_LIB override."""
    if "CBLOOM_LIB" in os.environ:
        return os.environ["CBLOOM_LIB"]
    system = platform.system()
    name = {
        "Darwin": "libcbloom.dylib",
        "Linux": "libcbloom.so",
        "Windows": "cbloom.dll",
    }.get(system, "libcbloom.so")
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(here, "..", "target", "release", name)


# --- repr(C) structs, mirrored as ctypes.Structure (Pill 4) ----------------
class CBloomStats(ctypes.Structure):
    _fields_ = [
        ("num_bits", ctypes.c_size_t),
        ("num_hashes", ctypes.c_uint32),
        ("approx_items", ctypes.c_uint64),
    ]


class CBloomBuffer(ctypes.Structure):
    _fields_ = [
        ("data", ctypes.POINTER(ctypes.c_uint8)),
        ("len", ctypes.c_size_t),
    ]


def declare(lib: ctypes.CDLL) -> None:
    """Pin every signature. An opaque `cbloom *` is a void pointer to Python."""
    p = ctypes.c_void_p
    u8p = ctypes.POINTER(ctypes.c_uint8)
    size = ctypes.c_size_t

    lib.cbloom_new.restype = p
    lib.cbloom_new.argtypes = [size, ctypes.c_double]
    lib.cbloom_free.argtypes = [p]
    lib.cbloom_add.argtypes = [p, u8p, size]
    lib.cbloom_add_str.argtypes = [p, ctypes.c_char_p]
    lib.cbloom_contains.restype = ctypes.c_bool
    lib.cbloom_contains.argtypes = [p, u8p, size]
    lib.cbloom_contains_str.restype = ctypes.c_bool
    lib.cbloom_contains_str.argtypes = [p, ctypes.c_char_p]
    lib.cbloom_get_stats.restype = CBloomStats
    lib.cbloom_get_stats.argtypes = [p]
    lib.cbloom_serialize.restype = CBloomBuffer
    lib.cbloom_serialize.argtypes = [p]
    lib.cbloom_buffer_free.argtypes = [CBloomBuffer]
    lib.cbloom_deserialize.restype = p
    lib.cbloom_deserialize.argtypes = [u8p, size]


def as_bytes(data: bytes):
    """A (uint8_t*, size_t) view of a Python bytes object."""
    buf = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
    return buf, len(data)


def main() -> int:
    path = library_path()
    if not os.path.exists(path):
        print(f"library not found: {path}\nrun `cargo build --release` first", file=sys.stderr)
        return 1

    lib = ctypes.CDLL(path)
    declare(lib)
    print("cbloom Python consumer")

    ok = True

    def check(label: str, cond: bool) -> None:
        nonlocal ok
        ok = ok and cond
        print(f"  [{'ok' if cond else 'FAIL'}] {label}")

    bf = lib.cbloom_new(1000, 0.01)
    check("cbloom_new", bool(bf))

    words = [b"alpha", b"bravo", b"charlie", b"delta", b"echo"]
    for w in words:
        lib.cbloom_add_str(bf, w)
    for w in words:
        check(w.decode(), lib.cbloom_contains_str(bf, w))
    check("foxtrot absent", not lib.cbloom_contains_str(bf, b"foxtrot"))

    # Raw byte key.
    buf, n = as_bytes(bytes([0, 1, 2, 255]))
    lib.cbloom_add(bf, buf, n)
    check("byte key present", lib.cbloom_contains(bf, buf, n))

    stats = lib.cbloom_get_stats(bf)
    print(f"  stats: num_bits={stats.num_bits} num_hashes={stats.num_hashes} "
          f"approx_items={stats.approx_items}")
    check("stats item count", stats.approx_items == len(words) + 1)

    # Serialize -> deserialize. The buffer is Rust-owned; hand it back to
    # cbloom_buffer_free, never to a Python/C free.
    sbuf = lib.cbloom_serialize(bf)
    check("serialize produced bytes", bool(sbuf.data) and sbuf.len > 0)
    restored = lib.cbloom_deserialize(sbuf.data, sbuf.len)
    check("deserialize", bool(restored))
    check("restored has alpha", lib.cbloom_contains_str(restored, b"alpha"))
    lib.cbloom_buffer_free(sbuf)

    lib.cbloom_free(restored)
    lib.cbloom_free(bf)

    print("done." if ok else "FAILURES above.")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
