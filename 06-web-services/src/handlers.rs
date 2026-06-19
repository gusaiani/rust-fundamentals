//! The route handlers — **step 5**, the heart of the project.
//!
//! Each is thin by design: validate the input (Pill 11), run *one*
//! owner-scoped query (Pill 12), return a row. The signatures are given and
//! correct (note the body extractor `Json<..>` comes **last**, Pill 2); the
//! bodies are `todo!()`.
//!
//! Two disciplines to hold throughout:
//!   - **validate first**: `body.validate()?` before any DB work.
//!   - **scope every task query by `auth.user_id`**: `... AND user_id = $n`, so
//!     someone else's id is a clean 404, never a leak.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::{
    CreateTask, LoginRequest, RegisterRequest, Task, TokenResponse, UpdateTask, User, UserResponse,
};
use crate::validation::Validate;
use crate::AppState;

/// `POST /auth/register` -> 201 `{id, email}`.
///
/// TODO (step 5): `body.validate()?` → `auth::hash_password(&body.password)?`
/// → `INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING *`
/// (`query_as::<_, User>`). A duplicate email trips the UNIQUE constraint and
/// surfaces as `AppError::Conflict` (409) via `From<sqlx::Error>`. Return
/// `(StatusCode::CREATED, Json(UserResponse::from(user)))`.
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    let _ = (&state.pool, body.email, body.password);
    todo!("validate, hash, insert user; 409 on duplicate email")
}

/// `POST /auth/login` -> 200 `{token, token_type}`.
///
/// TODO (step 5): look up the user by email
/// (`SELECT * FROM users WHERE email = $1`, `fetch_optional`) → if missing OR
/// `auth::verify_password` is false, return `AppError::Unauthorized` (don't
/// reveal *which* was wrong) → else `state.auth.issue(user.id)?` and return
/// `Json(TokenResponse::bearer(token))`.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let _ = (&state.pool, &state.auth, body.email, body.password);
    todo!("verify credentials, issue a JWT")
}

/// `GET /tasks` -> 200 `[Task, ...]` (only the caller's).
///
/// TODO (step 5): `SELECT * FROM tasks WHERE user_id = $1 ORDER BY created_at`,
/// `fetch_all`, return `Json(tasks)`.
pub async fn list_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Task>>, AppError> {
    let _ = (&state.pool, auth.user_id);
    todo!("list the caller's tasks")
}

/// `POST /tasks` -> 201 `Task`.
///
/// TODO (step 5): `body.validate()?` →
/// `INSERT INTO tasks (user_id, title) VALUES ($1, $2) RETURNING *` →
/// `(StatusCode::CREATED, Json(task))`.
pub async fn create_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<Task>), AppError> {
    let _ = (&state.pool, auth.user_id, body.title);
    todo!("validate + insert a task owned by auth.user_id")
}

/// `GET /tasks/{id}` -> 200 `Task` (404 if not yours).
///
/// TODO (step 5): `SELECT * FROM tasks WHERE id = $1 AND user_id = $2`,
/// `fetch_one` (RowNotFound -> 404). Return `Json(task)`.
pub async fn get_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, AppError> {
    let _ = (&state.pool, auth.user_id, id);
    todo!("fetch one owner-scoped task")
}

/// `PATCH /tasks/{id}` -> 200 `Task`.
///
/// TODO (step 5): `body.validate()?`, then a partial update scoped to the
/// owner. The tidy SQL trick is COALESCE so absent fields keep their value:
/// `UPDATE tasks SET title = COALESCE($1, title), done = COALESCE($2, done)
///  WHERE id = $3 AND user_id = $4 RETURNING *` (RowNotFound -> 404).
pub async fn update_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<Task>, AppError> {
    let _ = (&state.pool, auth.user_id, id, body.title, body.done);
    todo!("validate + partial-update an owner-scoped task")
}

/// `DELETE /tasks/{id}` -> 204.
///
/// TODO (step 5): `DELETE FROM tasks WHERE id = $1 AND user_id = $2` with
/// `execute`; if `rows_affected() == 0` return `AppError::NotFound`, else
/// `StatusCode::NO_CONTENT`.
pub async fn delete_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let _ = (&state.pool, auth.user_id, id);
    todo!("delete an owner-scoped task; 404 if it didn't exist")
}
