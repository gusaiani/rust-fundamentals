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

pub fn parse_line(line: &str) -> Result<LogEntry, ParseError> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 6 {
        return Err(ParseError::WrongFieldCount { found: parts.len() });
    }

    let status: u16 = parts[1]
        .parse()
        .map_err(|_| ParseError::BadNumber { field: "status" })?;

    let bytes: u64 = parts[2]
        .parse()
        .map_err(|_| ParseError::BadNumber { field: "bytes" })?;

    let request_time_ms: f32 = parts[3].parse().map_err(|_| ParseError::BadNumber {
        field: "request_time_ms",
    })?;

    Ok(LogEntry {
        ip: parts[0].to_owned(),
        status,
        bytes,
        request_time_ms,
        method: parts[4].to_owned(),
        path: parts[5].to_owned(),
    })
}
