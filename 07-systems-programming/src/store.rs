//! The keyspace — an in-memory map of byte-string keys to byte-string values,
//! with optional per-key expiry (Pills 8 & 9).
//!
//! What's **given**: the `Store`/`Entry` types, the clock helpers
//! ([`now_ms`]), the *active* expiry sweep ([`Store::purge_expired`]), and the
//! snapshot hooks persistence needs. What's **step 3**: the command-facing
//! operations — `get`, `set`, `del`, `exists`, `incr`, `expire`, `ttl` — each of
//! which must honour **lazy expiry**: a key whose deadline has passed is treated
//! as already gone, the moment you touch it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds since the Unix epoch. Expiry deadlines are stored as
/// absolute timestamps in this unit, so they survive a snapshot/reload.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_millis() as u64
}

/// One stored value plus its optional expiry deadline (absolute `now_ms`).
#[derive(Debug, Clone)]
pub struct Entry {
    pub value: Vec<u8>,
    /// `None` = never expires. `Some(t)` = dead once `now_ms() >= t`.
    pub expires_at: Option<u64>,
}

impl Entry {
    fn is_expired(&self, now: u64) -> bool {
        matches!(self.expires_at, Some(t) if now >= t)
    }
}

/// The whole database: a hash map, plus where to persist it.
#[derive(Debug, Default)]
pub struct Store {
    map: HashMap<Vec<u8>, Entry>,
    /// Where `SAVE`/shutdown writes the snapshot, and where boot loads it from.
    /// `None` disables persistence (used by the tests).
    pub snapshot_path: Option<PathBuf>,
}

impl Store {
    pub fn new(snapshot_path: Option<PathBuf>) -> Store {
        Store {
            map: HashMap::new(),
            snapshot_path,
        }
    }

    /// Number of live (non-expired) keys — used by tests and `DBSIZE`.
    pub fn len(&self) -> usize {
        let now = now_ms();
        self.map.values().filter(|e| !e.is_expired(now)).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **Active expiry** (Pill 9): drop every key whose deadline has passed.
    /// The event loop calls this on a timer so memory is reclaimed even for keys
    /// no client ever touches again. Given — the *lazy* half is your job below.
    pub fn purge_expired(&mut self) {
        let now = now_ms();
        self.map.retain(|_, e| !e.is_expired(now));
    }

    // ---- snapshot hooks (given; used by `persistence`) ----------------------

    /// Iterate live entries as `(key, value, expires_at)` where `expires_at == 0`
    /// means "no expiry". The snapshot writer (Pill 10) walks this.
    pub fn snapshot_iter(&self) -> impl Iterator<Item = (&[u8], &[u8], u64)> {
        let now = now_ms();
        self.map.iter().filter_map(move |(k, e)| {
            if e.is_expired(now) {
                None
            } else {
                Some((k.as_slice(), e.value.as_slice(), e.expires_at.unwrap_or(0)))
            }
        })
    }

    /// Insert an entry read back from a snapshot. `expires_at == 0` → no expiry.
    /// Used by [`crate::persistence::load`]; skips already-expired entries.
    pub fn load_entry(&mut self, key: Vec<u8>, value: Vec<u8>, expires_at: u64) {
        let expires_at = if expires_at == 0 {
            None
        } else {
            Some(expires_at)
        };
        if let Some(t) = expires_at {
            if now_ms() >= t {
                return; // already dead by the time we loaded it
            }
        }
        self.map.insert(key, Entry { value, expires_at });
    }

    // ---- command-facing operations (STEP 3 — implement these) ---------------

    /// `GET key` — the value, or `None` if absent **or expired**.
    pub fn get(&mut self, key: &[u8]) -> Option<&[u8]> {
        let now = now_ms();

        let expired = matches!(self.map.get(key), Some(e) if e.is_expired(now));
        if expired {
            self.map.remove(key);
            return None;
        }
        self.map.get(key).map(|e| e.value.as_slice())
    }

    /// `SET key value [expiry]` — store (overwriting any existing value).
    /// `expire_at` is an absolute `now_ms` deadline, or `None` for no expiry.
    pub fn set(&mut self, key: Vec<u8>, value: Vec<u8>, expire_at: Option<u64>) {
        self.map.insert(
            key,
            Entry {
                value,
                expires_at: expire_at,
            },
        );
    }

    /// `DEL key` — returns true if a live key was removed.
    /// hands back the Entry if one was present and not expired
    pub fn del(&mut self, key: &[u8]) -> bool {
        let now = now_ms();

        self.map.remove(key).map_or(false, |e| !e.is_expired(now))
    }

    /// `EXISTS key` — does a live (non-expired) key exist?
    ///
    pub fn exists(&mut self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }

    /// `INCR key` — parse the value as i64, add 1, store it back, return the new
    /// value. A missing key starts at 0 (so the result is 1).
    pub fn incr(&mut self, key: &[u8]) -> std::result::Result<i64, ()> {
        let now = now_ms();

        let existing = self.map.get(key).filter(|e| !e.is_expired(now));
        let current_ttl = existing.and_then(|e| e.expires_at);
        let current_bytes = existing.map(|e| e.value.as_slice());

        let current: i64 = match current_bytes {
            Some(bytes) => std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or(())?,
            None => 0,
        };
        let next = current + 1;

        self.set(key.to_vec(), next.to_string().into_bytes(), current_ttl);
        Ok(next)
    }

    /// `EXPIRE key seconds` — set a TTL on an existing key. Returns true if the
    /// key existed (and the TTL was set), false otherwise.
    pub fn expire(&mut self, key: &[u8], seconds: u64) -> bool {
        let now = now_ms();

        match self.map.get_mut(key) {
            Some(e) if !e.is_expired(now) => {
                e.expires_at = Some(now + seconds * 1000);
                true
            }
            _ => false,
        }
    }

    /// `TTL key` — seconds remaining, Redis-style: `-2` if the key doesn't
    /// exist, `-1` if it exists but has no expiry, else the remaining seconds.
    pub fn ttl(&mut self, key: &[u8]) -> i64 {
        let now = now_ms();
        match self.map.get(key) {
            Some(e) if e.is_expired(now) => -2,
            Some(e) => match e.expires_at {
                Some(deadline) => ((deadline - now) / 1000) as i64,
                None => -1,
            },
            None => -2,
        }
    }
}
