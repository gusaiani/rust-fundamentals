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

use brc::aggregate::format_results;
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
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
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

    let result = if threads <= 1 {
        run_sequential(data)
    } else {
        run_parallel(data, threads)
    };

    let output = format_results(&result);
    let elapsed = start.elapsed();

    println!("{output}");
    eprintln!(
        "{} bytes in {:?} ({} threads)",
        data.len(),
        elapsed,
        threads
    );
    ExitCode::SUCCESS
}
