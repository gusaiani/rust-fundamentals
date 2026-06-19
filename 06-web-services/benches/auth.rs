//! The deliverable benchmark — and it argues for the architecture (Pill 15).
//!
//! Run: `cargo bench`
//!
//! It compiles against the stubbed library, so it will *panic* in setup until
//! you've implemented `hash_password` / `issue` / `verify` — that's expected.
//! Once they work, read the two numbers:
//!
//!   - `argon2_verify` is **deliberately** slow (~1–10 ms) — that's Pill 7's
//!     memory-hard cost, the thing that makes a password-database leak survivable.
//!   - `jwt_verify` is ~microseconds — an HMAC check over a few hundred bytes.
//!
//! The ~1000× gap **is** the reason JWTs exist (Pill 8). If you re-verified the
//! password on every request, your API would cap at a few hundred req/s/core on
//! auth alone. Instead you pay argon2 *once* at login, mint a token, and every
//! later request pays only the microsecond verify. That's the whole argument
//! for stateless tokens, on a graph.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use taskline::auth::{hash_password, verify_password, AuthConfig};
use uuid::Uuid;

fn bench(c: &mut Criterion) {
    // --- argon2 password verification (the expensive path, run once at login) ---
    let password = "correct horse battery staple";
    let hash = hash_password(password).expect("implement hash_password (step 4) first");

    c.bench_function("argon2_verify", |b| {
        b.iter(|| verify_password(black_box(password), black_box(&hash)).unwrap());
    });

    // --- JWT verification (the cheap path, run on every authenticated request) ---
    let auth = AuthConfig::new("benchmark-secret", 3600);
    let token = auth
        .issue(Uuid::new_v4())
        .expect("implement AuthConfig::issue (step 4) first");

    c.bench_function("jwt_verify", |b| {
        b.iter(|| auth.verify(black_box(&token)).unwrap());
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
