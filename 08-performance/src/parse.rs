//! Zero-copy line parsing (Pills 6 & 7).
//!
//! Two jobs, both on raw bytes — no `String`, no `f64`:
//!   - [`split_line`] cuts a line into a `&[u8]` station name and a `&[u8]`
//!     temperature, finding the `;` with `memchr` (SIMD under the hood, Pill 11).
//!   - [`parse_temp`] turns the temperature bytes into an `i32` *number of
//!     tenths* (`"-12.3"` -> `-123`), so the whole hot loop is integer-only.

/// Split a line (no trailing `\n`) into `(name, temp)`, both borrowing the input.
///
/// The station name is everything before the `;`; the temperature is everything
/// after it. No allocation — both halves are slices into `line`.
///
/// TODO (step 1): find the `;` with `memchr::memchr` and slice around it. See the
/// Pill 6 example and the step-1 hint in the README.
pub fn split_line(line: &[u8]) -> (&[u8], &[u8]) {
    let _ = line;
    todo!("split on ';' with memchr — see Pill 6")
}

/// Parse a temperature in the fixed 1BRC format into tenths of a degree.
///
/// The format is rigid: an optional leading `-`, then one or two digits, a `.`,
/// then exactly one digit. So the value is always an integer count of tenths:
/// `"4.5"` -> `45`, `"-12.3"` -> `-123`, `"0.0"` -> `0`. No floating point.
///
/// TODO (step 1): strip an optional `-`, then match on the byte-slice shape
/// (`[d, b'.', d]` vs `[d, d, b'.', d]`) and fold the ASCII digits into an i32.
/// See Pill 7 for the near-complete version.
pub fn parse_temp(bytes: &[u8]) -> i32 {
    let _ = bytes;
    todo!("parse fixed-point tenths as i32 — see Pill 7")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_name_and_temp() {
        let (name, temp) = split_line(b"Hamburg;12.0");
        assert_eq!(name, b"Hamburg");
        assert_eq!(temp, b"12.0");
    }

    #[test]
    fn split_handles_spaces_and_unicode_in_name() {
        let (name, temp) = split_line("St. John's;-3.4".as_bytes());
        assert_eq!(name, "St. John's".as_bytes());
        assert_eq!(temp, b"-3.4");
    }

    #[test]
    fn parses_positive_two_digit() {
        assert_eq!(parse_temp(b"12.0"), 120);
        assert_eq!(parse_temp(b"38.8"), 388);
    }

    #[test]
    fn parses_single_digit() {
        assert_eq!(parse_temp(b"4.5"), 45);
        assert_eq!(parse_temp(b"0.0"), 0);
    }

    #[test]
    fn parses_negative() {
        assert_eq!(parse_temp(b"-3.4"), -34);
        assert_eq!(parse_temp(b"-12.3"), -123);
        assert_eq!(parse_temp(b"-0.5"), -5);
    }

    #[test]
    fn parses_extremes() {
        assert_eq!(parse_temp(b"99.9"), 999);
        assert_eq!(parse_temp(b"-99.9"), -999);
    }
}
