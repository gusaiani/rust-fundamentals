#!/usr/bin/env node
//
// A Node.js program that uses libcbloom through its C ABI via koffi.
//
// Same library, same symbols as the C and Python consumers — but loaded
// dynamically at runtime with no compile step and no header. This is the payoff
// of shipping a C ABI: every language with an FFI (C, Python, Node, Ruby, Go,
// ...) calls the identical .dylib for free. koffi is the actively-maintained
// pure-FFI library (the successor to ffi-napi); it dlopen's the library and,
// crucially, marshals #[repr(C)] structs by value — which cbloom_get_stats and
// cbloom_serialize need.
//
// Install the one dependency, then run via consumers/run.sh (it builds the
// cdylib first), or point CBLOOM_LIB at the built library and run directly:
//
//     npm install koffi
//     CBLOOM_LIB=target/release/libcbloom.dylib node consumers/consumer.js
//
// It works once the extern "C" functions in src/ffi.rs are implemented; before
// that the calls panic and are caught at the boundary (returning null/false).

const path = require("path");
const fs = require("fs");
const koffi = require("koffi");

// Locate the built cdylib (target/release), honoring a CBLOOM_LIB override.
function libraryPath() {
  if (process.env.CBLOOM_LIB) {
    return process.env.CBLOOM_LIB;
  }
  const name = {
    darwin: "libcbloom.dylib",
    linux: "libcbloom.so",
    win32: "cbloom.dll",
  }[process.platform] || "libcbloom.so";
  return path.join(__dirname, "..", "target", "release", name);
}

const libPath = libraryPath();
if (!fs.existsSync(libPath)) {
  console.error(`library not found: ${libPath}\nrun \`cargo build --release\` first`);
  process.exit(1);
}

const lib = koffi.load(libPath);

// --- repr(C) structs, mirrored as koffi structs (Pill 4) -------------------
// Registered by name so the prototype strings below can refer to them.
koffi.struct("CBloomStats", {
  num_bits: "size_t",
  num_hashes: "uint32_t",
  approx_items: "uint64_t",
});
koffi.struct("CBloomBuffer", {
  data: "uint8_t *",
  len: "size_t",
});

// The opaque `cbloom *` handle is just a void pointer to us — we never look
// inside it, exactly like the Python consumer's c_void_p.
const cbloom_new = lib.func("void *cbloom_new(size_t expected_items, double fp_rate)");
const cbloom_free = lib.func("void cbloom_free(void *bf)");
const cbloom_add = lib.func("void cbloom_add(void *bf, uint8_t *data, size_t len)");
const cbloom_add_str = lib.func("void cbloom_add_str(void *bf, const char *s)");
const cbloom_contains = lib.func("bool cbloom_contains(void *bf, uint8_t *data, size_t len)");
const cbloom_contains_str = lib.func("bool cbloom_contains_str(void *bf, const char *s)");
const cbloom_get_stats = lib.func("CBloomStats cbloom_get_stats(void *bf)");
const cbloom_serialize = lib.func("CBloomBuffer cbloom_serialize(void *bf)");
const cbloom_buffer_free = lib.func("void cbloom_buffer_free(CBloomBuffer buf)");
const cbloom_deserialize = lib.func("void *cbloom_deserialize(uint8_t *data, size_t len)");

function main() {
  console.log("cbloom Node.js consumer");

  let ok = true;
  const check = (label, cond) => {
    ok = ok && cond;
    console.log(`  [${cond ? "ok" : "FAIL"}] ${label}`);
  };

  const bf = cbloom_new(1000, 0.01);
  check("cbloom_new", Boolean(bf));

  const words = ["alpha", "bravo", "charlie", "delta", "echo"];
  for (const w of words) {
    cbloom_add_str(bf, w);
  }
  for (const w of words) {
    check(w, cbloom_contains_str(bf, w));
  }
  check("foxtrot absent", !cbloom_contains_str(bf, "foxtrot"));

  // Raw byte key: a Node Buffer maps straight onto (uint8_t *, size_t).
  const key = Buffer.from([0, 1, 2, 255]);
  cbloom_add(bf, key, key.length);
  check("byte key present", cbloom_contains(bf, key, key.length));

  const stats = cbloom_get_stats(bf);
  console.log(
    `  stats: num_bits=${stats.num_bits} num_hashes=${stats.num_hashes} ` +
    `approx_items=${stats.approx_items}`,
  );
  check("stats item count", stats.approx_items === words.length + 1);

  // Serialize -> deserialize. The buffer is Rust-owned; hand it back to
  // cbloom_buffer_free, never to a JS/C free (Pill 8, ownership transfer).
  const sbuf = cbloom_serialize(bf);
  check("serialize produced bytes", Boolean(sbuf.data) && sbuf.len > 0);
  const restored = cbloom_deserialize(sbuf.data, sbuf.len);
  check("deserialize", Boolean(restored));
  check("restored has alpha", cbloom_contains_str(restored, "alpha"));
  cbloom_buffer_free(sbuf);

  cbloom_free(restored);
  cbloom_free(bf);

  console.log(ok ? "done." : "FAILURES above.");
  process.exit(ok ? 0 : 1);
}

main();
