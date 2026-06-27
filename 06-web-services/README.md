# taskline

A production-grade REST API in Rust: register, log in for a JWT, and manage your own tasks.

`taskline` is a JSON API built on `axum` + `tower`, backed by Postgres via `sqlx`. Users register with an email and password (hashed with argon2), log in to receive a JWT, and then perform owner-scoped CRUD over their tasks. Every protected route verifies the bearer token through a custom extractor, every task query is scoped to the authenticated user, and errors map to honest status codes through one `IntoResponse`. A hand-rolled OpenAPI document is served at `/openapi.json`.

## What it does

- Email/password registration with salted, memory-hard argon2 password hashing.
- Stateless authentication: login mints a signed JWT; every protected request verifies it.
- Per-user task CRUD — list, create, read, update, delete — each query scoped by `user_id`.
- Request validation at the boundary, returning `422` with the failing fields.
- A machine-readable OpenAPI spec at `/openapi.json` and a `/health` probe.

## Features

- `axum` 0.8 router with extractor-based handlers (`{id}` path syntax).
- A custom `AuthUser` `FromRequestParts` extractor — protection is enforced by the handler signature.
- `tower-http` middleware: per-request tracing span + a 10s request timeout.
- `sqlx` `PgPool` in shared state, parameterized queries, and embedded forward-only migrations applied on boot.
- One unified `AppError` → `IntoResponse` that never leaks internal detail.
- Owner-scoped queries (`AND user_id = $n`) so someone else's id is a clean `404`.
- A `criterion` benchmark comparing argon2 verify vs JWT verify.

## API

| Method | Path | Body | Auth | Response |
| --- | --- | --- | --- | --- |
| `POST` | `/auth/register` | `{email, password}` | no | `201 {id, email}` (`409` on duplicate email) |
| `POST` | `/auth/login` | `{email, password}` | no | `200 {token, token_type}` (`401` on bad creds) |
| `GET` | `/tasks` | — | Bearer | `200 [Task, ...]` (only yours) |
| `POST` | `/tasks` | `{title}` | Bearer | `201 Task` |
| `GET` | `/tasks/{id}` | — | Bearer | `200 Task` (`404` if not yours) |
| `PATCH` | `/tasks/{id}` | `{title?, done?}` | Bearer | `200 Task` |
| `DELETE` | `/tasks/{id}` | — | Bearer | `204` (`404` if not yours) |
| `GET` | `/health` | — | no | `200 "ok"` |
| `GET` | `/openapi.json` | — | no | `200 <OpenAPI spec>` |

A `Task` serializes as `{id, title, done, created_at}` (the `user_id` is never put on the wire). Pass the JWT as `Authorization: Bearer <token>`.

Validation rules: `register` requires an email containing `@` and a password of at least 8 characters; task `title` must be non-empty after trimming and at most 200 characters.

## Configuration

All config is read once from the environment at boot (`src/config.rs`). Every var has a dev default, so `cargo run` works against the Docker Postgres below with no setup.

| Variable | Required? | Default | Meaning |
| --- | --- | --- | --- |
| `DATABASE_URL` | no | `postgres://postgres:postgres@localhost:5432/postgres` | Postgres connection string. |
| `JWT_SECRET` | no | `dev-secret-change-me` | HMAC secret used to sign and verify JWTs. **Override in production.** |
| `BIND_ADDR` | no | `127.0.0.1:8080` | Address the HTTP server binds to. |
| `TOKEN_TTL_SECS` | no | `3600` | JWT lifetime in seconds. |
| `RUST_LOG` | no | `taskline=info,tower_http=info` | `tracing` env-filter for log verbosity. |

No variable is strictly required to boot, but `DATABASE_URL` must point at a reachable Postgres for anything useful to happen.

## Running it

```bash
# 1. a throwaway Postgres
docker run --name taskline-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres:16

# 2. point the app at it (these match the defaults, shown for clarity)
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
export JWT_SECRET=dev-secret-change-me

# 3. run — migrations are embedded and applied automatically on boot
cargo run --bin taskline
# -> taskline listening on http://127.0.0.1:8080
```

Migrations run on startup: `db::run_migrations` calls `sqlx::migrate!()`, which embeds `migrations/` into the binary and applies any not-yet-applied files inside a transaction. There is no separate `sqlx migrate run` step.

Drive the auth flow with curl:

```bash
curl -s localhost:8080/auth/register \
  -H 'content-type: application/json' \
  -d '{"email":"a@b.com","password":"hunter2!"}'

TOKEN=$(curl -s localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"a@b.com","password":"hunter2!"}' | jq -r .token)

curl -s localhost:8080/tasks \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"title":"ship module 6"}'

curl -s localhost:8080/tasks -H "authorization: Bearer $TOKEN"
```

There's also a worked end-to-end client:

```bash
cargo run --example smoke      # runs the full register -> login -> CRUD flow against a live server
```

Tests and the benchmark:

```bash
cargo test     # #[sqlx::test] creates an isolated Postgres database per test and migrates it
cargo bench    # bench "auth": argon2 verify vs JWT verify — the ~1000x ratio that justifies tokens
```

`cargo test` needs `DATABASE_URL` to point at a server it can create databases on; it makes a fresh database per test, so tests never collide and the `cargo run` data is untouched.

## How it works

- **Routing & extractors (`app.rs`, `handlers.rs`):** `build_app` maps each route to an async handler whose arguments are extractors. Body-consuming `Json<..>` comes last; `State`, `Path`, and `AuthUser` come first.
- **Auth (`auth.rs`):** `hash_password`/`verify_password` use argon2; `AuthConfig::issue`/`verify` use `jsonwebtoken`. The `AuthUser` extractor reads `Authorization: Bearer`, verifies the JWT, and yields `user_id` — so a protected handler simply takes `AuthUser`.
- **Data (`db.rs`, `models.rs`, `migrations/`):** a `PgPool` lives in `AppState` (cheap to clone — it's an `Arc`). Rows (`User`, `Task`) derive `FromRow`; request/response DTOs are separate so a `password_hash` can never be serialized.
- **Errors & validation (`error.rs`, `validation.rs`):** every handler returns `Result<T, AppError>`; `From` conversions map `sqlx`/`jsonwebtoken`/validation failures, and one `IntoResponse` decides each status code while hiding internals.
- **Middleware (`tower-http`):** a `TraceLayer` and a `TimeoutLayer` (10s, returning `408`) wrap the whole router. The binary serves with graceful shutdown on Ctrl-C.

## Project layout

```text
src/
  bin/taskline.rs   server entrypoint: config -> pool -> migrate -> router -> serve
  config.rs         env config (Config::from_env)
  app.rs            AppState + build_app (router + middleware)
  handlers.rs       register, login, task CRUD
  auth.rs           argon2 hashing, JWT issue/verify, AuthUser extractor
  models.rs         User/Task rows + request/response DTOs
  error.rs          AppError + From conversions + IntoResponse
  validation.rs     Validate trait + DTO rules
  db.rs             connect + run_migrations
  openapi.rs        hand-rolled OpenAPI document
migrations/0001_init.sql   users + tasks schema
benches/auth.rs            argon2 vs JWT verify (bench "auth")
examples/smoke.rs          end-to-end client
tests/integration.rs       full-stack tests on #[sqlx::test]
```

## Status

Implemented and runnable; needs a reachable Postgres.

The concept pills and the step-by-step build that produced this — covering `axum` routing and extractors, `tower`/`tower-http` middleware, `sqlx` and forward-only migrations, argon2 password hashing, JWT auth, and a hand-rolled OpenAPI document — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [MIT license](https://opensource.org/licenses/MIT) or [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
</content>
</invoke>
