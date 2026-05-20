# envtyped

Typed environment-variable loading for Rust, with structured errors.

Describe your config schema fluently, load it once, and get **every** problem in a single error list instead of dying on the first missing var.

```toml
[dependencies]
envtyped = "0.1"
```

## Example

```rust
use envtyped::Loader;

let env = Loader::new()
    .require::<u16>("PORT")
    .require::<String>("DATABASE_URL")
    .optional_or::<String>("LOG_LEVEL", "info".into())
    .load()
    .expect("config errors");

let port: u16 = env.get("PORT").unwrap();
let db: String = env.get("DATABASE_URL").unwrap();
```

When something is wrong, `.load()` returns `Err(Vec<Error>)` containing **all** the failures — missing vars, parse errors, invalid names — not just the first one. The variants are structured (`Missing`, `Parse`, `InvalidName`, `NotRequested`), so callers can branch on them or just print them.

## Features

- Builder-style schema: `.require::<T>(name)`, `.optional::<T>(name)`, `.optional_or::<T>(name, default)`.
- Collects all errors per `.load()` call.
- Built-in support for `u16`, `u32`, `i32`, `i64`, `usize`, `bool`, `String` via the sealed `FromEnv` trait.
- `VarName` newtype enforces `[A-Z][A-Z0-9_]*` for variable names.
- `Error` enum is `#[non_exhaustive]` and implements `std::error::Error` with proper source chaining via `thiserror`.
- Tiny: one runtime dependency (`thiserror`).

## Errors

`Error` is a `#[non_exhaustive]` enum:

| Variant | When |
| --- | --- |
| `Missing { var }` | A `.require` var isn't set in the process environment. |
| `Parse { var, source }` | The raw string couldn't be parsed into the requested `T`. `source` chains the underlying parse error. |
| `InvalidName { name, reason }` | The schema declared a name that isn't valid (`[A-Z][A-Z0-9_]*`). |
| `NotRequested { var }` | `Env::get` was called for a name the schema didn't include. |

Use with `thiserror` in a library or `anyhow` in an application — both work.

## Running the example

```bash
PORT=8080 DATABASE_URL=postgres://x WORKERS=4 \
  cargo run --example load_app_config
```

Try omitting `PORT` or setting `WORKERS=not-a-number` to see the aggregated error output.

## Status

This crate was built as a teaching exercise for idiomatic Rust error handling and API design. The API is small and stable enough to use in real projects, but development is intermittent. Issues and PRs welcome.

The original learning material that produced this crate — the 15 "pills" covering `thiserror`, `anyhow`, sealed traits, newtypes, builders, `#[non_exhaustive]`, doc tests, and the rest — lives in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
