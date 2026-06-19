//! Full-stack tests: they drive the **real** router (no mocks) against a
//! **real** Postgres, and go green as you implement each step.
//!
//! `#[sqlx::test]` is the engine (Pill 15): for each test it creates a fresh,
//! isolated database, runs `migrations/` into it, and hands you a clean `PgPool`
//! — so tests can't pollute each other. It needs `DATABASE_URL` pointing at a
//! Postgres it can create databases on (see the README's Local setup).
//!
//! We send requests with `tower::ServiceExt::oneshot` — feeding a `Request`
//! straight into the `Router` and reading the `Response`, no socket involved.
//!
//! What we prove:
//!   1. register → login → create → list happy path
//!   2. `/tasks` rejects unauthenticated requests (401)
//!   3. a user cannot read another user's task (404, not 403 — Pill 12)
//!   4. a duplicate email is a 409 (Pill 13)
//!   5. a too-short password is a 422 (Pill 11)

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt; // oneshot

use taskline::app::{build_app, AppState};
use taskline::auth::AuthConfig;

fn app(pool: PgPool) -> Router {
    let state = AppState {
        pool,
        auth: AuthConfig::new("test-secret", 3600),
    };
    build_app(state)
}

fn post_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn post_json_auth(uri: &str, body: Value, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn get_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn send(router: Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = router.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// Register a user and return a logged-in bearer token.
async fn register_and_login(router: &Router, email: &str) -> String {
    let creds = json!({ "email": email, "password": "hunter2!pw" });

    let (status, _) = send(router.clone(), post_json("/auth/register", creds.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "register should 201");

    let (status, body) = send(router.clone(), post_json("/auth/login", creds)).await;
    assert_eq!(status, StatusCode::OK, "login should 200");
    body["token"].as_str().expect("token in login response").to_string()
}

#[sqlx::test]
async fn register_login_create_list(pool: PgPool) {
    let router = app(pool);
    let token = register_and_login(&router, "alice@example.com").await;

    // Create a task.
    let (status, task) = send(
        router.clone(),
        post_json_auth("/tasks", json!({ "title": "ship module 6" }), &token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(task["title"], "ship module 6");
    assert_eq!(task["done"], false);

    // List tasks — exactly the one we created.
    let (status, list) = send(router.clone(), get_auth("/tasks", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["title"], "ship module 6");
}

#[sqlx::test]
async fn tasks_require_auth(pool: PgPool) {
    let router = app(pool);
    // No Authorization header → the AuthUser extractor rejects with 401.
    let req = Request::builder().method("GET").uri("/tasks").body(Body::empty()).unwrap();
    let (status, _) = send(router, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn cannot_read_another_users_task(pool: PgPool) {
    let router = app(pool);
    let alice = register_and_login(&router, "alice@example.com").await;
    let bob = register_and_login(&router, "bob@example.com").await;

    // Alice creates a task.
    let (_, task) = send(
        router.clone(),
        post_json_auth("/tasks", json!({ "title": "alice secret" }), &alice),
    )
    .await;
    let id = task["id"].as_str().unwrap();

    // Bob asks for it by id → 404 (owner-scoped; we don't even admit it exists).
    let (status, _) = send(router.clone(), get_auth(&format!("/tasks/{id}"), &bob)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "owner isolation (Pill 12)");

    // Alice still sees it.
    let (status, _) = send(router, get_auth(&format!("/tasks/{id}"), &alice)).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn duplicate_email_conflicts(pool: PgPool) {
    let router = app(pool);
    let creds = json!({ "email": "dup@example.com", "password": "hunter2!pw" });

    let (first, _) = send(router.clone(), post_json("/auth/register", creds.clone())).await;
    assert_eq!(first, StatusCode::CREATED);

    let (second, _) = send(router, post_json("/auth/register", creds)).await;
    assert_eq!(second, StatusCode::CONFLICT, "duplicate email → 409 (Pill 13)");
}

#[sqlx::test]
async fn short_password_is_unprocessable(pool: PgPool) {
    let router = app(pool);
    let (status, _) = send(
        router,
        post_json("/auth/register", json!({ "email": "x@example.com", "password": "short" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "validation → 422 (Pill 11)");
}
