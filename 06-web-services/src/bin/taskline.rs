//! `taskline` server entrypoint.
//!
//! This file is **given** — the worked main, like `aprobe.rs` in Module 5.
//! Read it as the assembly order: config → tracing → pool → migrate → state →
//! router → serve. It compiles against the stubbed library, but every request
//! will `todo!()`-panic until you implement the handlers — that's expected.
//!
//! ```text
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \
//! JWT_SECRET=dev-secret cargo run
//! ```

use std::process::ExitCode;

use taskline::app::{build_app, AppState};
use taskline::auth::AuthConfig;
use taskline::config::Config;
use taskline::db;

#[tokio::main]
async fn main() -> ExitCode {
    // Structured logging: `RUST_LOG=taskline=debug,tower_http=debug` to see the
    // per-request spans the TraceLayer emits (Pill 10; deepened in Module 12).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "taskline=info,tower_http=info".into()),
        )
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::from_env();

    // Pool + migrations (Pills 5 & 6). On a fresh database this creates the
    // schema; on an existing one it's a no-op.
    let pool = db::connect(&cfg.database_url).await?;
    db::run_migrations(&pool).await?;

    let state = AppState {
        pool,
        auth: AuthConfig::new(&cfg.jwt_secret, cfg.token_ttl_secs),
    };

    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    tracing::info!("taskline listening on http://{}", cfg.bind_addr);

    // Graceful shutdown — the same Ctrl-C-to-drain idea you built by hand in
    // Module 5, here handed to axum: it stops accepting and lets in-flight
    // requests finish before returning.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown requested, draining…");
}
