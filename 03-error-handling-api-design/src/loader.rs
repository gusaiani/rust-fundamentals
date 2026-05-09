//! `Loader` builder + `Env` reader.
//!
//! Usage:
//!
//! ```ignore
//! let env = Loader::new()
//!     .require::<u16>("PORT")
//!     .optional_or::<String>("LOG_LEVEL", "info".into())
//!     .load()?;
//!
//! let port: u16 = env.get("PORT")?;
//! ```

use std::any::Any;
use std::collections::HashMap;

use crate::error::Error;
use crate::from_env::FromEnv;
use crate::var_name::VarName;

/// Type-erased value bag. Each entry is `Box<dyn Any>` because each var may
/// be a different concrete type. `Send + Sync` so the loaded `Env` can be
/// shared across threads.
type ValueBag = HashMap<String, Box<dyn Any + Send + Sync>>;

// Step 1: declare the builder.
//
//   pub struct Loader {
//       values: ValueBag,
//       errors: Vec<Error>,
//   }
//
// Both fields private. `errors` accumulates instead of short-circuiting so
// the user gets every problem in one shot.

/// Stub. Replace with the real `Loader`.
pub struct Loader {
    /// TODO
    _placeholder: (),
}

impl Loader {
    // Step 2: `pub fn new() -> Self` — empty bag, empty error list.

    // Step 3: `.require::<T>(name)`
    //
    //   pub fn require<T: FromEnv + Send + Sync + 'static>(self, name: &str) -> Self
    //
    // - Validate the name with `VarName::parse`. On failure, push `Error::InvalidName`
    //   into `self.errors` and return.
    // - Read the env: `std::env::var(name)`.
    //   - On `Err(_)`, push `Error::Missing { var: name.into() }`.
    //   - On `Ok(raw)`, parse with `T::from_env_str`. On parse error,
    //     push `Error::Parse { var, source }`. On success, insert the boxed
    //     value into `self.values` keyed by the var's name.

    // Step 4: `.optional::<T>(name)`
    //
    // Same as `require`, but a missing var is *not* an error — just skip it.

    // Step 5: `.optional_or::<T>(name, default)`
    //
    //   pub fn optional_or<T: FromEnv + Send + Sync + 'static>(
    //       self, name: &str, default: T,
    //   ) -> Self
    //
    // Same as `optional`, but if the var is missing, insert the default.
    // (A *parse* failure is still an error — don't fall back to the default
    // when the user clearly tried to set the var but typed it wrong.)

    // Step 6: `.load(self) -> Result<Env, Vec<Error>>`
    //
    // If `self.errors` is empty, return `Ok(Env { values: self.values })`.
    // Otherwise return `Err(self.errors)`.

    /// Stub.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

// Step 7: declare the reader returned by `Loader::load`.
//
//   pub struct Env {
//       values: ValueBag,
//   }
//
// One method:
//
//   pub fn get<T: FromEnv + Clone + Send + Sync + 'static>(&self, name: &str) -> Result<T, Error>
//
// - Look up `name` in the map.
// - If not present: `Err(Error::NotRequested { var: name.into() })`.
// - If present: `downcast_ref::<T>()` and `.cloned()`. A failed downcast is
//   also `NotRequested` — the caller asked for a different type than was
//   loaded, which from the caller's perspective is the same kind of bug.

/// Stub. Replace with the real `Env`.
pub struct Env {
    /// TODO
    _placeholder: (),
}

impl Env {
    /// Stub.
    pub fn get<T>(&self, _name: &str) -> Result<T, Error> {
        todo!("implement Env::get")
    }
}

// Suppress unused-import warnings until the real types reference these.
#[allow(dead_code)]
fn _imports_used<T: FromEnv>(_: ValueBag, _: VarName, _: T) {}
