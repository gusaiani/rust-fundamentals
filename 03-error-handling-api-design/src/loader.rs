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

/// Builder that describes the env-var schema and accumulates parse problems.
///
/// Call `.require`/`.optional`/`.optional_or` to declare vars, then `.load()`
/// to finalize. All errors are collected — you get one `Vec<Error>` with every
/// missing or malformed var, not just the first.
pub struct Loader {
    values: ValueBag,   // successfully parsed values, keyed by var name
    errors: Vec<Error>, // every problem seen so far; non-empty => .load() fails
}

impl Loader {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Declare a required env var of type `T`.
    ///
    /// Failures (invalid name, missing var, parse failure) are *recorded*, not
    /// returned — call `.load()` to surface them all at once.
    pub fn require<T>(mut self, name: &str) -> Self 
    where
        T: FromEnv + Send + Sync + 'static,
    {
        // 1. Validate the name. Invalid names are a hard error: we can't even
        //    look up the value, so just record and return
        let var_name = match VarName::parse(name) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };

        // 2. Read the raw env value. Missing it is its own error variant
        //    so callers can distinguish "not set" from "set but garbage".
        let raw = match std::env::var(name) {
            Ok(s) => s,
            Err(_) => {
                self.errors.push(Error::Missing { var: name.to_owned() });
                return self;
            }
        };

        // 3. Parse via the FromEnv impl chosen at the call site (the turbofish
        //    on `.require::<T>(...)` decides with impl)
        let parsed: T = match T::from_env_str(&raw) {
            Ok(v) => v,
            Err(source) => {
                self.errors.push(Error::Parse {
                    var: name.to_owned(),
                    source,  // already a Box<dyn Error + Send + Sync>, matches the field type
                });
                return self;
            }
        };

        // 4. Type-erase and stash. The cast `as Box<dyn Any + ...>` is the 
        //    type-erasure step; `Env::get` will downcast back to T later.
        let boxed: Box<dyn Any + Send + Sync> = Box::new(parsed);
        self.values.insert(var_name.as_str().to_owned(), boxed);

        self
    }

    pub fn optional<T>(mut self, name: &str) -> Self
    where
        T: FromEnv + Send + Sync + 'static,
    {
        let var_name = match VarName::parse(name) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };

        // Difference from `require`: missing is fine, just return without recording.
        let raw = match std::env::var(name) {
            Ok(s) => s,
            Err(_) => return self,
        };

        let parsed: T = match T::from_env_str(&raw) {
            Ok(v) => v,
            Err(source) => {
                self.errors.push(Error::Parse {
                    var: name.to_owned(),
                    source,
                });
                return self;
            }
        };

        let boxed: Box<dyn Any + Send + Sync> = Box::new(parsed);
        self.values.insert(var_name.as_str().to_owned(), boxed);

        self
    }

    /// Declare an optional env var with a fallback default.
    ///
    /// - Missing → insert `default`.
    /// — Present → parse via `T::from_env_str`. Parse failure is recorded as
    ///   an error (a typo isn't silently replaced by the default).
    pub fn optional_or<T>(mut self, name: &str, default: T) -> Self
    where
        T: FromEnv + Send + Sync + 'static,
    {
        let var_name = match VarName::parse(name) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };

        // Branch on env presence. Missing → use the default we were given;
        // present → parse and fall back to recording an error on failure.
        let value: T = match std::env::var(name) {
            Err(_) => default,
            Ok(raw) => match T::from_env_str(&raw) {
                Ok(v) => v,
                Err(source) => {
                    self.errors.push(Error::Parse {
                        var: name.to_owned(),
                        source,
                    });
                    return self;
                }
            },
        };

        let boxed: Box<dyn Any + Send + Sync> = Box::new(value);
        self.values.insert(var_name.as_str().to_owned(), boxed);

        self
    }


    // Step 6: `.load(self) -> Result<Env, Vec<Error>>`
    //
    // If `self.errors` is empty, return `Ok(Env { values: self.values })`.
    // Otherwise return `Err(self.errors)`.

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