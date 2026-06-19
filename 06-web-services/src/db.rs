//! Database wiring: the pool (Pill 4 & 5) and migrations (Pill 6).
//!
//! Both functions are **step 3** — short, but they're the two things that turn
//! a `DATABASE_URL` string into a usable, migrated database.

use sqlx::postgres::{PgPool, PgPoolOptions};

/// Build the connection pool. Opens a handful of connections once and shares
/// them for the process's life — never one-per-request.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

/// Apply any not-yet-run migrations from `migrations/`, idempotently.
///
/// `sqlx::migrate!()` embeds the `migrations/` directory at compile time; its
/// `.run(pool)` applies each pending file in a transaction and records it, so
/// calling this on every boot is safe.
///
/// TODO (step 3): `sqlx::migrate!().run(pool).await` (map the error — the
/// `migrate::MigrateError` converts into `sqlx::Error`).
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!()
        .run(pool)
        .await
        .map_err(|e| sqlx::Error::Migrate(Box::new(e)))
}
