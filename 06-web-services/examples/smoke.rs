//! A worked end-to-end client — register, log in, create, list — against a
//! running `taskline` server. **Given**, like Module 5's `echo_server`: read it
//! as the happy path the integration tests assert in-process.
//!
//! ```text
//! # terminal 1
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo run
//! # terminal 2
//! cargo run --example smoke
//! ```
//!
//! It hits `BASE` (default http://127.0.0.1:8080) and prints each step. A fresh
//! random email keeps re-runs from colliding on the UNIQUE constraint.

use serde_json::json;

const BASE: &str = "http://127.0.0.1:8080";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = reqwest::Client::new();
    // Unique-ish email per run without pulling extra deps: nanosecond clock.
    let email = format!(
        "smoke+{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let creds = json!({ "email": email, "password": "hunter2!pw" });

    // 1. Register.
    let res = http.post(format!("{BASE}/auth/register")).json(&creds).send().await?;
    println!("register -> {}", res.status());

    // 2. Log in, capture the token.
    let res = http.post(format!("{BASE}/auth/login")).json(&creds).send().await?;
    println!("login    -> {}", res.status());
    let token = res.json::<serde_json::Value>().await?["token"]
        .as_str()
        .ok_or("no token in login response")?
        .to_string();

    // 3. Create a task (authenticated).
    let res = http
        .post(format!("{BASE}/tasks"))
        .bearer_auth(&token)
        .json(&json!({ "title": "smoke-test task" }))
        .send()
        .await?;
    println!("create   -> {}", res.status());

    // 4. List tasks.
    let res = http.get(format!("{BASE}/tasks")).bearer_auth(&token).send().await?;
    println!("list     -> {}", res.status());
    let tasks = res.json::<serde_json::Value>().await?;
    println!("tasks    -> {tasks}");

    Ok(())
}
