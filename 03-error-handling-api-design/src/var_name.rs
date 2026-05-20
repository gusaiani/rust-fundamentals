//! `VarName` — a validated, type-safe wrapper around an env-var name.
//!
//! Once a `VarName` exists, it has been through `parse` and is guaranteed to
//! match `[A-Z][A-Z0-9_]*`. The rest of the crate doesn't have to re-check.

use crate::error::Error;

/// A validated env-var name
///
/// Construct via [`VarName::parse`] or `TryFrom<&str>`. Once you hold one,
/// the inner string is guaranteed to match `[A-Z][A-Z0-9_]*`.
pub struct VarName(String);

impl VarName {
    /// Validate an env-var name. Must match `[A-Z][A-Z0-9_]*`.
    ///
    /// ```
    ///
    /// use envtyped::var_name::VarName;
    ///
    /// let ok = VarName::parse("PORT").unwrap();
    /// assert_eq!(ok.as_str(), "PORT");
    ///
    /// assert!(VarName::parse("port").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::InvalidName {
                name: s.to_owned(),
                reason: "empty",
            });
        }

        let first = s.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return Err(Error::InvalidName {
                name: s.to_owned(),
                reason: "first char must be A-Z",
            });
        }

        for c in s.chars().skip(1) {
            let valid = c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_';
            if !valid {
                return Err(Error::InvalidName {
                    name: s.to_owned(),
                    reason: "only A-Z, 0-9, and _ allowed after first char",
                });
            }
        }

        Ok(VarName(s.to_owned()))
    }

    /// Borrow the validated name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for VarName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

// Step 4: add a doc test on `parse` showing a successful name and a rejected one.
// (Doc tests live in `///` comments above the item, between triple-backticks.)
