//! Build script. Does nothing for a default build — the core crate has no C
//! dependencies. With `--features cffi` it compiles the bundled `cbits/fnv.c`
//! and runs `bindgen` over `cbits/fnv.h` to generate Rust bindings into
//! `OUT_DIR`, which `src/sys.rs` then `include!`s (Pill 11). That path needs a C
//! compiler and libclang; the default path needs neither.

fn main() {
    #[cfg(feature = "cffi")]
    cffi::build();
}

#[cfg(feature = "cffi")]
mod cffi {
    use std::env;
    use std::path::PathBuf;

    pub fn build() {
        // 1. Compile the C source into a static lib linked into the crate. The
        //    `cc` crate finds the platform toolchain (clang/gcc/MSVC) for us.
        cc::Build::new()
            .file("cbits/fnv.c")
            .include("cbits")
            .compile("cfnv");

        // 2. Generate Rust bindings for the C header. In real life this is the
        //    `bindgen` lesson: point it at a header, allowlist what you want,
        //    and it emits the `extern "C"` block you'd otherwise hand-write.
        let bindings = bindgen::Builder::default()
            .header("cbits/fnv.h")
            .allowlist_function("cbloom_fnv1a64")
            .generate()
            .expect("bindgen failed — is libclang installed?");

        let out = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out.join("fnv_bindings.rs"))
            .expect("could not write bindings");

        println!("cargo:rerun-if-changed=cbits/fnv.c");
        println!("cargo:rerun-if-changed=cbits/fnv.h");
    }
}
