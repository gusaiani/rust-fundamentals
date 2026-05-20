//! The aggregate (`Stats`), its associative/commutative `Merge`, and the
//! human-facing `Report`.
//!
//! Everything in `Stats` must combine *associatively and commutatively* —
//! thread finish order is nondeterministic, so `a.merge(b)` and `b.merge(a)`,
//! and any grouping, must yield the same result. Counts and per-key count
//! maps qualify. Averages do **not** — keep `(sum, count)` and divide late.
//! Percentiles don't compose either — see the step-2 hint in the README.

use std::collections::HashMap;

use crate::parser::LogEntry;

/// Combine `other` into `self`. Must be associative and commutative.
pub trait Merge {
    fn merge(&mut self, other: Self);
}

/// Running aggregate over a stream of [`LogEntry`]s. The fields are given;
/// you implement the four methods and the `Merge` impl below.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub requests: u64,
    /// Lines that failed to parse (counted, never lost).
    pub malformed: u64,
    pub total_bytes: u64,
    pub status_counts: HashMap<u16, u64>,
    pub path_counts: HashMap<String, u64>,
    pub ip_counts: HashMap<String, u64>,
    /// Every request-time sample. Needed because percentiles don't merge;
    /// note the memory cost — the stretch goal swaps this for a sketch.
    pub request_times: Vec<f32>,
}

impl Stats {
    /// Fold one successfully-parsed entry into the running totals.
    //
    // TODO (step 2): bump `requests`, add `bytes`, increment the three count
    // maps (`*self.status_counts.entry(e.status).or_insert(0) += 1`, etc.),
    // push `request_time_ms`.
    pub fn record(&mut self, entry: &LogEntry) {
        let _ = entry;
        todo!("step 2: fold one entry into the aggregate")
    }

    /// Count a line that failed to parse (don't lose track of bad input).
    //
    // TODO (step 2): `self.malformed += 1`.
    pub fn record_malformed(&mut self) {
        todo!("step 2: count a skipped malformed line")
    }

    /// `p` is a percentile in `0.0..=100.0`. Returns `None` if no samples.
    //
    // TODO (step 2): clone the samples, `sort_unstable_by(|a,b|
    // a.partial_cmp(b).unwrap())` (f32 has no total Ord), index at
    // `((p/100.0) * (len-1)).round() as usize`.
    pub fn percentile(&self, p: f32) -> Option<f32> {
        let _ = p;
        todo!("step 2: exact percentile from the sample vec")
    }

    /// Freeze the aggregate into a printable report. `top_k` caps the
    /// path/IP leaderboards.
    //
    // TODO (step 2): turn each count map into a Vec, sort by count desc,
    // truncate to top_k; error_rate = (sum of 5xx) / requests; pull
    // p50/p95/p99 from `percentile`.
    pub fn into_report(self, top_k: usize) -> Report {
        let _ = top_k;
        todo!("step 2: build the Report")
    }
}

impl Merge for Stats {
    // TODO (step 2): add the scalars; for each map fold `other`'s entries in
    // (`*self.m.entry(k).or_insert(0) += v`); `self.request_times.extend(
    // other.request_times)`. This is the one provably-correct combine reused
    // by every parallel implementation — get it right once.
    fn merge(&mut self, other: Self) {
        let _ = other;
        todo!("step 2: associative + commutative merge")
    }
}

/// The frozen, printable result. `Stats` is the mutable accumulator;
/// `Report` is what the CLI prints.
#[derive(Debug, Clone)]
pub struct Report {
    pub requests: u64,
    pub malformed: u64,
    pub total_bytes: u64,
    pub status_counts: Vec<(u16, u64)>,
    pub top_paths: Vec<(String, u64)>,
    pub top_ips: Vec<(String, u64)>,
    pub error_rate: f32,
    pub p50: Option<f32>,
    pub p95: Option<f32>,
    pub p99: Option<f32>,
}

impl std::fmt::Display for Report {
    // TODO (step 2, optional polish): pretty-print. `{:#?}` is fine to start;
    // make it readable once the numbers are correct.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}
