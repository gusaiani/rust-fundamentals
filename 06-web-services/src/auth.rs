//! Authentication: password hashing (Pill 7), JWTs (Pill 8), and the
//! `AuthUser` extractor (Pill 9).
//!
//! What's **given**: the `AuthConfig` (holds the signing/verifying keys + TTL),
//! the `Claims` shape, and the extractor's *type plumbing*. What's **step 4**:
//! the four function bodies and the extractor's logic — all marked `todo!()`.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// Signing material + token lifetime, derived once from the config secret and
/// shared (cheaply — keys are small, Clone is fine) via `AppState`.
#[derive(Clone)]
pub struct AuthConfig {
    encoding: EncodingKey,
    decoding: DecodingKey,
    ttl_secs: i64,
}

impl AuthConfig {
    /// Build the keys from the HMAC secret. (Given — straightforward setup.)
    pub fn new(secret: &str, ttl_secs: i64) -> Self {
        AuthConfig {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            ttl_secs,
        }
    }

    /// Mint a signed JWT for `user_id`, valid for `ttl_secs`.
    ///
    /// TODO (step 4): build `Claims { sub: user_id, iat: now, exp: now + ttl }`
    /// (seconds since epoch as `usize`), then
    /// `jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)`.
    pub fn issue(&self, user_id: Uuid) -> Result<String, AppError> {
        let now = Utc::now().timestamp();
        let exp = now + self.ttl_secs;

        let claims = Claims {
            sub: user_id,
            iat: now as usize,
            exp: exp as usize,
        };

        let token = encode(&Header::default(), &claims, &self.encoding)?;
        Ok(token)
    }

    /// Verify a token's signature and expiry, returning its claims.

    pub fn verify(&self, token: &str) -> Result<Claims, AppError> {
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())?;
        Ok(data.claims)
    }
}

/// The signed payload. `sub` is the user id; `exp`/`iat` are unix seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
    pub iat: usize,
}

/// Hash a plaintext password into a self-describing argon2 PHC string
/// (`$argon2id$...`) with a fresh random salt.

pub fn hash_password(plain: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);

    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|_| AppError::Internal)?
        .to_string();

    Ok(hash)
}

/// Check a candidate password against a stored argon2 hash. Returns `Ok(true)`
/// on match, `Ok(false)` on mismatch — only a *corrupt stored hash* is an error.
pub fn verify_password(plain: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash).map_err(|_| AppError::Internal)?;

    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

/// The authenticated caller, produced by verifying the bearer token. A handler
/// that takes this argument is protected *by its signature* (Pill 9) — and
/// `user_id` is the second half of every owner-scoped query (Pill 12).
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    AuthConfig: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    /// TODO (step 4):
    ///   1. read the `Authorization` header: `parts.headers.get(AUTHORIZATION)`
    ///      then `.to_str().ok()`
    ///   2. strip the `"Bearer "` prefix → the token (else `AppError::Unauthorized`)
    ///   3. `AuthConfig::from_ref(state).verify(token)?` → `Claims`
    ///   4. `Ok(AuthUser { user_id: claims.sub })`
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok());

        let token = header
            .and_then(|h| h.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        let claims = AuthConfig::from_ref(state).verify(token)?;

        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}
