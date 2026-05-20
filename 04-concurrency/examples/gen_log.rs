//! Synthetic-log generator — make a big, valid input to chunk and bench.
//!
//! ```text
//! cargo run --release --example gen_log -- 2000000 > big.log
//! ```
//!
//! Emits N lines in the `logcrunch` format. No `rand` dependency: a tiny
//! xorshift PRNG is plenty for a load fixture (and keeps deps minimal —
//! Module 3's Pill 14 lesson carries over).

use std::io::{self, Write};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next() as usize) % xs.len()]
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let ips = ["10.0.0.5", "10.0.0.6", "192.168.1.9", "172.16.0.2", "10.0.0.7"];
    let statuses = [200u16, 200, 200, 200, 301, 404, 500, 503];
    let methods = ["GET", "GET", "GET", "POST", "PUT", "DELETE"];
    let paths = [
        "/", "/api/users", "/api/checkout", "/favicon.ico",
        "/static/app.js", "/health", "/api/search", "/login",
    ];

    let mut rng = XorShift(0x9E3779B97F4A7C15);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for _ in 0..n {
        let ip = rng.pick(&ips);
        let status = *rng.pick(&statuses);
        let bytes = rng.next() % 50_000;
        let rt = (rng.next() % 200_000) as f32 / 100.0; // 0.00..2000.00 ms
        let method = rng.pick(&methods);
        let path = rng.pick(&paths);
        // Occasionally emit a malformed line so the skip-path gets exercised.
        if rng.next().is_multiple_of(1000) {
            writeln!(out, "garbage line not six fields").unwrap();
        } else {
            writeln!(out, "{ip} {status} {bytes} {rt:.1} {method} {path}").unwrap();
        }
    }
}
