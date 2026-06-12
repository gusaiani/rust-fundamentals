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

use logcrunch::parallel::analyze_parallel;
use logcrunch::pipeline::analyze_pipeline;
use logcrunch::rayon_impl::analyze_rayon;
use logcrunch::sequential::analyze_sequential;

fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let path = match args.iter().find(|a| !a.starts_with('-')) {
        Some(p) => p.clone(),
        None => {
            eprintln!(
                "usage: logcrunch <FILE> [--threads N] [--top K] [--mode seq|par|pipeline|rayon]"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut threads = default_threads();

    if let Some(i) = args.iter().position(|a| a == "--threads") {
        match args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
            Some(n) if n > 0 => threads = n,
            _ => {
                eprintln!("--threads needs a positive integer");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut top_k = 10;

    if let Some(i) = args.iter().position(|a| a == "--top") {
        match args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
            Some(n) if n > 0 => top_k = n,
            _ => {
                eprintln!("--top needs a positive integer");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut mode = "par".to_string();

    if let Some(i) = args.iter().position(|a| a == "--mode") {
        match args.get(i + 1).map(|v| v.as_str()) {
            Some(m @ ("seq" | "par" | "pipeline" | "rayon")) => mode = m.to_string(),
            _ => {
                eprintln!("--mode must be one of: seq, par, pipeline, rayon");
                return ExitCode::FAILURE;
            }
        }
    }

    let data = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let stats = match mode.as_str() {
        "seq" => analyze_sequential(&data),
        "par" => analyze_parallel(&data, threads),
        "pipeline" => analyze_pipeline(&data, threads),
        "rayon" => analyze_rayon(&data),
        _ => unreachable!("mode validated during arg parsing"),
    };

    let report = stats.into_report(top_k);
    print!("{report}");

    ExitCode::SUCCESS
}
