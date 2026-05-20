//! Drives the public API. These will `todo!()`-panic until the matching step
//! is done — implement the function, then this test goes green.
//!
//! Three things to prove:
//!   1. the parser accepts valid lines and rejects each malformation
//!   2. `Merge` is associative (the map-reduce correctness premise)
//!   3. every parallel mode produces the same report as the sequential oracle
//!      (the newline-chunking correctness guard — Pill 13)

use logcrunch::parallel::analyze_parallel;
use logcrunch::pipeline::analyze_pipeline;
use logcrunch::rayon_impl::analyze_rayon;
use logcrunch::sequential::analyze_sequential;
use logcrunch::{parse_line, ParseError};

const FIXTURE: &[u8] = include_bytes!("fixtures/sample.log");

#[test]
fn parses_a_valid_line() {
    let e = parse_line("10.0.0.5 200 1432 12.4 GET /api/users")
        .expect("valid line should parse");
    assert_eq!(e.ip, "10.0.0.5");
    assert_eq!(e.status, 200);
    assert_eq!(e.bytes, 1432);
    assert_eq!(e.method, "GET");
    assert_eq!(e.path, "/api/users");
}

#[test]
fn rejects_wrong_field_count() {
    let err = parse_line("10.0.0.5 200 1432").unwrap_err();
    assert!(matches!(err, ParseError::WrongFieldCount { .. }), "got {err:?}");
}

#[test]
fn rejects_non_numeric_status() {
    let err = parse_line("10.0.0.5 OK 1432 12.4 GET /x").unwrap_err();
    assert!(matches!(err, ParseError::BadNumber { .. }), "got {err:?}");
}

#[test]
fn merge_is_associative() {
    // (a ⊕ b) ⊕ c  must equal  a ⊕ (b ⊕ c). Build three disjoint-ish
    // sub-logs, merge both ways, assert the reports match.
    use logcrunch::stats::Merge;

    let split = |s: &str| analyze_sequential(s.as_bytes());
    let a = "10.0.0.1 200 10 1.0 GET /a\n";
    let b = "10.0.0.2 500 20 2.0 POST /b\n10.0.0.1 200 30 3.0 GET /a\n";
    let c = "10.0.0.3 404 0 4.0 GET /c\n";

    let mut left = split(a);
    left.merge(split(b));
    left.merge(split(c));

    let mut right_bc = split(b);
    right_bc.merge(split(c));
    let mut right = split(a);
    right.merge(right_bc);

    let lr = left.into_report(10);
    let rr = right.into_report(10);
    assert_eq!(lr.requests, rr.requests);
    assert_eq!(lr.total_bytes, rr.total_bytes);
    assert_eq!(lr.status_counts, rr.status_counts);
}

#[test]
fn parallel_modes_match_sequential() {
    // The chunking-correctness guard: if a line is split across a chunk
    // boundary it gets double-counted or dropped, and this fails.
    let oracle = analyze_sequential(FIXTURE).into_report(10);

    for (name, got) in [
        ("parallel", analyze_parallel(FIXTURE, 4).into_report(10)),
        ("pipeline", analyze_pipeline(FIXTURE, 4).into_report(10)),
        ("rayon", analyze_rayon(FIXTURE).into_report(10)),
    ] {
        assert_eq!(got.requests, oracle.requests, "{name}: request count");
        assert_eq!(got.malformed, oracle.malformed, "{name}: malformed count");
        assert_eq!(got.total_bytes, oracle.total_bytes, "{name}: bytes");
        assert_eq!(got.status_counts, oracle.status_counts, "{name}: statuses");
        assert_eq!(got.top_paths, oracle.top_paths, "{name}: top paths");
    }
}
