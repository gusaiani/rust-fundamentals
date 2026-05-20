//! `logcrunch` — a parallel server-log analyzer.
//!
//! The same aggregation is implemented four ways so you can compare them:
//!
//! - [`sequential::analyze_sequential`] — the baseline / correctness oracle
//! - [`parallel::analyze_parallel`] — hand-rolled scoped-thread map-reduce
//! - [`pipeline::analyze_pipeline`] — bounded crossbeam channel pipeline
//! - [`rayon_impl::analyze_rayon`] — `rayon` `par_iter().fold().reduce()`
//!
//! All four must produce identical [`stats::Report`]s for the same input —
//! the integration tests enforce that.

pub mod parallel;
pub mod parser;
pub mod pipeline;
pub mod rayon_impl;
pub mod sequential;
pub mod stats;

pub use parser::{parse_line, LogEntry, ParseError};
pub use stats::{Merge, Report, Stats};
