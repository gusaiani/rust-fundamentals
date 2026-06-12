//! Target specs and scan-result types.
//!
//! A target spec is a host plus a port set, e.g.
//!
//! ```text
//! 127.0.0.1:1-1024          a range
//! example.com:22,80,443     a list
//! 10.0.0.1:80-90,443,8080   ranges and singletons mixed
//! ```
//!
//! Parsing is pure (no I/O, no DNS) so it's trivially unit-testable — DNS
//! resolution happens later, in the scanner, where it can be `.await`ed.

use std::time::Duration;

/// A resolved-on-paper scan target: a host string (resolved to addresses at
/// scan time) and the expanded, de-duplicated, sorted list of ports to probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub ports: Vec<u16>,
}

/// Why a target spec failed to parse. Pure parse errors — no I/O involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// No `:` separating host from ports, or an empty host.
    MissingHostOrPorts,
    /// A port token wasn't a u16 (or a range bound wasn't).
    BadPort { token: String },
    /// A range like `90-80` whose start exceeds its end.
    EmptyRange { start: u16, end: u16 },
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetError::MissingHostOrPorts => {
                write!(f, "expected `host:ports`, e.g. 127.0.0.1:1-1024")
            }
            TargetError::BadPort { token } => write!(f, "invalid port `{token}`"),
            TargetError::EmptyRange { start, end } => {
                write!(f, "empty port range {start}-{end} (start > end)")
            }
        }
    }
}

impl std::error::Error for TargetError {}

/// What a single probe concluded about one port.
///
/// The three-way distinction is the whole game in port scanning:
/// - `Open` — the `connect` succeeded (something is listening).
/// - `Closed` — the host actively refused (RST). Fast, definitive.
/// - `Filtered` — the connect neither succeeded nor was refused before the
///   timeout fired. A firewall is probably black-holing the SYN. *This is the
///   case that makes concurrency matter* — filtered ports cost a full timeout
///   each, so doing them one at a time is agony.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    Open,
    Closed,
    Filtered,
}

/// The result of probing one port: its state and how long the probe took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanOutcome {
    pub port: u16,
    pub state: PortState,
    pub rtt: Duration,
}

/// Parse a `host:ports` spec into a [`Target`].
///
/// `ports` is a comma-separated list of either single ports (`443`) or
/// inclusive ranges (`80-90`). Expand it to a concrete `Vec<u16>`, then sort
/// and de-duplicate so `80-82,81,80` becomes `[80, 81, 82]`.
///
/// Steps:
///   1. `rsplit_once(':')` to peel host from the port list. (Reject empty.)
///   2. For each comma token, `split_once('-')` → range, else parse a single.
///   3. Validate `start <= end` for ranges; collect into the port vec.
///   4. `sort_unstable` + `dedup`.
pub fn parse_target(spec: &str) -> Result<Target, TargetError> {
    // TODO (step 1): implement per the doc comment above. Pure function — no
    // network, no async. Make the unit tests at the bottom pass first; they
    // pin the exact semantics (mixed ranges, dedup, the error cases).
    let _ = spec;
    todo!("parse host:ports into a Target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_port() {
        let t = parse_target("127.0.0.1:443").unwrap();
        assert_eq!(t.host, "127.0.0.1");
        assert_eq!(t.ports, vec![443]);
    }

    #[test]
    fn expands_and_dedups_a_mixed_spec() {
        let t = parse_target("example.com:80-82,81,443").unwrap();
        assert_eq!(t.host, "example.com");
        assert_eq!(t.ports, vec![80, 81, 82, 443]);
    }

    #[test]
    fn rejects_missing_ports() {
        assert_eq!(
            parse_target("127.0.0.1").unwrap_err(),
            TargetError::MissingHostOrPorts
        );
    }

    #[test]
    fn rejects_a_backwards_range() {
        assert_eq!(
            parse_target("h:90-80").unwrap_err(),
            TargetError::EmptyRange { start: 90, end: 80 }
        );
    }

    #[test]
    fn rejects_a_non_numeric_port() {
        assert!(matches!(
            parse_target("h:http").unwrap_err(),
            TargetError::BadPort { .. }
        ));
    }
}
