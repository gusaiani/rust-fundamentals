#!/usr/bin/env bash
# Build libcbloom and run both consumers against it: the C program (compiled and
# dynamically linked) and the Python program (loaded with ctypes). Run from the
# crate root: `./consumers/run.sh`.
#
# Until the `extern "C"` stubs in src/ffi.rs are implemented, the library builds
# fine but the functions panic when called, so the consumers will abort. That's
# the expected pre-implementation state.
set -euo pipefail

cd "$(dirname "$0")/.."   # crate root

echo "==> building libcbloom (release cdylib + staticlib)"
cargo build --release

REL="target/release"
case "$(uname -s)" in
  Darwin)  LIB="$REL/libcbloom.dylib"; export DYLD_LIBRARY_PATH="$REL:${DYLD_LIBRARY_PATH:-}" ;;
  Linux)   LIB="$REL/libcbloom.so";    export LD_LIBRARY_PATH="$REL:${LD_LIBRARY_PATH:-}" ;;
  *)       echo "unsupported OS: $(uname -s)"; exit 1 ;;
esac

echo "==> compiling consumers/consumer.c against the C ABI"
cc consumers/consumer.c -Iinclude -L"$REL" -lcbloom -o consumers/consumer

echo "==> running the C consumer"
./consumers/consumer

echo
echo "==> running the Python consumer (ctypes)"
CBLOOM_LIB="$LIB" python3 consumers/consumer.py
