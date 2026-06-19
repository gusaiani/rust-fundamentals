//! Process configuration, read once from the environment.
//!
//! This file is **given** — it's scaffolding, not the exercise. Twelve-factor
//! style: config comes from the environment, with defaults that make `cargo
//! run` work against the Docker Postgres from the README.

/// Everything the process needs to boot, resolved from env vars.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string, e.g. `postgres://user:pass@host:5432/db`.
    pub database_url: String,
    /// HMAC secret used to sign and verify JWTs. **Must** be overridden in
    /// production — a leaked secret means anyone can forge tokens.
    pub jwt_secret: String,
    /// Where the HTTP server binds, e.g. `127.0.0.1:8080`.
    pub bind_addr: String,
    /// How long an issued JWT stays valid, in seconds.
    pub token_ttl_secs: i64,
}

impl Config {
    /// Load config from the environment, falling back to dev defaults.
    ///
    /// Only `DATABASE_URL` is truly required to do anything useful; the rest
    /// default so the README's quickstart works with one export.
    pub fn from_env() -> Self {
        Config {
            database_url: env_or(
                "DATABASE_URL",
                "postgres://postgres:postgres@localhost:5432/postgres",
            ),
            jwt_secret: env_or("JWT_SECRET", "dev-secret-change-me"),
            bind_addr: env_or("BIND_ADDR", "127.0.0.1:8080"),
            token_ttl_secs: env_or("TOKEN_TTL_SECS", "3600")
                .parse()
                .unwrap_or(3600),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
