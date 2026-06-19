//! The one error type every handler returns.
//!
//! The enum and its `From` conversions are **given**; implementing
//! [`IntoResponse`] (the status-code mapping) is **step 1** of the project.
//!
//! The design (Pill 3): handlers return `Result<T, AppError>`, so `?` turns any
//! failure into the right HTTP response in *one* place. The `From` impls below
//! are what make `?` work on `sqlx`/`jsonwebtoken`/validation errors — and they
//! also enforce Pill 13's "never leak internals": a raw database error becomes a
//! generic `Internal` (logged here, opaque to the client).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::validation::ValidationErrors;

/// Every way a request can fail, mapped to a status code in `into_response`.
#[derive(Debug)]
pub enum AppError {
    /// Input parsed but is semantically invalid -> 422 (Pill 11).
    Validation(ValidationErrors),
    /// Missing/invalid/expired credentials -> 401.
    Unauthorized,
    /// Authenticated but not allowed -> 403.
    Forbidden,
    /// Unknown id, or someone else's owner-scoped row -> 404 (Pill 12).
    NotFound,
    /// A uniqueness/business conflict, e.g. duplicate email -> 409.
    Conflict(String),
    /// Malformed request the framework didn't already reject -> 400.
    BadRequest(String),
    /// Our fault -> 500. The real cause is logged, never returned (Pill 13).
    Internal,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Validation(_) => write!(f, "validation failed"),
            AppError::Unauthorized => write!(f, "unauthorized"),
            AppError::Forbidden => write!(f, "forbidden"),
            AppError::NotFound => write!(f, "not found"),
            AppError::Conflict(m) => write!(f, "conflict: {m}"),
            AppError::BadRequest(m) => write!(f, "bad request: {m}"),
            AppError::Internal => write!(f, "internal error"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ValidationErrors> for AppError {
    fn from(e: ValidationErrors) -> Self {
        AppError::Validation(e)
    }
}

/// Map database errors to HTTP-meaningful variants. This is where Pill 13's
/// "don't leak, do log" lives: `RowNotFound` -> 404, a unique-violation -> 409,
/// everything else is logged and collapsed to a generic 500.
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Conflict("resource already exists".into())
            }
            other => {
                // Logged for us; opaque to the client.
                tracing::error!(error = %other, "database error");
                AppError::Internal
            }
        }
    }
}

/// JWT failures (bad signature, expired) are always an auth problem.
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        AppError::Unauthorized
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AppError::Validation(errs) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({ "error": "validation failed", "fields": errs }),
            ),
            AppError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, json!({ "error": "unauthorized" }))
            }
            AppError::Forbidden => (StatusCode::FORBIDDEN, json!({ "error": "forbidden" })),
            AppError::NotFound => (StatusCode::NOT_FOUND, json!({ "error": "not found" })),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, json!({ "error": msg })),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, json!({ "error": msg })),
            AppError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "internal error"}),
            ),
        };

        (status, Json(body)).into_response()
    }
}
