//! Shared state and the router assembly.
//!
//! `AppState` and its `FromRef` impl are **given**; `build_app` (the route +
//! middleware wiring) is **step 6**.

use axum::extract::FromRef;
use axum::Router;
use sqlx::PgPool;

use crate::auth::AuthConfig;

/// Everything a handler can reach via `State<AppState>` (Pill 4). `Clone` is
/// cheap: `PgPool` is an `Arc`, `AuthConfig` holds small keys.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthConfig,
}

/// Lets the `AuthUser` extractor pull just the `AuthConfig` sub-state out of
/// `AppState` (Pill 9) — `AuthConfig::from_ref(state)`.
impl FromRef<AppState> for AuthConfig {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

/// Assemble the application router: the nine routes, the `tower` middleware
/// stack, and the shared state.
///
/// TODO (step 6):
/// ```ignore
/// use axum::routing::{get, post};
/// use tower_http::{trace::TraceLayer, timeout::TimeoutLayer};
/// use std::time::Duration;
/// use crate::{handlers, openapi};
///
/// Router::new()
///     .route("/health", get(|| async { "ok" }))
///     .route("/openapi.json", get(openapi::openapi))
///     .route("/auth/register", post(handlers::register))
///     .route("/auth/login", post(handlers::login))
///     .route("/tasks", get(handlers::list_tasks).post(handlers::create_task))
///     .route("/tasks/{id}", get(handlers::get_task)       // axum 0.8: {id}, not :id
///                          .patch(handlers::update_task)
///                          .delete(handlers::delete_task))
///     .layer(TraceLayer::new_for_http())                  // outermost-added = outermost (Pill 10)
///     .layer(TimeoutLayer::new(Duration::from_secs(10)))
///     .with_state(state)
/// ```
pub fn build_app(state: AppState) -> Router {
    let _ = state;
    todo!("wire routes + layers + with_state")
}
