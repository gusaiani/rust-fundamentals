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
    pub fn record(&mut self, entry: &LogEntry) {
        self.requests += 1;
        self.total_bytes += entry.bytes;

        *self.status_counts.entry(entry.status).or_insert(0) += 1;
        *self.path_counts.entry(entry.path.clone()).or_insert(0) += 1;
        *self.ip_counts.entry(entry.ip.clone()).or_insert(0) += 1;

        self.request_times.push(entry.request_time_ms);
    }

    pub fn record_malformed(&mut self) {
        self.malformed += 1;
    }

    pub fn percentile(&self, p: f32) -> Option<f32> {
        if self.request_times.is_empty() {
            return None;
        }

        let mut sorted = self.request_times.clone();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

        let len = sorted.len();
        let rank = (p / 100.0) * (len - 1) as f32;
        let index = rank.round() as usize;
        Some(sorted[index])
    }

    /// Freeze the aggregate into a printable report. `top_k` caps the
    /// path/IP leaderboards.
    pub fn into_report(self, top_k: usize) -> Report {
        let p50 = self.percentile(50.0);
        let p95 = self.percentile(95.0);
        let p99 = self.percentile(99.0);

        let mut status_counts: Vec<(u16, u64)> = self.status_counts.into_iter().collect();
        status_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut top_paths: Vec<(String, u64)> = self.path_counts.into_iter().collect();
        top_paths.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_paths.truncate(top_k);

        let mut top_ips: Vec<(String, u64)> = self.ip_counts.into_iter().collect();
        top_ips.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_ips.truncate(top_k);

        let server_errors: u64 = status_counts
            .iter()
            .filter(|(code, _)| (500..=599).contains(code))
            .map(|(_, count)| count)
            .sum();

        let error_rate = if self.requests == 0 {
            0.0
        } else {
            server_errors as f32 / self.requests as f32
        };

        Report {
            requests: self.requests,
            malformed: self.malformed,
            total_bytes: self.total_bytes,
            status_counts,
            top_paths,
            top_ips,
            error_rate,
            p50,
            p95,
            p99,
        }
    }
}

impl Merge for Stats {
    fn merge(&mut self, other: Self) {
        self.requests += other.requests;
        self.malformed += other.malformed;
        self.total_bytes += other.total_bytes;

        for (status, count) in other.status_counts {
            *self.status_counts.entry(status).or_insert(0) += count;
        }
        for (path, count) in other.path_counts {
            *self.path_counts.entry(path).or_insert(0) += count;
        }
        for (ip, count) in other.ip_counts {
            *self.ip_counts.entry(ip).or_insert(0) += count;
        }

        self.request_times.extend(other.request_times);
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
        writeln!(f, "=== logcrunch report ===")?;
        writeln!(f, "requests:   {}", self.requests)?;
        writeln!(f, "malformed:  {}", self.malformed)?;
        writeln!(f, "bytes:      {}", self.total_bytes)?;
        writeln!(f, "error rate: {:.2}%", self.error_rate * 100.0)?;
        writeln!(f, "\nstatus codes:")?;
        for (code, count) in &self.status_counts {
            writeln!(f, "  {code}  {count}")?;
        }
        writeln!(f, "\ntop paths:")?;
        for (path, count) in &self.top_paths {
            writeln!(f, "  {count:>8}  {path}")?;
        }
        writeln!(f, "\ntop IPs:")?;
        for (ip, count) in &self.top_ips {
            writeln!(f, "  {count:>8}  {ip}")?;
        }

        let fmt_ms = |p: Option<f32>| {
            p.map(|v| format!("{v:.1}ms"))
                .unwrap_or_else(|| "n/a".to_string())
        };

        writeln!(f, "\nrequest time:")?;
        writeln!(f, "  p50  {}", fmt_ms(self.p50))?;
        writeln!(f, "  p95  {}", fmt_ms(self.p95))?;
        writeln!(f, "  p99  {}", fmt_ms(self.p99))?;

        Ok(())
    }
}
