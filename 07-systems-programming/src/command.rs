//! The command set — parse a RESP request array into a [`Command`], then run it
//! against the [`Store`] (Pill 8).
//!
//! The [`Command`] enum and the [`args_of`] helper are **given**; the parse
//! ([`Command::parse`], step 4) and the dispatch ([`Command::execute`], step 5)
//! are yours. This is the "command pattern": requests become a typed enum, so
//! `execute` is one exhaustive `match` the compiler checks for completeness.

use crate::persistence;
use crate::resp::Resp;
use crate::store::{now_ms, Store};
use crate::{Error, Result};

/// One parsed client request. Keys/values are raw bytes — RESP is binary-safe,
/// so a key can contain any byte, not just UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `PING [message]` — health check; replies `+PONG` or echoes the message.
    Ping(Option<Vec<u8>>),
    /// `ECHO message`.
    Echo(Vec<u8>),
    /// `GET key`.
    Get(Vec<u8>),
    /// `SET key value [EX seconds | PX millis]`.
    Set {
        key: Vec<u8>,
        value: Vec<u8>,
        /// Absolute `now_ms` deadline, pre-computed from EX/PX at parse time.
        expire_at: Option<u64>,
    },
    /// `DEL key [key ...]`.
    Del(Vec<Vec<u8>>),
    /// `EXISTS key [key ...]`.
    Exists(Vec<Vec<u8>>),
    /// `INCR key`.
    Incr(Vec<u8>),
    /// `EXPIRE key seconds`.
    Expire(Vec<u8>, u64),
    /// `TTL key`.
    Ttl(Vec<u8>),
    /// `DBSIZE` — number of keys.
    DbSize,
    /// `SAVE` — write a snapshot now.
    Save,
    /// `COMMAND ...` — `redis-cli` sends this on connect; we reply with an empty
    /// array so the CLI is happy. (Given as a variant so you don't have to.)
    Command,
}

/// Validate that a request is an `Array` of `Bulk` strings and return the raw
/// argument list. **Given** — the byte-juggling isn't the lesson; dispatch is.
pub fn args_of(resp: Resp) -> Result<Vec<Vec<u8>>> {
    let items = match resp {
        Resp::Array(items) => items,
        _ => return Err(Error::protocol("expected an array of bulk strings")),
    };
    let mut args = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Resp::Bulk(bytes) => args.push(bytes),
            _ => return Err(Error::protocol("command arguments must be bulk strings")),
        }
    }
    if args.is_empty() {
        return Err(Error::protocol("empty command"));
    }
    Ok(args)
}

impl Command {
    /// Parse a RESP request value into a `Command`.
    ///
    /// TODO (step 4):
    ///   1. `let args = args_of(resp)?;` (given helper above).
    ///   2. Uppercase `args[0]` to get the command name (Redis is
    ///      case-insensitive). `String::from_utf8_lossy(&args[0]).to_ascii_uppercase()`
    ///      is the easy route.
    ///   3. `match name.as_str()` and build the right variant, checking arity:
    ///      - `"PING"` → `Ping(args.get(1).cloned())`
    ///      - `"ECHO"` → require exactly 1 arg → `Echo`
    ///      - `"GET"`/`"INCR"`/`"TTL"` → require 1 key arg
    ///      - `"DEL"`/`"EXISTS"` → 1+ key args → `Del`/`Exists`
    ///      - `"EXPIRE"` → key + seconds (parse the seconds as u64)
    ///      - `"SET"` → key, value, and optional `EX <secs>` / `PX <ms>`. Turn
    ///        the relative TTL into an absolute deadline:
    ///        `now_ms() + secs*1000` (EX) or `now_ms() + ms` (PX).
    ///      - `"DBSIZE"` → `DbSize`; `"SAVE"` → `Save`; `"COMMAND"` → `Command`
    ///      - anything else → `Err(Error::protocol(format!("unknown command '{name}'")))`
    ///   Wrong arity → `Err(Error::protocol(...))` (the connection layer turns a
    ///   `Protocol` error into a `-ERR` reply, Pill 6).
    pub fn parse(resp: Resp) -> Result<Command> {
        let args = args_of(resp)?;
        let name = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();

        match name.as_str() {
            "PING" => Ok(Command::Ping(args.get(1).cloned())),
            "ECHO" => {
                if args.len() != 2 {
                    return Err(Error::protocol("ECHO requires one argument"));
                }
                Ok(Command::Echo(args[1].clone()))
            }
            "GET" => {
                if args.len() != 2 {
                    return Err(Error::protocol("GET requires one argument"));
                }
                Ok(Command::Get(args[1].clone()))
            }
            "INCR" => {
                if args.len() != 2 {
                    return Err(Error::protocol("INCR requires one argument"));
                }
                Ok(Command::Incr(args[1].clone()))
            }
            "TTL" => {
                if args.len() != 2 {
                    return Err(Error::protocol("TTL requires one argument"));
                }
                Ok(Command::Ttl(args[1].clone()))
            }
            "DEL" => {
                if args.len() < 2 {
                    return Err(Error::protocol("DEL requires at least one key"));
                }
                Ok(Command::Del(args[1..].to_vec()))
            }
            "EXISTS" => {
                if args.len() < 2 {
                    return Err(Error::protocol("EXISTS requires at least one key"));
                }
                Ok(Command::Exists(args[1..].to_vec()))
            }
            "EXPIRE" => {
                if args.len() != 3 {
                    return Err(Error::protocol("EXPIRE requires key and seconds"));
                }
                let seconds: u64 = std::str::from_utf8(&args[2])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::protocol("invalid expire seconds"))?;
                Ok(Command::Expire(args[1].clone(), seconds))
            }

            "SET" => {
                // SET key value          -> 2 args after the name
                // SET key value EX 10    -> 4 args, with a unit + amount
                if args.len() != 3 && args.len() != 5 {
                    return Err(Error::protocol(
                        "SET requires key, value, and optional EX/PX",
                    ));
                }
                let key = args[1].clone();
                let value = args[2].clone();

                let expire_at = if args.len() == 5 {
                    let amount: u64 = std::str::from_utf8(&args[4])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| Error::protocol("invalid SET expiry amount"))?;

                    let unit = String::from_utf8_lossy(&args[3]).to_ascii_uppercase();
                    let deadline = match unit.as_str() {
                        "EX" => now_ms() + amount * 1000,
                        "PX" => now_ms() + amount,
                        _ => return Err(Error::protocol("SET expiry unit must be EX or PX")),
                    };
                    Some(deadline)
                } else {
                    None
                };

                Ok(Command::Set {
                    key,
                    value,
                    expire_at,
                })
            }
            "DBSIZE" => Ok(Command::DbSize),
            "SAVE" => Ok(Command::Save),
            "COMMAND" => Ok(Command::Command),
            _ => Err(Error::protocol(format!("unknown command '{name}'"))),
        }
    }

    /// Run the command against the store, producing the reply value.
    ///
    /// TODO (step 5): one `match self { ... }` returning a `Resp`:
    ///   - `Ping(None)`      → `Resp::Simple("PONG")`; `Ping(Some(m))` → `Bulk(m)`
    ///   - `Echo(m)`         → `Bulk(m)`
    ///   - `Get(k)`          → `store.get(&k)` → `Bulk(..)` or `Resp::Null`
    ///   - `Set{..}`         → `store.set(k, v, expire_at)` then `Resp::ok()`
    ///   - `Del(keys)`       → count how many `store.del` returned true → `Integer`
    ///   - `Exists(keys)`    → count live ones → `Integer`
    ///   - `Incr(k)`         → `store.incr(&k)` → `Integer` or
    ///                          `Resp::error("ERR value is not an integer or out of range")`
    ///   - `Expire(k, secs)` → `Integer(1)` if set, `Integer(0)` if missing
    ///   - `Ttl(k)`          → `Integer(store.ttl(&k))`
    ///   - `DbSize`          → `Integer(store.len() as i64)`
    ///   - `Save`            → call `persistence::save` if `store.snapshot_path`
    ///                          is set; `Resp::ok()` or a `Resp::error`
    ///   - `Command`         → `Resp::Array(vec![])`
    pub fn execute(self, store: &mut Store) -> Resp {
        match self {
            Command::Ping(None) => Resp::Simple("PONG".to_string()),
            Command::Ping(Some(message)) => Resp::Bulk(message),

            Command::Echo(message) => Resp::Bulk(message),

            Command::Get(key) => match store.get(&key) {
                Some(value) => Resp::Bulk(value.to_vec()),
                None => Resp::Null,
            },

            Command::Set {
                key,
                value,
                expire_at,
            } => {
                store.set(key, value, expire_at);
                Resp::ok()
            }

            Command::Del(keys) => {
                let removed = keys.iter().filter(|key| store.del(key)).count();
                Resp::Integer(removed as i64)
            }

            Command::Exists(keys) => {
                let present = keys.iter().filter(|key| store.exists(key)).count();
                Resp::Integer(present as i64)
            }

            Command::Incr(key) => match store.incr(&key) {
                Ok(next) => Resp::Integer(next),
                Err(()) => Resp::error("ERR value is not an integer or out of range"),
            },

            Command::Expire(key, seconds) => {
                let was_set = store.expire(&key, seconds);
                Resp::Integer(was_set as i64)
            }

            Command::Ttl(key) => Resp::Integer(store.ttl(&key)),

            Command::DbSize => Resp::Integer(store.len() as i64),

            Command::Save => match &store.snapshot_path {
                Some(path) => match persistence::save(store, path) {
                    Ok(()) => Resp::ok(),
                    Err(e) => Resp::error(format!("ERR save failed: {e}")),
                },
                None => Resp::error("ERR persistence is not configured"),
            },

            Command::Command => Resp::Array(vec![]),
        }
    }
}
