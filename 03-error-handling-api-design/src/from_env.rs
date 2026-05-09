//! `FromEnv` — sealed trait of types that can be parsed from an env-var
//! string. "Sealed" means external crates can see the trait but cannot
//! implement it; the crate controls the supported set.

// Step 1: create the private supertrait.
//
//   mod private {
//       pub trait Sealed {}
//   }
//
// External crates can't access `private::Sealed`, so they can't implement
// any trait that requires it as a supertrait.

// Step 2: declare the public trait, sealed by inheriting from `private::Sealed`.
//
//   pub trait FromEnv: private::Sealed + Sized {
//       fn from_env_str(raw: &str) -> Result<Self, ParseFailure>;
//   }
//
// Where `ParseFailure` is `Box<dyn std::error::Error + Send + Sync>` — we
// erase the concrete parse error so the trait method has one return type
// regardless of `T`.
//
// type alias for clarity:
//
//   pub type ParseFailure = Box<dyn std::error::Error + Send + Sync>;

/// Stub alias. Replace once the trait is wired up.
pub type ParseFailure = Box<dyn std::error::Error + Send + Sync>;

// Step 3: implement Sealed + FromEnv for each supported type.
// Recommended set: u16, u32, u64, i32, i64, usize, bool, String.
//
// Pattern:
//
//   impl private::Sealed for u16 {}
//   impl FromEnv for u16 {
//       fn from_env_str(raw: &str) -> Result<Self, ParseFailure> {
//           raw.parse::<u16>().map_err(|e| Box::new(e) as ParseFailure)
//       }
//   }
//
// `String` is the trivial case: just `Ok(raw.to_owned())`.
// `bool`: accept "true"/"false" (case-insensitive) plus "1"/"0".

/// Stub trait. Replace with the sealed real one.
pub trait FromEnv: Sized {
    /// Parse the raw env value into `Self`.
    fn from_env_str(raw: &str) -> Result<Self, ParseFailure>;
}
