// Demonstrates `envguard` from inside an `anyhow::Result<()>` main.
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

// TODO: import what you need from envguard.
// use envguard::Loader;

#[derive(Debug)]
#[allow(dead_code)]
struct AppConfig {
    port: u16,
    database_url: String,
    log_level: String,
    workers: usize,
}

fn load_config() -> Result<AppConfig> {
    // TODO:
    //   1. Build the schema with Loader::new() and the right require/
    //      optional_or calls.
    //   2. Call .load(). If it returns Err(errors), join them into a single
    //      anyhow error so all problems print in one shot — something like
    //      `anyhow::bail!("config errors:\n  - {}", errors.iter().join("\n  - "))`.
    //   3. Pull each value out of the resulting Env with .get::<T>(name)?
    //      and assemble the AppConfig.
    todo!("wire envguard up")
}

fn main() -> Result<()> {
    let cfg = load_config().context("loading app config")?;
    println!("loaded: {cfg:?}");
    Ok(())
}
