//! `brc` — a One Billion Row Challenge solver, built as a performance lab.
//!
//! The workload is fixed and trivially simple (Pill 2): read a text file of
//! `station;temperature` lines and print min/mean/max per station, sorted. The
//! *point* is not the answer — it's taking a naive baseline and driving it 10x+
//! faster, proving every step with a benchmark (Pill 1). The modules map onto
//! the optimizations:
//!
//! - [`io`] gets the bytes in: [`io::map_file`] memory-maps the file so you parse
//!   straight out of the page cache (Pill 5), and [`io::split_chunks`] tiles the
//!   buffer into per-core ranges cut on newline boundaries (Pill 12).
//! - [`parse`] is **zero-copy** (Pill 6): the station name is a `&[u8]` borrow
//!   into the mmap, never a `String`; and [`parse::parse_temp`] reads the
//!   temperature as a fixed-point `i32` of tenths, with no floating point in the
//!   hot loop (Pill 7).
//! - [`aggregate`] holds the per-station accumulator [`aggregate::Stats`]
//!   (integer min/max/sum/count) and formats the final `{Name=min/mean/max, ...}`
//!   line.
//! - [`hash`] swaps std's DoS-resistant SipHash for a fast FxHash-style hasher
//!   (Pill 8) via the [`hash::FastMap`] alias — safe to do *here* because the
//!   keys are ~400 names from a file you control.
//! - [`runner`] ties it together: [`runner::run_sequential`] is the single-core
//!   path, [`runner::run_parallel`] fans the chunks across `thread::scope`
//!   workers and merges their maps (Pill 12).
//!
//! The deliverable is the *curve* (Pill 14): a table of wall-clock per version
//! showing the climb from 1x to 10x+, plus flamegraphs and an allocation profile.

pub mod aggregate;
pub mod hash;
pub mod io;
pub mod parse;
pub mod runner;

pub use aggregate::Stats;
pub use hash::FastMap;
