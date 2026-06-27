//! `gen` — the measurements generator. **Given** (you don't implement this).
//!
//! Writes a valid 1BRC `measurements.txt`: `count` lines of `station;temp`,
//! where each station has a fixed mean and the temperature is that mean plus
//! Gaussian-ish noise, rounded to one decimal and clamped to [-99.9, 99.9].
//!
//! It is **deterministic** (a fixed-seed xorshift PRNG, no `rand` crate) so your
//! benchmark runs over identical bytes every time — a precondition for Pill 1's
//! measure-change-measure loop.
//!
//! ```bash
//! cargo run --release --bin gen -- 1000000 measurements.txt    # 1M rows
//! cargo run --release --bin gen -- 1000000000 measurements.txt # the full ~13 GB
//! ```

use std::env;
use std::io::{self, BufWriter, Write};

/// A station and its mean temperature in °C. A representative slice of the
/// official 1BRC station list — enough distinct keys to make hashing matter.
const STATIONS: &[(&str, f64)] = &[
    ("Abha", 18.0), ("Abidjan", 26.0), ("Accra", 26.4), ("Addis Ababa", 16.0),
    ("Adelaide", 17.3), ("Algiers", 18.2), ("Amsterdam", 10.2), ("Anchorage", 2.8),
    ("Ankara", 12.0), ("Athens", 19.2), ("Auckland", 15.2), ("Baghdad", 22.8),
    ("Bangkok", 28.6), ("Barcelona", 18.2), ("Beijing", 12.9), ("Berlin", 10.3),
    ("Bogotá", 15.4), ("Boston", 10.9), ("Brussels", 10.5), ("Bucharest", 10.8),
    ("Budapest", 11.3), ("Buenos Aires", 17.3), ("Bulawayo", 18.9), ("Cairo", 21.4),
    ("Calgary", 4.4), ("Cape Town", 16.2), ("Caracas", 27.5), ("Chicago", 9.8),
    ("Copenhagen", 9.1), ("Dakar", 24.0), ("Dallas", 19.0), ("Dar es Salaam", 25.8),
    ("Delhi", 25.0), ("Denver", 10.4), ("Dhaka", 25.9), ("Dubai", 26.9),
    ("Dublin", 9.8), ("Hamburg", 9.7), ("Hanoi", 23.6), ("Havana", 25.2),
    ("Helsinki", 5.9), ("Ho Chi Minh City", 27.4), ("Hong Kong", 23.3),
    ("Istanbul", 13.9), ("Jakarta", 26.7), ("Johannesburg", 15.5), ("Karachi", 26.0),
    ("Kolkata", 26.7), ("Lagos", 26.8), ("Lima", 18.2), ("Lisbon", 17.5),
    ("London", 11.3), ("Los Angeles", 18.6), ("Madrid", 15.0), ("Manila", 28.4),
    ("Mexico City", 17.5), ("Miami", 24.9), ("Montreal", 6.8), ("Moscow", 5.8),
    ("Mumbai", 27.1), ("Nairobi", 17.8), ("New York City", 12.9), ("Oslo", 5.7),
    ("Palembang", 27.3), ("Paris", 12.3), ("Perth", 18.7), ("Reykjavík", 4.3),
    ("Rome", 15.2), ("Saint Petersburg", 5.8), ("Santiago", 14.0), ("São Paulo", 19.0),
    ("Seoul", 12.5), ("Shanghai", 16.7), ("Singapore", 27.0), ("St. John's", 5.0),
    ("Stockholm", 6.6), ("Sydney", 17.7), ("Tokyo", 15.4), ("Toronto", 9.4),
    ("Vancouver", 10.4), ("Vienna", 10.4), ("Warsaw", 8.5), ("Zürich", 9.3),
];

/// A tiny xorshift64* PRNG — deterministic, fast, no dependency. Good enough to
/// shape test data; not for anything that needs real randomness.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A float in [0, 1).
    fn next_f64(&mut self) -> f64 {
        // Top 53 bits give a uniform double in [0, 1).
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Approximately-normal noise via the central-limit trick (sum of uniforms),
    /// scaled to roughly ±`spread` degrees.
    fn next_noise(&mut self, spread: f64) -> f64 {
        let s: f64 = (0..6).map(|_| self.next_f64()).sum::<f64>() - 3.0; // mean 0, ~unit-ish
        s * spread
    }
}

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let count: u64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let path = args.next().unwrap_or_else(|| "measurements.txt".to_string());

    let file = std::fs::File::create(&path)?;
    let mut out = BufWriter::with_capacity(1 << 20, file);

    let mut rng = Rng(0x1234_5678_9abc_def0); // fixed seed -> reproducible file
    let n = STATIONS.len() as u64;

    for _ in 0..count {
        let (name, mean) = STATIONS[(rng.next_u64() % n) as usize];
        let mut temp = mean + rng.next_noise(7.0);
        temp = temp.clamp(-99.9, 99.9);
        // Round to one decimal — the exact precision the format (and parser) expects.
        let tenths = (temp * 10.0).round() as i64;
        // Build sign explicitly: integer division loses the minus when the whole
        // part is zero (e.g. -0.5 has tenths/10 == 0), which would emit "0.5".
        let sign = if tenths < 0 { "-" } else { "" };
        let mag = tenths.abs();
        writeln!(out, "{name};{sign}{}.{}", mag / 10, mag % 10)?;
    }

    out.flush()?;
    eprintln!("wrote {count} rows to {path}");
    Ok(())
}
