# Web Services in Rust — in 5-Minute Pills

## Goal

Build a **production-grade REST API** — user registration, password login that mints a JWT, and per-user CRUD over a resource — backed by Postgres, with request validation, tower middleware, a hand-rolled OpenAPI document, and a benchmark that *explains the auth design instead of asserting it*. By the end you can read a request from socket to row and back, and say exactly which layer owns which failure mode.

## Time estimate

~1 day (15 pills × 5 min + project)

## What you'll learn

- The shape of an `axum` service — `Router`, async handlers, and the `FromRequest`/`FromRequestParts` **extractor** model that turns "parse the request" into the type system's job
- One unified `AppError` → `IntoResponse` so every handler is just `-> Result<T, AppError>` and `?` does the right thing
- `sqlx` against Postgres — a `PgPool` in shared state, parameterized queries that can't be injected, `FromRow`, and *forward-only migrations*
- Real password storage (`argon2`, salted, memory-hard) and stateless auth with **JWTs** — issue once at login, verify cheaply per request
- The **auth extractor**: a custom `FromRequestParts` that verifies the bearer token, so a protected handler just takes `AuthUser` as an argument and the compiler enforces it
- `tower`/`tower-http` middleware — tracing, timeouts, and layer ordering — cross-cutting concerns without touching a single handler
- The difference between **authentication and authorization** (and the one-line `WHERE user_id = $1` that prevents the most common API vulnerability)
- Why a benchmark says "hash at login, not per request": argon2 verify is ~milliseconds, JWT verify is ~microseconds — the whole reason stateless tokens exist

## Concepts

### Pill 1: The Shape of an `axum` Service

`axum` is a web framework built on `tower` and `hyper`. The entire mental model: a **`Router`** maps a method+path to a **handler** — an `async fn` whose arguments are **extractors** (pulled *from* the request) and whose return type is anything that's **`IntoResponse`**.

```rust
async fn health() -> &'static str { "ok" }

let app = Router::new()
    .route("/health", get(health))
    .route("/tasks", get(list_tasks).post(create_task));

let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
axum::serve(listener, app).await?;   // hyper drives it on the tokio runtime
```

There's no magic dispatcher object, no controllers, no annotations. A handler is a plain function; the framework's job is to *call it with the right arguments and turn its return value into bytes*. Everything else in this module is "what can be an argument" (extractors, Pills 2 & 9) and "what can be a return value" (`IntoResponse`, Pill 3). Note `axum::serve` takes a tokio `TcpListener` — the same primitive you bound by hand in Module 5; the async runtime underneath is identical.

### Pill 2: Extractors — Parsing as a Type

An **extractor** is any type implementing `FromRequest` (consumes the body) or `FromRequestParts` (reads only headers/URI, leaves the body). axum runs them left-to-right against the incoming request:

```rust
async fn create_task(
    State(state): State<AppState>,   // shared app state (the pool, keys)
    auth: AuthUser,                  // your custom extractor (Pill 9) — verifies the JWT
    Json(body): Json<CreateTask>,    // deserialize the JSON body into CreateTask
) -> Result<Json<Task>, AppError> { /* ... */ }
```

Each parameter is independently fallible: if `Json` can't deserialize, the handler is *never called* — axum returns the extractor's rejection (a `400`/`422`). This is the framework's best idea: **request parsing is pushed to the boundary and expressed in the signature.** A handler that takes `Json<CreateTask>` *cannot* run with a malformed body. One rule you'll trip on once: a body-consuming extractor (`Json`, `String`, `Bytes`) must be **last** in the argument list — there's only one body and it gets consumed. Everything `FromRequestParts` (`State`, `Path`, `Query`, headers, `AuthUser`) comes first.

### Pill 3: `IntoResponse` and One Error Type

A handler returns something that can become an HTTP response. `Json<T>`, `(StatusCode, Json<T>)`, `String`, `StatusCode` all implement `IntoResponse`. The decisive move is making your **error** type implement it too, so handlers return `Result<T, AppError>` and `?` works:

```rust
pub enum AppError { Validation(..), Unauthorized, NotFound, Conflict(String), Internal }

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound      => (StatusCode::NOT_FOUND, "not found".into()),
            AppError::Unauthorized  => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::Internal      => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
            /* ... */
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}
```

Now a handler reads like straight-line happy-path code — `let user = find_user(&pool, id).await?;` — and every `?` that bubbles an `AppError` is automatically rendered with the right status code. This is Module 3's error-handling discipline (`thiserror`, error enums) applied at the HTTP boundary: **one place decides how each failure becomes a status code**, and it's not scattered through 12 handlers.

### Pill 4: Shared State and the Connection Pool

Handlers are stateless functions, but they need the database pool and the signing keys. axum threads that through `State<S>`:

```rust
#[derive(Clone)]
pub struct AppState { pub pool: PgPool, pub auth: AuthConfig }

let app = Router::new().route(/* ... */).with_state(state);
//  handler: async fn h(State(state): State<AppState>) { state.pool ... }
```

`AppState` must be `Clone` — axum clones it per request — but that's cheap: a `PgPool` is an `Arc` around the real pool, so cloning bumps a refcount, it doesn't open connections. **Never open a connection per request.** A pool keeps N live connections and hands them out; opening a Postgres connection is a TCP+TLS+auth handshake costing milliseconds, and doing it per request is the single most common way to make a Rust API mysteriously slow. You configure the pool once (Pill 5) and share it for the process's life.

### Pill 5: `sqlx` — Typed, Injection-Proof Postgres

`sqlx` is an async, pure-Rust SQL toolkit. You build a pool once and run **parameterized** queries — values go through bind parameters (`$1`, `$2`), never string-formatted into SQL:

```rust
let pool = PgPoolOptions::new().max_connections(5).connect(&database_url).await?;

let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
    .bind(email)
    .fetch_optional(&pool)
    .await?;                                  // Option<User>
```

`query_as::<_, User>` maps each row into a `User` via `#[derive(sqlx::FromRow)]`. Because `email` is a *bound parameter*, there is no SQL injection surface — the value never touches the query string. (`'; DROP TABLE users; --` is just a string that doesn't match.) That's not a convention you have to remember; it's the only ergonomic way to pass a value.

**Compile-time-checked queries** are sqlx's headline feature: the `sqlx::query!` *macro* connects to your dev database **at compile time**, verifies the SQL, and checks that your Rust types match the columns — a typo in a column name is a *build error*, not a 500 at 3 a.m. The catch: it needs a reachable database (or a cached `.sqlx/` from `cargo sqlx prepare`) to build. This starter uses the **runtime** `query`/`query_as` API so it compiles with no database; converting to the checked `query!` macro is a stretch goal, and it's the one most worth doing.

### Pill 6: Migrations — Forward-Only Schema Evolution

Your schema lives in versioned SQL files, not in your head:

```text
migrations/
  0001_init.sql            -- CREATE TABLE users ...; CREATE TABLE tasks ...;
```

`sqlx::migrate!()` embeds that directory into the binary at compile time; `.run(&pool).await` applies any not-yet-applied files inside a transaction and records them in a `_sqlx_migrations` table, so it's idempotent — run it on every boot. Migrations are **forward-only and immutable once shipped**: you never edit `0001` after it's run in production (some row out there was created under the old shape); you add `0002_add_due_date.sql`. This is the same discipline as an append-only log — the database's current shape is the *replay* of every migration in order. The CLI (`cargo install sqlx-cli`; `sqlx migrate add`, `sqlx migrate run`) is how you author them.

### Pill 7: Never Store Passwords — Hash Them (`argon2`)

A password column stores a **verifier**, never the password. You hash with a slow, salted, memory-hard function so a database leak doesn't hand attackers the plaintext:

```rust
let salt = SaltString::generate(&mut OsRng);
let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?.to_string();
// stored string is self-describing: algorithm, params, salt, and digest, e.g.
//   $argon2id$v=19$m=19456,t=2,p=1$<salt>$<digest>
```

Three properties make this safe and `sha256(password)` catastrophic:

- **Salt** — a unique random salt per user means identical passwords produce different hashes, killing rainbow tables and "crack one, crack all."
- **Slow & memory-hard** — argon2 is *deliberately* expensive (tunable memory/time), so brute-forcing leaked hashes costs real money. A fast hash like SHA-256 is brute-forceable at billions/sec on a GPU.
- **Self-describing** — the stored string carries its own parameters, so you can raise cost over time and still verify old hashes.

Verification re-derives the hash from the candidate password and the stored salt/params and compares in constant time. You will *feel* this cost in Pill 15 — it's the whole reason tokens exist.

### Pill 8: JWTs — Stateless Authentication

A **JSON Web Token** is a signed, self-contained claim: `header.payload.signature`, each part base64url. The payload holds **claims** — here `sub` (the user id), `iat` (issued-at), `exp` (expiry). The server signs it with a secret (HMAC-SHA256); anyone can *read* a JWT, but only the holder of the secret can *forge* one.

```rust
let claims = Claims { sub: user_id, iat: now, exp: now + ttl };
let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))?;
// later, per request:
let data = decode::<Claims>(&token, &DecodingKey::from_secret(secret), &Validation::default())?;
//          ^ fails if the signature is wrong OR exp has passed
```

"Stateless" is the point: the server stores **nothing** per session. The token *is* the session — present it, the server verifies the signature and expiry, and trusts the `sub`. No session table, no Redis lookup, horizontally scalable for free. The trade-off is the flip side: you can't easily *revoke* a JWT before it expires (the server isn't tracking it), which is why expiries are short and revocation lists are a stretch goal. Mint it at login (Pill 7's expensive check happens *once*); verify it per request (cheap — Pill 15 measures exactly how much cheaper).

### Pill 9: The Auth Extractor — Protection as a Parameter

Here's where Pills 2 and 8 combine into the prettiest idea in the module. Implement `FromRequestParts` for an `AuthUser` type: it reads the `Authorization: Bearer <token>` header, verifies the JWT, and yields the user id — or rejects with `401`.

```rust
impl<S> FromRequestParts<S> for AuthUser
where AuthConfig: FromRef<S>, S: Send + Sync {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;
        let claims = AuthConfig::from_ref(state).verify(&token)?;
        Ok(AuthUser { user_id: claims.sub })
    }
}
```

Now **a handler is protected by its signature**:

```rust
async fn list_tasks(State(s): State<AppState>, auth: AuthUser) -> Result<Json<Vec<Task>>, AppError>
//                                              ^^^^^^^^^^^^^^ unauthenticated requests never reach the body
```

You cannot *forget* to check auth on a protected route — if the handler needs the user id, it takes `AuthUser`, and the extractor ran first. Forgetting the check isn't a subtle bug you might introduce; it's a parameter you didn't write, so the handler simply can't use `auth.user_id`. (`FromRef` lets the extractor pull just the `AuthConfig` sub-state out of `AppState` — that's why `AppState` implements `FromRef<AppState> for AuthConfig`.)

### Pill 10: `tower` Middleware — Cross-Cutting Concerns

`tower`'s `Service` trait is "an async function from request to response," and a `Layer` wraps a `Service` in another `Service`. That's the whole abstraction behind middleware: logging, timeouts, auth, rate limiting, compression — each is a layer, none touches your handlers.

```rust
use tower_http::{trace::TraceLayer, timeout::TimeoutLayer};

let app = Router::new()
    .route(/* ... */)
    .layer(TraceLayer::new_for_http())          // a tracing span + log per request
    .layer(TimeoutLayer::new(Duration::from_secs(10)))
    .with_state(state);
```

**Layer order is request order, outside-in.** The *last* `.layer()` added is the *outermost* — it sees the request first and the response last. Put the timeout outside the trace layer and a timed-out request still gets logged; flip them and you might not. `tower-http` ships the batteries (trace, timeout, compression, CORS, normalize-path); you rarely write a `Service` by hand. The payoff: "every request is traced and capped at 10s" is two lines in *one* place, not a copy-pasted preamble in every handler — the same separation `IntoResponse` gave errors, applied to behavior.

### Pill 11: Request Validation — Reject at the Boundary

Deserializing succeeds for plenty of *invalid* input: an empty title, a 4-character password, an email with no `@`. Validation is a second gate after parsing, and it returns **422 Unprocessable Entity** with which field failed and why — never a 500, never a database constraint doing your validation for you:

```rust
pub trait Validate { fn validate(&self) -> Result<(), ValidationErrors>; }

impl Validate for RegisterRequest {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut e = ValidationErrors::new();
        if !self.email.contains('@') { e.add("email", "must be a valid email"); }
        if self.password.len() < 8   { e.add("password", "must be at least 8 characters"); }
        e.into_result()
    }
}
```

We hand-roll a tiny `Validate` trait rather than pulling the `validator` crate — same ethos as Modules 4 and 5 hand-rolling their arg parsing: one fewer dependency, and you see there's no magic. The handler calls `body.validate()?` as its first line, so invalid data dies *before* a query runs. Validating at the edge keeps every layer below it dealing only in already-valid data — your SQL never has to defend against an empty string, because an empty string can't get that far.

### Pill 12: Authentication ≠ Authorization (the `WHERE user_id` discipline)

**Authentication** is "who are you" (the JWT, Pill 9). **Authorization** is "are you allowed to touch *this row*." Conflating them is the most common real-world API vulnerability — IDOR (Insecure Direct Object Reference):

```rust
// WRONG — authenticated, but not authorized:
"SELECT * FROM tasks WHERE id = $1"                       // any logged-in user reads ANY task
// RIGHT — scoped to the owner:
"SELECT * FROM tasks WHERE id = $1 AND user_id = $2"      // only your own
```

The first query is "authenticated": the caller had a valid token. It's still a breach, because user A can pass user B's task id and read it. The fix is doctrine, not cleverness: **every query over a user-owned resource carries `AND user_id = $auth` in its `WHERE`** (and `UPDATE`/`DELETE` too). A "not found" for someone else's row is the *correct* response — you don't even leak that the id exists. This is why `AuthUser` carries `user_id`: it's not decoration, it's the second half of every query in `handlers.rs`.

### Pill 13: Status Codes — Mapping Failure Honestly

REST clients act on status codes; getting them right is the API's contract. The ones this project uses, and when:

| Code | Meaning | In this API |
|------|---------|-------------|
| `200` / `201` | OK / Created | reads / successful `POST` |
| `204` | No Content | successful `DELETE` |
| `400` | malformed request | unparseable JSON |
| `401` | not authenticated | missing/invalid/expired token |
| `403` | authenticated, not allowed | (where you distinguish from 404) |
| `404` | not found | unknown id *or* someone else's row (Pill 12) |
| `409` | conflict | registering an email that already exists |
| `422` | semantically invalid | validation failed (Pill 11) |
| `500` | server fault | a bug, a dead database — **never** the client's fault |

Two rules that separate a toy from production: **`500` is always *your* fault** — if a client can trigger a 500 with bad input, you have a validation gap, not a server error. And **don't leak internals** — a `sqlx::Error` becomes a generic `500` with `"internal error"` *and a logged detail*, never the raw database message in the response body. The `From<sqlx::Error> for AppError` conversion is where this mapping lives: `RowNotFound → 404`, a unique-violation → `409`, everything else → `500`-and-log.

### Pill 14: OpenAPI — A Machine-Readable Contract

OpenAPI (formerly Swagger) is a JSON/YAML description of your API — every path, method, request shape, and response. It's what generates client SDKs, drives Swagger UI, and lets a frontend team build against your API before it's deployed. You can derive it (the `utoipa` crate) or hand-write it; this module serves a **hand-rolled** document at `/openapi.json`:

```rust
async fn openapi() -> Json<serde_json::Value> { Json(spec()) }   // spec() builds the document
```

Hand-rolling once shows you OpenAPI is *just a JSON document with an agreed schema* — `info`, `paths`, `components` — not a framework feature. The value isn't the file; it's that the contract is **machine-readable and versioned alongside the code**: a client can discover that `POST /auth/register` takes `{email, password}` and returns `201` without reading your source. (Wiring `utoipa` to derive it from your handler types, so the spec can't drift from the code, is a stretch goal.)

### Pill 15: Testing & the Benchmark That Explains the Design

**Testing a web service** has two layers. Pure logic — validation, JWT round-trips, password hashing — unit-tests with no I/O. Handlers need a database; the idiomatic tool is **`#[sqlx::test]`**, which spins up an **isolated database per test**, runs your migrations into it, and hands you a clean `PgPool` — so tests can't pollute each other and start from a known schema. You drive the app *without a socket* using `tower::ServiceExt::oneshot`, feeding a `Request` straight into the `Router` and asserting on the `Response`:

```rust
let res = app.oneshot(post("/auth/register", json)).await?;
assert_eq!(res.status(), StatusCode::CREATED);
```

**The benchmark is the deliverable, and here it argues for the architecture.** Bench two operations: `argon2` password verification and JWT verification. argon2 is *deliberately* ~1–10 ms (Pill 7); a JWT HMAC verify is ~microseconds. That ~1000× gap **is the reason Pill 8 exists**: if you re-checked the password on every request, your API would cap at a few hundred req/s/core on auth alone. Instead you pay the argon2 cost *once* at login, mint a JWT, and every subsequent request pays only the microsecond verify. The benchmark turns "stateless tokens are a performance decision, not just convenience" from a claim into two numbers with a 1000× ratio. (You can't `criterion`-bench the full handler cheaply — it needs a database — so we bench the CPU-bound auth primitives, which is exactly where the interesting cost lives.)

## Project: `taskline` — a production REST API

A JSON API where users register, log in for a JWT, and manage their own tasks:

```text
POST   /auth/register     {email, password}          -> 201 {id, email}
POST   /auth/login        {email, password}          -> 200 {token, token_type}
GET    /tasks                          (Bearer)       -> 200 [Task, ...]   (only yours)
POST   /tasks             {title}      (Bearer)       -> 201 Task
GET    /tasks/{id}                     (Bearer)       -> 200 Task          (404 if not yours)
PATCH  /tasks/{id}        {title?,done?}(Bearer)      -> 200 Task
DELETE /tasks/{id}                     (Bearer)       -> 204
GET    /health                                        -> 200 "ok"
GET    /openapi.json                                  -> 200 <spec>
```

Why it's the right vehicle for this module:

- **Every skill is load-bearing.** Routing & extractors (`axum`), a pool in shared state and migrations (`sqlx`), real password storage (`argon2`) and stateless tokens (JWT), a custom auth extractor, validation, middleware, and an OpenAPI contract — drop any one and the API is broken or insecure, not just less polished.
- **It's the canonical backend interview project, done right.** "REST API with JWT auth and Postgres" is on a thousand job posts; the difference between a toy and a hire is exactly the parts this module insists on — owner-scoped queries (Pill 12), honest status codes (Pill 13), errors that don't leak (Pill 3), and a benchmark that justifies the design (Pill 15).
- **The security pills are real, not decorative.** Salted memory-hard hashing and the `WHERE user_id` discipline are the two things that separate "works in the demo" from "doesn't get you breached." You implement both by hand.
- **Testable without a fake.** `#[sqlx::test]` gives a real, isolated Postgres per test, and `oneshot` drives the real router with no socket — so the tests exercise the actual stack, not a mock of it.

### Requirements

1. **Config** (`config.rs`, *given*): read `DATABASE_URL`, `JWT_SECRET`, `BIND_ADDR`, `TOKEN_TTL_SECS` from the environment with sane defaults.
2. **Migrations** (`migrations/0001_init.sql`, *given*): `users` (id, email unique, password_hash, created_at) and `tasks` (id, user_id FK, title, done, created_at).
3. **Models** (`models.rs`, *given*): `User`/`Task` rows (`FromRow`) and the request/response DTOs (`RegisterRequest`, `LoginRequest`, `TokenResponse`, `CreateTask`, `UpdateTask`).
4. **Errors** (`error.rs`): the `AppError` enum + `From` conversions are *given*; implement **`IntoResponse`** (the status-code mapping, Pills 3 & 13).
5. **Validation** (`validation.rs`): the `Validate` trait + `ValidationErrors` are *given*; implement `validate` for the request DTOs (Pill 11).
6. **DB** (`db.rs`): implement `connect` (build the `PgPool`) and `run_migrations` (Pills 5 & 6).
7. **Auth** (`auth.rs`): implement `hash_password`/`verify_password` (Pill 7), `issue`/`verify` for JWTs (Pill 8), and the `AuthUser` `FromRequestParts` extractor (Pill 9).
8. **Handlers** (`handlers.rs`): implement `register`, `login`, and the five task handlers — each task query **owner-scoped** (Pill 12), each input **validated** (Pill 11).
9. **App** (`app.rs`): assemble the `Router` — routes, `tower` layers, and `with_state` (Pills 1, 4, 10).
10. **OpenAPI** (`openapi.rs`, *given*): serve the hand-rolled spec at `/openapi.json` (Pill 14).
11. **Binary** (`src/bin/taskline.rs`, *given*): the worked entrypoint — load config, connect, migrate, build the app, serve with graceful shutdown.
12. **Benchmark** (`benches/auth.rs`, *given*): `argon2` verify vs JWT verify — the required deliverable (Pill 15).
13. **Tests** (`tests/integration.rs`, *given*): register → login → create → list → owner-isolation → auth-required, on an isolated `#[sqlx::test]` database.

### Starter files

- `Cargo.toml` — `axum`, `tokio`, `tower`/`tower-http`, `sqlx` (Postgres), `jsonwebtoken`, `argon2`, `serde`, `uuid`, `chrono`, `tracing`; `criterion` + `reqwest` dev-deps; `[[bin]]` and `[[bench]] harness = false` wired.
- `src/lib.rs` — module declarations + re-exports.
- `src/config.rs` — env config (*given*).
- `src/models.rs` — domain rows + request/response DTOs (*given*).
- `src/error.rs` — `AppError` + `From` conversions (*given*); `IntoResponse` **(stubbed)**.
- `src/validation.rs` — `Validate` trait + `ValidationErrors` (*given*); DTO impls **(stubbed)**.
- `src/db.rs` — `connect`, `run_migrations` **(stubbed)**.
- `src/auth.rs` — `AuthConfig`, `Claims`, hashing, JWT, `AuthUser` extractor **(stubbed)**.
- `src/handlers.rs` — `register`, `login`, task CRUD **(stubbed)**.
- `src/app.rs` — `AppState` + `FromRef` (*given*); `build_app` router assembly **(stubbed)**.
- `src/openapi.rs` — hand-rolled OpenAPI document (*given*).
- `src/bin/taskline.rs` — the worked entrypoint (*given*).
- `migrations/0001_init.sql` — schema (*given*).
- `benches/auth.rs` — argon2 vs JWT verify (*given*).
- `tests/integration.rs` — full-stack tests on `#[sqlx::test]` (*given*).
- `examples/smoke.rs` — a fully-written client that runs the auth flow against a live server (*given*).

### Local setup (you need a Postgres)

```bash
# 1. a throwaway Postgres (Docker is easiest)
docker run --name taskline-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres:16

# 2. point the app + sqlx at it
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
export JWT_SECRET=dev-secret-change-me

# 3. (optional) the sqlx CLI, for authoring migrations and offline mode
cargo install sqlx-cli --no-default-features --features postgres

cargo test          # #[sqlx::test] creates an isolated DB per test and migrates it
cargo run           # starts the server on BIND_ADDR (default 127.0.0.1:8080)
cargo bench         # argon2 vs JWT — the deliverable
```

`#[sqlx::test]` needs `DATABASE_URL` to point at a server it can create databases on; it makes a *fresh* database per test, so your tests never collide and the `cargo run` data is untouched.

### Your task

1. **Errors (`error.rs`)**: implement `IntoResponse for AppError` — match each variant to its `(StatusCode, body)`. Make the response body `{"error": "..."}`; for `Validation`, include the field errors.
2. **Validation (`validation.rs`)**: implement `validate` for `RegisterRequest` (email has `@`, password ≥ 8), `CreateTask` (title non-empty, ≤ 200), `UpdateTask` (if present, same title rule).
3. **DB (`db.rs`)**: `connect` = `PgPoolOptions::new().max_connections(5).connect(url)`; `run_migrations` = `sqlx::migrate!().run(pool)`.
4. **Auth (`auth.rs`)**: `hash_password`/`verify_password` with `argon2`; `AuthConfig::issue`/`verify` with `jsonwebtoken`; the `AuthUser` extractor (read bearer, `verify`, yield `user_id`).
5. **Handlers (`handlers.rs`)**: `register` (validate → hash → insert, map unique-violation to `409`); `login` (look up → verify password → issue token); task CRUD (validate, **owner-scope every query**, map `RowNotFound` to `404`).
6. **App (`app.rs`)**: `build_app` — route the nine endpoints, add `TraceLayer` + `TimeoutLayer`, `.with_state(state)`.
7. **Run it**: `cargo run`, then `cargo run --example smoke` (or the curl block below) drives the whole flow.
8. **Green the tests**: `cargo test`. **Read the benchmark**: `cargo bench` — note the argon2-vs-JWT ratio and re-read Pill 8.

### Hints

<details>
<summary>Hint for step 1 (IntoResponse)</summary>

`IntoResponse` is implemented for `(StatusCode, Json<T>)`, so build that tuple and call `.into_response()`. Keep the public body generic — leak nothing:

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AppError::Validation(errs) =>
                (StatusCode::UNPROCESSABLE_ENTITY, json!({ "error": "validation failed", "fields": errs })),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, json!({ "error": "unauthorized" })),
            AppError::NotFound     => (StatusCode::NOT_FOUND,    json!({ "error": "not found" })),
            AppError::Conflict(m)  => (StatusCode::CONFLICT,     json!({ "error": m })),
            AppError::BadRequest(m)=> (StatusCode::BAD_REQUEST,  json!({ "error": m })),
            AppError::Internal     => (StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": "internal error" })),
        };
        (status, Json(body)).into_response()
    }
}
```

The `Internal` arm deliberately hides detail — the *real* cause was already logged in the `From<sqlx::Error>` conversion (Pill 13).
</details>

<details>
<summary>Hint for step 4 (password hashing & the JWT round-trip)</summary>

argon2 0.5, the salt-generate / verify incantation:

```rust
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, PasswordHash, rand_core::OsRng};

pub fn hash_password(plain: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(plain.as_bytes(), &salt)
        .map_err(|_| AppError::Internal)?.to_string())
}
pub fn verify_password(plain: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash).map_err(|_| AppError::Internal)?;
    Ok(Argon2::default().verify_password(plain.as_bytes(), &parsed).is_ok())
}
```

JWT (jsonwebtoken 9): `exp`/`iat` are seconds since epoch as `usize`. `Validation::default()` checks the signature *and* `exp` for you, so an expired token fails `verify` automatically:

```rust
let exp = (Utc::now() + Duration::seconds(self.ttl_secs)).timestamp() as usize;
let claims = Claims { sub: user_id, iat: Utc::now().timestamp() as usize, exp };
encode(&Header::default(), &claims, &self.encoding)        // -> String
decode::<Claims>(token, &self.decoding, &Validation::default())  // -> Err on bad sig OR expiry
```
</details>

<details>
<summary>Hint for step 4 (the AuthUser extractor)</summary>

In axum 0.8, `FromRequestParts` uses a native `async fn` — no `#[async_trait]`. Pull the header, strip the `Bearer ` prefix, verify:

```rust
impl<S> FromRequestParts<S> for AuthUser
where AuthConfig: FromRef<S>, S: Send + Sync {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let header = parts.headers.get(AUTHORIZATION).and_then(|h| h.to_str().ok());
        let token = header.and_then(|h| h.strip_prefix("Bearer ")).ok_or(AppError::Unauthorized)?;
        let claims = AuthConfig::from_ref(state).verify(token)?;
        Ok(AuthUser { user_id: claims.sub })
    }
}
```

`FromRef<S>` is what lets the extractor work for *any* state that contains an `AuthConfig` — `AppState` provides `impl FromRef<AppState> for AuthConfig` (given in `app.rs`).
</details>

<details>
<summary>Hint for step 5 (owner-scoped CRUD)</summary>

Every task query carries the authenticated user id. `RowNotFound` maps to `404` via your `From<sqlx::Error>` — and because the query is owner-scoped, "someone else's id" *is* a not-found, no special case:

```rust
// GET /tasks/{id}
let task = sqlx::query_as::<_, Task>(
    "SELECT * FROM tasks WHERE id = $1 AND user_id = $2")
    .bind(id).bind(auth.user_id)
    .fetch_one(&state.pool).await?;     // RowNotFound -> 404
Ok(Json(task))

// POST /tasks  (RETURNING gives you the inserted row back)
let task = sqlx::query_as::<_, Task>(
    "INSERT INTO tasks (user_id, title) VALUES ($1, $2) RETURNING *")
    .bind(auth.user_id).bind(&body.title)
    .fetch_one(&state.pool).await?;
Ok((StatusCode::CREATED, Json(task)))
```

For `register`, a duplicate email trips the `UNIQUE` constraint — catch it as `409`: match `sqlx::Error::Database(e) if e.is_unique_violation()` and return `AppError::Conflict`.
</details>

<details>
<summary>Hint for step 6 (router + layers) and trying it with curl</summary>

```rust
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/openapi.json", get(openapi::openapi))
        .route("/auth/register", post(handlers::register))
        .route("/auth/login", post(handlers::login))
        .route("/tasks", get(handlers::list_tasks).post(handlers::create_task))
        .route("/tasks/{id}", get(handlers::get_task)
                              .patch(handlers::update_task)
                              .delete(handlers::delete_task))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .with_state(state)
}
```

Note axum 0.8's path syntax is `/tasks/{id}` (curly braces), not `:id`. Drive it by hand:

```bash
curl -s localhost:8080/auth/register -d '{"email":"a@b.com","password":"hunter2!"}' -H 'content-type: application/json'
TOKEN=$(curl -s localhost:8080/auth/login -d '{"email":"a@b.com","password":"hunter2!"}' -H 'content-type: application/json' | jq -r .token)
curl -s localhost:8080/tasks -H "authorization: Bearer $TOKEN" -d '{"title":"ship module 6"}' -H 'content-type: application/json'
curl -s localhost:8080/tasks -H "authorization: Bearer $TOKEN"
```
</details>

## Stretch goals

- **Compile-time-checked SQL.** Convert the runtime `query`/`query_as` calls to the `sqlx::query!`/`query_as!` *macros* and run `cargo sqlx prepare` so a column typo is a build error. The single highest-value upgrade here.
- **Refresh tokens & revocation.** Short-lived access JWT + a long-lived refresh token in a `refresh_tokens` table you *can* revoke — the standard answer to "JWTs can't be revoked."
- **`utoipa`-derived OpenAPI.** Replace the hand-rolled `spec()` with `#[derive(ToSchema)]` + `#[utoipa::path]` so the spec is generated from the handlers and can't drift. Serve Swagger UI.
- **Pagination & filtering** on `GET /tasks` (`?limit=&offset=&done=`) — and a covering index so it stays fast.
- **Rate limiting** as a `tower` layer on `/auth/login` (per-IP), the real defense against credential stuffing.
- **Role-based authz.** Add a `role` claim and an extractor like `AdminUser` that rejects non-admins — authorization as a *type*, the natural sequel to Pill 12.

## Key questions

- A handler takes `Json<CreateTask>` and never runs on a malformed body. Trace *where* the request dies and what status comes back — which trait, on which type, produced it?
- Why does cloning `AppState` per request not open a database connection? What exactly is cloned?
- `sha256(password)` and `argon2(password)` both "hash the password." Name the three things argon2 does that make a database leak survivable and SHA-256 a disaster.
- A logged-in user requests `GET /tasks/{someone-elses-id}`. Walk the query that makes the right thing happen, and say why returning `404` (not `403`) is the better answer.
- JWTs are "stateless." State the concrete thing the server is *not* storing — and the capability you give up because of it.
- Your benchmark shows argon2 verify at ~3 ms and JWT verify at ~3 µs. Convert that into a sentence about max logins/sec vs max authenticated-requests/sec on one core, and explain why that ratio is the entire argument for tokens.
- Layer order: you add `TimeoutLayer` then `TraceLayer`. Which one sees the request first, and give a case where the order changes what gets logged.

## Resources

- [axum docs](https://docs.rs/axum/latest/axum/) and the [examples directory](https://github.com/tokio-rs/axum/tree/main/examples) — `jwt`, `sqlx-postgres`, and `error-handling` especially
- [sqlx README](https://github.com/launchbadge/sqlx) — the pool, the `query!` macros, offline mode (`cargo sqlx prepare`)
- [`#[sqlx::test]` docs](https://docs.rs/sqlx/latest/sqlx/attr.test.html) — isolated-database-per-test
- [jsonwebtoken docs](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/) — `encode`/`decode`, `Validation`
- [argon2 / RustCrypto password-hashes](https://docs.rs/argon2/latest/argon2/) — the `PasswordHasher`/`PasswordVerifier` traits
- [tower](https://docs.rs/tower/latest/tower/) and [tower-http](https://docs.rs/tower-http/latest/tower_http/) — `Service`, `Layer`, the middleware battery
- [OWASP: Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html) and [Broken Object Level Authorization (IDOR)](https://owasp.org/API-Security/editions/2023/en/0xa1-broken-object-level-authorization/) — Pills 7 & 12, from the source
- [The OpenAPI Specification](https://spec.openapis.org/oas/latest.html) — what the document at `/openapi.json` conforms to
