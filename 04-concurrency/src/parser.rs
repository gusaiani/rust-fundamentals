//! Log-line parsing.
//!
//! The format is fixed and space-delimited — exactly six fields:
//!
//! ```text
//! <ip> <status> <bytes> <request_time_ms> <method> <path>
//! 10.0.0.5 200 1432 12.4 GET /api/users
//! ```
//!
//! `path` never contains spaces in this synthetic format, so
//! `split_whitespace` is enough — no regex, no quoting.

/// One parsed log record. The shape is given; you implement [`parse_line`].
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub ip: String,
    pub status: u16,
    pub bytes: u64,
    pub request_time_ms: f32,
    pub method: String,
    pub path: String,
}

/// Why a line failed to parse. Callers *count and skip* these — a malformed
/// line must never panic the analyzer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Did not contain exactly six whitespace-separated fields.
    WrongFieldCount { found: usize },
    /// A numeric field (`status`, `bytes`, `request_time_ms`) didn't parse.
    BadNumber { field: &'static str },
}

/// Parse a single line into a [`LogEntry`].
///
/// TODO (step 1):
///   1. `split_whitespace().collect::<Vec<_>>()` (path has no spaces, so the
///      6th field is the whole path).
///   2. `if parts.len() != 6 { return Err(WrongFieldCount { found }) }`.
///   3. Parse `status: u16`, `bytes: u64`, `request_time_ms: f32`, mapping
///      each `.parse()` error to `BadNumber { field: "status" }` etc.
///   4. `Ok(LogEntry { ip: parts[0].to_owned(), ... })`.
pub fn parse_line(line: &str) -> Result<LogEntry, ParseError> {
    let _ = line;
    todo!("step 1: parse the six fields")
}
