//! End-to-end correctness. **Given.** These fail (panic: `not yet implemented`)
//! until the stubs are done, then pin the two properties that matter:
//!   1. the output is *correct* (checked against a hand-computed expected line);
//!   2. the parallel path *agrees* with the sequential path for any thread count
//!      — the safety net for every parallel optimization you make in Pill 12.
//!
//! Run with `cargo test`.

use brc::aggregate::format_results;
use brc::runner::{run_parallel, run_sequential};

/// A small, fully hand-verifiable input.
const SAMPLE: &[u8] = b"\
Hamburg;12.0
Bulawayo;8.9
Palembang;38.8
Hamburg;-3.4
Bulawayo;19.2
Palembang;-5.0
Hamburg;0.0
Bulawayo;0.0
";

#[test]
fn output_matches_hand_computed_reference() {
    // Hamburg:  min -3.4, max 12.0, mean (12.0 - 3.4 + 0.0)/3 =  2.8666.. -> 2.9
    // Bulawayo: min  0.0, max 19.2, mean (8.9 + 19.2 + 0.0)/3 =  9.3666.. -> 9.4
    // Palembang:min -5.0, max 38.8, mean (38.8 - 5.0)/2       = 16.9
    let expected =
        "{Bulawayo=0.0/9.4/19.2, Hamburg=-3.4/2.9/12.0, Palembang=-5.0/16.9/38.8}";
    let got = format_results(&run_sequential(SAMPLE));
    assert_eq!(got, expected);
}

#[test]
fn parallel_agrees_with_sequential() {
    let seq = format_results(&run_sequential(SAMPLE));
    for threads in [1, 2, 3, 4, 8, 16] {
        let par = format_results(&run_parallel(SAMPLE, threads));
        assert_eq!(seq, par, "parallel output diverged at threads={threads}");
    }
}

#[test]
fn handles_single_station_single_row() {
    let out = format_results(&run_sequential(b"Reykjav\xc3\xadk;4.3\n"));
    assert_eq!(out, "{Reykjavík=4.3/4.3/4.3}");
}

#[test]
fn more_threads_than_lines_is_safe() {
    // Three lines, sixteen requested workers: split_chunks must produce no empty
    // chunks and the merge must still be correct.
    let data = b"A;1.0\nB;2.0\nC;3.0\n";
    let seq = format_results(&run_sequential(data));
    let par = format_results(&run_parallel(data, 16));
    assert_eq!(seq, par);
}
