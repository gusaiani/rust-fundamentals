// Demonstrates `envtyped` from inside an `anyhow::Result<()>` main.
//
// Run with the schema satisfied:
//
//   PORT=8080 DATABASE_URL=postgres://x WORKERS=4 \
//     cargo run --example load_app_config
//
// Run with something wrong (missing PORT, garbage WORKERS) to see the
// structured error list:
//
//   WORKERS=not-a-number cargo run --example load_app_config

use anyhow::{Context, Result};
use envtyped::Loader;

#[derive(Debug)]
#[allow(dead_code)]
struct AppConfig {
    port: u16,
    database_url: String,
    log_level: String,
    workers: usize,
}

fn load_config() -> Result<AppConfig> {
    let env = Loader::new()
        .require::<u16>("PORT")
        .require::<String>("DATABASE_URL")
        .optional_or::<String>("LOG_LEVEL", "info".into())
        .require::<usize>("WORKERS")
        .load()
        .map_err(|errors| {
            // `.load()` gives Vec<Error>; anyhow wants one error. Join each
            // error's Display into a single bulleted block. No itertools —
            // std's map→collect→join keeps the dep list minimal (Pill 14)
            let joined = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n  - ");
            anyhow::anyhow!("config errors:\n  - {joined}")
        })?;

    // Each field's declared type drives `Env::get`'s `T` inference:
    // `port: u16` => get::<u16>, `database_url: String` => get::<String>, etc.
    let config = AppConfig {
        port: env.get("PORT")?,
        database_url: env.get("DATABASE_URL")?,
        log_level: env.get("LOG_LEVEL")?,
        workers: env.get("WORKERS")?,
    };

    Ok(config)
}

fn main() -> Result<()> {
    let cfg = load_config().context("loading app config")?;
    println!("loaded: {cfg:?}");
    Ok(())
}
