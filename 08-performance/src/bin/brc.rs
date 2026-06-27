//! `brc` — the solver CLI.
//!
//! Usage:
//! ```bash
//! brc measurements.txt              # parallel over all cores (default)
//! brc measurements.txt --threads 1 # force the single-core path
//! ```
//!
//! Arg parsing and the wall-clock timing are scaffolded. The middle — actually
//! running the aggregation and printing — is yours (step 5). The elapsed time it
//! prints to stderr is your Pill 1 whole-file number; record it per version to
//! build the speedup table (Pill 14).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use brc::io::map_file;
use brc::runner::{run_parallel, run_sequential};

fn main() -> ExitCode {
    // --- arg parsing (given) ---
    let mut path: Option<PathBuf> = None;
    let mut threads: Option<usize> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--threads" | "-t" => {
                threads = args.next().and_then(|s| s.parse().ok());
                if threads.is_none() {
                    eprintln!("--threads needs a number");
                    return ExitCode::FAILURE;
                }
            }
            _ => path = Some(PathBuf::from(arg)),
        }
    }
    let Some(path) = path else {
        eprintln!("usage: brc <measurements.txt> [--threads N]");
        return ExitCode::FAILURE;
    };

    // Default to one worker per available core.
    let threads = threads.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    });

    // --- the run (TODO, step 5) ---
    let mmap = match map_file(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("could not open {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let data: &[u8] = &mmap;

    let start = Instant::now();

    // TODO (step 5): pick the path by `threads` (1 -> run_sequential, else
    // run_parallel), then `format_results` the result and print it to stdout.
    // Keep the call inside the timed region; keep printing/formatting in it too
    // if you want the output cost included, or stop the clock first if you only
    // want the compute. Be explicit about which you're measuring (Pill 1).
    let _ = (data, threads, run_sequential as fn(&[u8]) -> _, run_parallel as fn(&[u8], usize) -> _);
    todo!("run the aggregation, format, and print — see step 5");

    #[allow(unreachable_code)]
    {
        let elapsed = start.elapsed();
        eprintln!("{} rows in {:?} ({} threads)", data.len(), elapsed, threads);
        ExitCode::SUCCESS
    }
}
