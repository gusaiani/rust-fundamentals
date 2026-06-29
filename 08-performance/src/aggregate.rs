//! The per-station accumulator and output formatting.
//!
//! [`Stats`] keeps min/max/sum/count as integers (Pill 7): min/max in tenths as
//! `i32`, sum as `i64` (a billion values of up to ±999 overflows `i32`), count
//! as `u64`. Float appears only at the very end, for the mean and for display.

use std::collections::BTreeMap;

use crate::hash::FastMap;

/// Running min/mean/max for one station, in integer tenths of a degree.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    /// Minimum temperature seen, in tenths (e.g. -123 == -12.3°C).
    pub min: i32,
    /// Maximum temperature seen, in tenths.
    pub max: i32,
    /// Sum of all temperatures, in tenths. `i64` so a billion adds can't overflow.
    pub sum: i64,
    /// Number of measurements folded in.
    pub count: u64,
}

impl Default for Stats {
    /// The identity for `record`: min starts at +inf, max at -inf (in tenths).
    fn default() -> Self {
        Stats {
            min: i32::MAX,
            max: i32::MIN,
            sum: 0,
            count: 0,
        }
    }
}

impl Stats {
    /// Fold one measurement (in tenths) into the accumulator.
    pub fn record(&mut self, temp: i32) {
        self.min = self.min.min(temp);
        self.max = self.max.max(temp);
        self.sum += temp as i64; // a billion values can't overflow
        self.count += 1;
    }

    /// Fold another (partial) `Stats` into this one — used to merge per-thread
    /// maps in the parallel path (Pill 12).
    pub fn merge(&mut self, other: &Stats) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.count += other.count;
    }

    /// Mean temperature in degrees Celsius (float only here, out of the hot loop).
    pub fn mean(&self) -> f64 {
        self.sum as f64 / 10.0 / self.count as f64
    }
}

/// Format the final result line in the exact 1BRC format, sorted by station:
/// `{Abha=-23.0/18.0/59.2, Abidjan=-16.2/26.0/67.3, ...}`.
///
/// `min`/`max` are tenths (divide by 10.0); `mean` comes from [`Stats::mean`].
/// All three print with one decimal (`{:.1}`). The `BTreeMap` gives the
/// alphabetical ordering the spec requires.
pub fn format_results(stats: &BTreeMap<Vec<u8>, Stats>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (name, s) in stats {
        let name = String::from_utf8_lossy(name);
        let min = s.min as f64 / 10.0;
        let max = s.max as f64 / 10.0;
        parts.push(format!("{name}={min:.1}/{:.1}/{max:.1}", s.mean()));
    }
    format!("{{{}}}", parts.join(", "))
}

/// Sort a borrowed-key map (the hot-loop representation) into the owned,
/// alphabetically-ordered map that [`format_results`] takes. Copies the ~400
/// keys exactly once, at the end — never in the hot loop.
pub fn into_sorted(map: FastMap<&[u8], Stats>) -> BTreeMap<Vec<u8>, Stats> {
    map.into_iter().map(|(k, v)| (k.to_vec(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_read() {
        let mut s = Stats::default();
        for t in [120, -34, 388, 0] {
            s.record(t);
        }
        assert_eq!(s.min, -34);
        assert_eq!(s.max, 388);
        assert_eq!(s.sum, 120 - 34 + 388 + 0);
        assert_eq!(s.count, 4);
    }

    #[test]
    fn merge_is_associative_with_record() {
        let mut whole = Stats::default();
        for t in [10, 20, 30, 40] {
            whole.record(t);
        }

        let mut a = Stats::default();
        a.record(10);
        a.record(20);
        let mut b = Stats::default();
        b.record(30);
        b.record(40);
        a.merge(&b);

        assert_eq!(a.min, whole.min);
        assert_eq!(a.max, whole.max);
        assert_eq!(a.sum, whole.sum);
        assert_eq!(a.count, whole.count);
    }

    #[test]
    fn mean_is_in_degrees() {
        let mut s = Stats::default();
        s.record(100); // 10.0
        s.record(200); // 20.0
        assert!((s.mean() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn formats_sorted_one_decimal() {
        let mut map: BTreeMap<Vec<u8>, Stats> = BTreeMap::new();
        let mut bravo = Stats::default();
        bravo.record(50); // 5.0
        let mut alpha = Stats::default();
        alpha.record(-15); // -1.5
        map.insert(b"Bravo".to_vec(), bravo);
        map.insert(b"Alpha".to_vec(), alpha);
        assert_eq!(
            format_results(&map),
            "{Alpha=-1.5/-1.5/-1.5, Bravo=5.0/5.0/5.0}"
        );
    }
}
