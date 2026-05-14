mod private {
    // `pub` within the crate, but the enclosing `private` module is not `pub`,
    // so external crates cannot reach this trait — and therefore cannot impl it.
    pub trait Sealed {}
}

/// Types that can be parsed from a raw env-var string.
///
/// Sealed: only this crate may implement it. That lets us add methods later
/// without breaking downstream impls (there aren't any).
pub trait FromEnv: private::Sealed + Sized {
    /// Parse the raw env value into `Self`. Returns a boxed error so the
    /// trait method has one signature regardless of the concrete parse error.
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure>;
}

pub type ParseFailure = Box<dyn std::error::Error + Send + Sync>;

impl private::Sealed for u16 {}
impl FromEnv for u16 {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        // `raw.parse::<u16>()` returns Result<u16, ParseIntError>.
        // We box+erase the concrete error so the trait method has one return type.
        raw.parse::<u16>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for u32 {}
impl FromEnv for u32 {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        raw.parse::<u32>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for u64 {}
impl FromEnv for u64 {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        raw.parse::<u64>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for i32 {}
impl FromEnv for i32 {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        raw.parse::<i32>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for i64 {}
impl FromEnv for i64 {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        raw.parse::<i64>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for usize {}
impl FromEnv for usize {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        raw.parse::<usize>().map_err(|e| Box::new(e) as ParseFailure)
    }
}

impl private::Sealed for String {}
impl FromEnv for String {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        // `raw` is borrowed; `to_owned` allocates a String we can return by value.
        Ok(raw.to_owned())
    }
}

impl private::Sealed for bool {}
impl FromEnv for bool {
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
        // Normalize once so the match arms stay readable.
        let normalized = raw.trim().to_ascii_lowercase();

        match normalized.as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => {
                // No std error type exactly fits "bad bool literal", so build one
                // from a string message via `Box::<dyn Error + ...>::from(...)`.
                let msg = format!("expected one of true/false/1/0, got `{raw}`");
                Err(msg.into())  // String -> Box<dyn Error + Send + Sync> via blanket impl
            }
        }
    }
}