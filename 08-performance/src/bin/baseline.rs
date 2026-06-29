//! v0 - the deliberately naive 1BRC baseline (Pill 3). slow on purpose.
//! Self-contained: uses no crate internals, so it stays "the obvious Rust."
//! This is the number every optimized version divides into.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::time::Instant;

fn main() {
    // Last CLI arg is the path; default to measurements.txt.
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "measurements.txt".into());

    let file = std::fs::File::open(&path).expect("open measurements file");

    // (min, max, sum, count) per station - all f64, default SipHash HashMap.

    let mut map: HashMap<String, (f64, f64, f64, u64)> = HashMap::new();

    let start = Instant::now();

    for line in BufReader::new(file).lines() {
        let line = line.expect("read line"); // allocates a String per row
        let (name, temp) = line.split_once(';').unwrap(); // borrows from `line`
        let temp: f64 = temp.parse().unwrap(); // general-purpose float parse
        let e = map
            .entry(name.to_string()) // allocates a String per row
            .or_insert((f64::MAX, f64::MIN, 0.0, 0));
        e.0 = e.0.min(temp);
        e.1 = e.1.max(temp);
        e.2 += temp;
        e.3 += 1;
    }

    // Sort alphabetically (the spec's output order) by collecting into a BTreeMap.
    let sorted: BTreeMap<String, (f64, f64, f64, u64)> = map.into_iter().collect();

    // Build "Name=min/mean/max" for each station , then wrap in `{...}`.
    let mut parts: Vec<String> = Vec::new();
    for (name, &(min, max, sum, count)) in &sorted {
        // &(...) copies the Copy fields (f64/u64) out by value - no refs to juggle
        let mean = sum / count as f64;
        parts.push(format!("{name}={min:.1}/{mean:.1}/{max:.1}"));
    }
    println!("{{{}}}", parts.join(", ")); // {{ and }} are literal braces

    eprintln!("v0 (naive baseline): {:?}", start.elapsed());
}
