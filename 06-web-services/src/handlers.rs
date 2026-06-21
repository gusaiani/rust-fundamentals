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
    body.validate()?;
    let password_hash = crate::auth::hash_password(&body.password)?;

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING *",
    )
    .bind(&body.email)
    .bind(&password_hash)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
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
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&body.email)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !crate::auth::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let token = state.auth.issue(user.id)?;
    Ok(Json(TokenResponse::bearer(token)))
}

/// `GET /tasks` -> 200 `[Task, ...]` (only the caller's).
pub async fn list_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Task>>, AppError> {
    let tasks =
        sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE user_id = $1 ORDER BY created_at")
            .bind(auth.user_id)
            .fetch_all(&state.pool)
            .await?;

    Ok(Json(tasks))
}

/// `POST /tasks` -> 201 `Task`.
pub async fn create_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<Task>), AppError> {
    body.validate()?;

    let task =
        sqlx::query_as::<_, Task>("INSERT INTO tasks (user_id, title) VALUES ($1, $2) RETURNING *")
            .bind(auth.user_id)
            .bind(&body.title)
            .fetch_one(&state.pool)
            .await?;

    Ok((StatusCode::CREATED, Json(task)))
}

/// `GET /tasks/{id}` -> 200 `Task` (404 if not yours).
pub async fn get_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, AppError> {
    let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(task))
}

/// `PATCH /tasks/{id}` -> 200 `Task`.
pub async fn update_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<Task>, AppError> {
    let task = sqlx::query_as::<_, Task>(
        "UPDATE tasks SET title = COALESCE($1, title), done = COALESCE($2, done) \
        WHERE id = $3 and user_id = $4 RETURNING *",
    )
    .bind(&body.title)
    .bind(body.done)
    .bind(id)
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(task))
}

/// `DELETE /tasks/{id}` -> 204.
pub async fn delete_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
