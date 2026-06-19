//! Domain rows and the request/response DTOs that cross the HTTP boundary.
//!
//! This file is **given**. Two kinds of types live here:
//!
//! - **Rows** (`User`, `Task`) derive [`sqlx::FromRow`] — `query_as` maps a
//!   Postgres row straight into them. They mirror the `migrations/` schema.
//! - **DTOs** derive `serde` `Deserialize` (request bodies) or `Serialize`
//!   (responses). Keeping these separate from the rows is deliberate: a
//!   `User` row holds the `password_hash`, but no response type ever does, so
//!   it's impossible to leak it by serializing the wrong struct.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user row. Note `password_hash` lives here and in **no** response DTO —
/// that's how it never reaches the wire.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

/// A task row, owned by exactly one user. Serializable so handlers can return
/// it directly — but `user_id` is skipped on the wire (the client is already
/// scoped to itself; exposing owner ids leaks nothing useful and invites
/// confusion).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub user_id: Uuid,
    pub title: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
}

/// `POST /auth/register` and `/auth/login` body.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

/// `POST /auth/login` body (same shape as register, named for clarity at the
/// call site).
pub type LoginRequest = RegisterRequest;

/// What `/auth/register` returns: the created identity, never the hash.
#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        UserResponse { id: u.id, email: u.email }
    }
}

/// What `/auth/login` returns: a bearer token to put in `Authorization`.
#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub token_type: &'static str,
}

impl TokenResponse {
    pub fn bearer(token: String) -> Self {
        TokenResponse { token, token_type: "Bearer" }
    }
}

/// `POST /tasks` body.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateTask {
    pub title: String,
}

/// `PATCH /tasks/{id}` body — both fields optional (partial update).
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub done: Option<bool>,
}
