//! `taskline` — a production-grade REST API built on `axum` + Postgres.
//!
//! Users register, log in for a JWT, and manage their own tasks. The point of
//! the module is to see one request travel the whole stack and know which layer
//! owns which job:
//!
//! - [`app::build_app`] wires the [`axum::Router`] — routes, `tower` layers,
//!   and the shared [`app::AppState`] (the pool + signing keys).
//! - Extractors do the parsing: [`axum::Json`] for bodies, and the custom
//!   [`auth::AuthUser`] that verifies the bearer token *as a function argument*,
//!   so a protected handler can't forget the check.
//! - [`handlers`] are thin: validate, run one owner-scoped query, return a row.
//! - [`error::AppError`] is the single type that turns every failure into an
//!   honest status code (its `IntoResponse` is your job).
//!
//! The security spine — salted memory-hard hashing ([`auth::hash_password`])
//! and the `WHERE user_id = $1` discipline in every task query — is what makes
//! this an API you could ship rather than a demo.

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod models;
pub mod openapi;
pub mod validation;

pub use app::{build_app, AppState};
pub use auth::{AuthConfig, AuthUser};
pub use config::Config;
pub use error::AppError;
