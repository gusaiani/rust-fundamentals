# Idiomatic Error Handling & API Design in 5-Minute Pills

## Goal

Design a small Rust crate the way the ecosystem expects — clear error types, ergonomic public API, well-documented entry points — and ship it the way you would to crates.io.

## Time estimate

~1 day (15 pills × 5 min + project)

## What you'll learn

- The `Error` trait, `Result<T, E>`, and the `?` operator
- `thiserror` for libraries vs `anyhow` for applications
- Designing error enums that compose with `From` and `#[source]`
- The newtype pattern for cheap, type-checked validation
- The builder pattern for ergonomic construction
- Sealed traits, `#[non_exhaustive]`, and other API hygiene tricks
- Doc tests as both documentation and CI
- The shape of a publishable crate root

## Concepts

### Pill 1: The `Error` Trait

`std::error::Error` is the standard interface every Rust error type implements. The trait itself is small:

```rust
trait Error: Debug + Display {
    fn source(&self) -> Option<&(dyn Error + 'static)> { None }
}
```

Two requirements: be printable (`Display` for users, `Debug` for developers), and optionally point at an inner cause via `source()`. That's enough for callers to walk a chain of failures and print "couldn't load config: parse failed for `PORT`: invalid digit." You almost never write the impl by hand — `thiserror` derives it. But knowing what's in the trait explains what the derive is doing.

### Pill 2: `Result<T, E>` and `?`

`Result<T, E>` is the enum `Ok(T) | Err(E)`. Every fallible function returns one. The `?` operator unwraps `Ok` or **early-returns** the `Err` after running it through `From::from`:

```rust
fn read_port() -> Result<u16, MyError> {
    let raw: String = std::env::var("PORT")?;   // VarError -> MyError
    let n: u16 = raw.parse()?;                   // ParseIntError -> MyError
    Ok(n)
}
```

`?` is just `match value { Ok(v) => v, Err(e) => return Err(From::from(e)) }`. The `From` conversion is what makes it compose — see Pill 6.

### Pill 3: Library Errors vs Application Errors — Two Different Jobs

Two audiences, two needs:

- **Library code** wants **specific, structured** errors. Callers branch on them. ("Did this fail because the var was missing or because it didn't parse?")
- **Application code** mostly wants to **propagate, log, and bail**. The exact cause matters for the log line, not for control flow.

Don't use the same type for both. Libraries: a hand-crafted enum, derived with `thiserror`. Applications: `anyhow::Result<T>`, which boxes any error and chains causes.

### Pill 4: `thiserror` — Derive `Error` for an Enum

`thiserror` is a tiny proc-macro crate that generates `Display` + `Error` impls from attributes:

```rust
#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("missing env var `{0}`")]
    Missing(String),

    #[error("failed to parse `{var}`")]
    Parse {
        var: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
```

`#[error("...")]` becomes `Display`. `#[source]` marks the inner cause for the chain. `#[from]` (next pill) auto-implements `From<InnerErr>`. Zero runtime cost — it's all macro expansion at compile time.

### Pill 5: `anyhow` — One Error to Rule Them All

`anyhow::Error` is a heap-allocated, type-erased error wrapper. Use it in `main`, in CLI tools, in glue code — anywhere you don't need to branch on the specific type:

```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let cfg = load_config().context("loading app config")?;
    println!("{cfg:?}");
    Ok(())
}
```

`.context("...")` wraps the error with a human-readable layer. `?` works with **any** type implementing `std::error::Error + Send + Sync + 'static` — including your `thiserror` enums.

Don't expose `anyhow::Error` from a library. Callers can't pattern-match it.

### Pill 6: `From` and `?`

`?` calls `From::from` on the error before returning it. So if you write:

```rust
#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

`#[from]` generates `impl From<std::io::Error> for MyError`. Now any function returning `Result<_, MyError>` can use `?` on an `io::Error` and the conversion happens automatically.

Use `#[from]` for variants that are direct wrappers. Use `#[source]` (with a hand-written `From` or no `From` at all) when the variant carries **additional** fields like the var name.

### Pill 7: `#[non_exhaustive]` — Future-Proof Your Errors

Adding a new variant to a public enum is normally a breaking change — downstream `match` blocks lose exhaustiveness. `#[non_exhaustive]` defers that:

```rust
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum LoadError { /* ... */ }
```

Outside the defining crate, `match err { ... }` now requires a `_ =>` arm. You can add new variants in the next minor release without breaking anyone. Use it on every public error enum, and on any public struct you might want to add fields to later.

### Pill 8: The Newtype Pattern

A struct with one field that wraps an existing type, giving it a new name and (usually) restricted construction:

```rust
pub struct VarName(String);

impl VarName {
    pub fn parse(s: &str) -> Result<Self, NameError> {
        // validate, then return Self(s.to_owned())
    }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

Two payoffs:

1. **Type-level distinction.** A `VarName` is not interchangeable with any old `String`. The compiler enforces "this function only takes validated names."
2. **Single validation point.** Every `VarName` in the program has been through `parse`. No defensive checking elsewhere.

Pair with `impl TryFrom<&str> for VarName` for `?`-friendly construction.

### Pill 9: The Builder Pattern

Build complex objects by chaining method calls and finalizing with a terminal method:

```rust
let env = Loader::new()
    .require::<u16>("PORT")
    .optional_or::<String>("LOG_LEVEL", "info".into())
    .load()?;
```

Each method takes `self` by value and returns `Self` — so the chain consumes the previous step. (You can also do `&mut self` for in-place builders; the consuming style fits one-shot construction better.)

Builders shine when:

- Construction has many optional parameters.
- Some parameters are required and need a final validation step.
- The constructed object is immutable after build.

### Pill 10: Sealed Traits

A "sealed" trait is one that outside crates **cannot implement**, even though they can see and use it. The trick: require a private supertrait.

```rust
mod private {
    pub trait Sealed {}
}

pub trait FromEnv: private::Sealed { /* methods */ }

impl private::Sealed for u16 {}
impl FromEnv for u16 { /* ... */ }
```

External crates can't `impl FromEnv for MyType` because they can't `impl private::Sealed for MyType` — `Sealed` isn't accessible to them. Why bother? You control the set of types that satisfy the trait, which means you can add new methods later without it being a breaking change for downstream impls (there *aren't any*).

### Pill 11: Re-exports & the Crate Root

A library's `lib.rs` is its public face. Aim for: `use mycrate::{Loader, Env, Error};` — one `use`, all the common types.

```rust
// src/lib.rs
mod error;
mod loader;
mod var_name;

pub use error::Error;
pub use loader::{Env, Loader};
pub use var_name::VarName;
```

Internals stay in modules; the crate root re-exports the small surface. Don't make users dig three modules deep for the type they need.

### Pill 12: Doc Tests

Code blocks in `///` comments are *compiled and run* by `cargo test`:

````rust
/// ```
/// let v = envguard::VarName::parse("PORT").unwrap();
/// assert_eq!(v.as_str(), "PORT");
/// ```
pub struct VarName(String);
````

Two wins: documentation that can't go stale (it'd fail CI), and friction-free examples for users browsing docs.rs. Use `no_run` for examples that need real env or network. Use `compile_fail` to *prove* a misuse doesn't compile (typestate guarantees, sealed traits).

### Pill 13: `TryFrom` for Validating Constructors

`TryFrom<&str>` is the standard trait for "convert from a primitive into a richer type, with a chance of failure":

```rust
impl TryFrom<&str> for VarName {
    type Error = NameError;
    fn try_from(s: &str) -> Result<Self, Self::Error> { /* ... */ }
}
```

Once implemented, callers can write `let v: VarName = "PORT".try_into()?;` and `?` propagates the error if input is bad. Pair `From` with infallible conversions, `TryFrom` with fallible ones.

### Pill 14: API Surface Hygiene

Habits that separate "a Rust file" from "a publishable crate":

- **Doc-comment every public item.** `#![deny(missing_docs)]` if you're feeling spicy.
- **`#[must_use]`** on builders and on `Result`-returning functions where ignoring the result is a bug.
- **Minimal dependencies.** Each new dep is a downstream version-conflict risk.
- **Re-export public-API types** from your dependencies (e.g., `pub use serde::Deserialize`) so users don't pin a different version.
- **`Cargo.toml` metadata:** `description`, `repository`, `license`, `keywords`, `categories`. Without these, your crate looks abandoned on crates.io.
- **Semver discipline.** Public-API breaking change → bump major. Adding behind `#[non_exhaustive]` → minor.

### Pill 15: Choosing Between the Patterns

| Need                                          | Reach for                       |
| --------------------------------------------- | ------------------------------- |
| Cheap typed wrapper around a primitive        | Newtype (Pill 8)                |
| Restrict who can implement your trait         | Sealed trait (Pill 10)          |
| Many optional fields, single freeze           | Builder (Pill 9)                |
| Library error type                            | `thiserror` (Pill 4)            |
| Application error type                        | `anyhow` (Pill 5)               |
| Convert a primitive into a richer type        | `TryFrom` (Pill 13)             |
| Future-proof your public enums                | `#[non_exhaustive]` (Pill 7)    |
| Document and test in one place                | Doc tests (Pill 12)             |

Knowing *which* pattern fits *which* problem is most of the skill. Each pattern is small. The taste comes from picking the right one fast.

## Project: `envguard` — typed env-var loading with helpful errors

A tiny library that loads typed configuration from environment variables and produces a structured error when anything is missing or malformed. Every Rust service needs this; most reinvent it badly. You're going to build the small, opinionated, publishable version.

Why it's a good vehicle for this module:

- **Errors matter.** Five distinct failure modes (missing, parse failed, bad name, ...). The error type *is* the API.
- **Builder fits naturally.** Describe the schema fluently, then `.load()` once and get *all* errors at once — a strict improvement on the usual "die on first missing var."
- **Sealed trait.** Only specific types are loadable from env (`u16`, `bool`, `String`, etc.) — and you control the list.
- **Newtype.** `VarName` validates that a name is upper-snake-case, freeing the rest of the codebase from string-checking.
- **Publishable.** The API is small enough to fit in your head, real enough to be useful.

### Requirements

1. `Loader` builder with `.require::<T>(name)`, `.optional::<T>(name)`, `.optional_or::<T>(name, default)`. Each returns `Self`.
2. `.load()` returns `Result<Env, Vec<Error>>` — collect *all* config problems, not just the first.
3. `Env::get::<T>(name)` returns the typed value, or `Error::NotRequested` if it wasn't in the schema.
4. `Error` enum using `thiserror`, marked `#[non_exhaustive]`, with `#[source]` chaining for parse failures.
5. `VarName` newtype with `TryFrom<&str>` — only `[A-Z][A-Z0-9_]*` is accepted.
6. `FromEnv` is a public trait but **sealed** — outside crates cannot implement it.
7. At least one doc test in the public API that compiles and runs as part of `cargo test`.
8. An example binary that uses `envguard` from inside an `anyhow::Result<()>` `main`.

### Starter files

- `Cargo.toml` — `thiserror` dependency, `anyhow` dev-dependency, full `[package]` metadata.
- `src/lib.rs` — module declarations and re-exports.
- `src/error.rs` — `Error` enum with all variants stubbed.
- `src/var_name.rs` — `VarName` newtype + `TryFrom<&str>` validator.
- `src/from_env.rs` — sealed `FromEnv` trait + impls for the common types.
- `src/loader.rs` — `Loader` builder and `Env` reader.
- `examples/load_app_config.rs` — uses `envguard` from an anyhow main.
- `tests/integration.rs` — drives the public API with temporary env vars.

### Your task

1. **Error enum (`error.rs`):** declare `Error` with variants `Missing { var }`, `Parse { var, source }`, `InvalidName { name, reason }`, `NotRequested { var }`. Derive `thiserror::Error + Debug`. Mark `#[non_exhaustive]`. Use `#[source]` on the inner cause in `Parse`.
2. **`VarName` newtype (`var_name.rs`):** validate `[A-Z][A-Z0-9_]*` in a `parse` constructor, expose `as_str`, implement `TryFrom<&str>`. Add a doc test.
3. **Sealed `FromEnv` (`from_env.rs`):** create a private `Sealed` trait, declare `FromEnv: Sealed` with `fn from_env_str(raw: &str) -> Result<Self, ParseFailure>`, implement for `u16`, `u32`, `i32`, `i64`, `usize`, `bool`, `String`. Use `Box<dyn Error + Send + Sync>` as the boxed source for parse failures.
4. **`Loader` builder (`loader.rs`):** accumulates parsed values in a `HashMap<VarName, Box<dyn Any + Send + Sync>>` and errors in a `Vec<Error>`. `.require`, `.optional`, `.optional_or` consume and return `Self`. `.load()` returns `Ok(Env)` if `errors` is empty, else `Err(errors)`.
5. **`Env::get<T>`:** look up the value by name and downcast to `T`. Return `Error::NotRequested` if the name isn't in the map.
6. **Re-exports (`lib.rs`):** re-export `Error`, `Loader`, `Env`, `VarName`, `FromEnv` from the crate root.
7. **Example (`examples/load_app_config.rs`):** define an `AppConfig` struct, build it from env via `envguard`, propagate errors with `anyhow`.
8. **Tests (`tests/integration.rs`):** at least three tests — happy path, missing-var error, parse-failure error.

### Hints

<details>
<summary>Hint for step 1 (`#[from]` vs `#[source]`)</summary>

`Parse { var, source }` carries an *additional* field (`var`) alongside the inner cause, so `#[from]` won't help — it only generates `From<InnerErr>` on tuple-shaped variants. Use `#[source]` to mark the cause for the chain, and convert manually inside the loader: `Error::Parse { var: name.into(), source: Box::new(inner) }`.

</details>

<details>
<summary>Hint for step 3 (sealed via private supertrait)</summary>

Put `pub trait Sealed {}` inside a `mod private { ... }`. `Sealed` is `pub` *within the crate* but the module isn't `pub`, so external crates can see neither the trait nor the module path. Then `pub trait FromEnv: private::Sealed { ... }`. Inside your crate, `impl private::Sealed for u16 {}` works fine. From outside, it doesn't.

</details>

<details>
<summary>Hint for step 4 (heterogeneous map)</summary>

The map stores `Box<dyn Any + Send + Sync>` because each variable can be a different concrete type. `Box::new(value) as Box<dyn Any + Send + Sync>` performs the type erasure. The `Send + Sync` bound is so callers can share the loaded `Env` across threads.

</details>

<details>
<summary>Hint for step 5 (downcast)</summary>

`map.get(&name)?.downcast_ref::<T>()` returns `Option<&T>`. Clone or copy out of it to return a `T`, which means `T: FromEnv + Clone + 'static`. Match on `None` to return a more specific error (you'll know whether it's "not requested" vs "wrong type requested" by checking whether the key is in the map at all).

</details>

<details>
<summary>Hint for step 8 (tests need isolated env)</summary>

Tests run in parallel and share process env. Set var names with a per-test prefix (e.g. `T1_PORT`, `T2_PORT`) to avoid collisions, or use `#[serial]` from the `serial_test` crate (don't add the dep — just stagger the names).

</details>

## Stretch goals

- **Derive macro:** write a `#[derive(FromEnv)]` proc-macro in a sibling crate that generates a `Loader` schema from a struct's fields. (This is its own small project — a couple hours.)
- **`.dotenv` support:** add a `.env` file loader behind a `dotenv` feature flag. Demonstrate Cargo features and conditional compilation.
- **Custom parsers:** let users register `fn(&str) -> Result<T, _>` for their own types without breaking the seal — via a `Loader::with::<T>(name, parser)` method.
- **Publish it:** pick a unique name, fill in the metadata, run `cargo publish --dry-run`. (Don't actually publish unless you mean to — names are first-come-first-serve and squatting is frowned upon.)

## Key questions

- Why do libraries prefer `thiserror` over `anyhow`? What does each give up?
- What problem does `#[non_exhaustive]` solve, and what's the cost to callers?
- When would you choose `From` (`?`-friendly) over `TryFrom`? When the reverse?
- Why does the sealed-trait pattern require a *private* supertrait, not a regular one?
- The builder consumes `self` and returns `Self`. What ergonomic benefit does that give versus `&mut self`? What does it cost?

## Resources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) — the official checklist for a publishable crate
- [`thiserror` docs](https://docs.rs/thiserror)
- [`anyhow` docs](https://docs.rs/anyhow)
- [BurntSushi — *Error Handling in Rust*](https://blog.burntsushi.net/rust-error-handling/) — long, opinionated, worth it
- [The Rustonomicon — Sealed Traits](https://rust-lang.github.io/api-guidelines/future-proofing.html#sealed-traits-protect-against-downstream-implementations-c-sealed)
- [Cargo Book — publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
