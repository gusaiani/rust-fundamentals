//! `logcrunch` CLI.
//!
//! ```text
//! logcrunch <FILE> [--threads N] [--top K] [--mode seq|par|pipeline|rayon]
//! ```
//!
//! Defaults: threads = available parallelism, K = 10, mode = par.
//! No clap on purpose — hand-rolled arg parsing keeps the dependency list
//! honest and the concurrency the star. (clap is a fine stretch upgrade.)

use std::process::ExitCode;

#[allow(unused_imports)]
use logcrunch::parallel::analyze_parallel;
#[allow(unused_imports)]
use logcrunch::pipeline::analyze_pipeline;
#[allow(unused_imports)]
use logcrunch::rayon_impl::analyze_rayon;
#[allow(unused_imports)]
use logcrunch::sequential::analyze_sequential;

#[allow(dead_code)]
fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn main() -> ExitCode {
    // TODO (step 9):
    //   1. Collect `std::env::args()`. First positional = path (required;
    //      print usage + return `ExitCode::FAILURE` if missing).
    //   2. Parse flags: `--threads N`, `--top K`, `--mode <s>`. Unknown flag
    //      or bad value → usage error.
    //   3. `std::fs::read(path)` into a `Vec<u8>` (read once, here — the
    //      analyze fns take `&[u8]` so I/O stays out of measured work).
    //   4. Dispatch on mode:
    //        seq      => analyze_sequential(&data)
    //        par      => analyze_parallel(&data, threads)
    //        pipeline => analyze_pipeline(&data, threads)
    //        rayon    => analyze_rayon(&data)
    //   5. `let report = stats.into_report(top_k); println!("{report}");`
    //   6. Return `ExitCode::SUCCESS`.
    todo!("step 9: parse args, dispatch on --mode, print the report")
}
