//! `envguard` — typed environment-variable loading with structured errors.
//!
//! Describe the schema fluently, load it once, and get every config problem
//! in a single `Vec<Error>` rather than dying on the first missing var.
//!
//! ```no_run
//! use envguard::Loader;
//!
//! let env = Loader::new()
//!     .require::<u16>("PORT")
//!     .optional_or::<String>("LOG_LEVEL", "info".into())
//!     .load()
//!     .expect("config errors");
//!
//! let port: u16 = env.get("PORT").unwrap();
//! ```
//!
//! The doc test above is `ignore`d while the crate is still stubbed. Once you
//! have the public API in place, change `ignore` to nothing — `cargo test`
//! will then compile and run it.

// Once you've filled the public API, switch this on:
// #![deny(missing_docs)]

pub mod error;
pub mod from_env;
pub mod loader;
pub mod var_name;

pub use error::Error;
pub use from_env::{FromEnv, ParseFailure};
pub use loader::{Env, Loader};
pub use var_name::VarName;
