# Rust Engineering Course

A project-based course to go from zero to a $250k+ remote Rust engineer, working from Brazil.

**Goal:** Build real, portfolio-worthy systems every module. No toy examples after the first one. The kind of work that gets shortlisted at databases, infra, fintech, and AI-infra companies.

---

## Roadmap

| #   | Module                                                                   | Key Skills                                                                                            | Project                                                                   | Status |
| --- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------ |
| 01  | [Rust Fundamentals](./01-rust-fundamentals/)                             | Ownership, borrowing, enums, `Option`/`Result`, iterators, serde, file I/O                            | Expense tracker CLI                                                       | ✅     |
| 02  | [Ownership, Types & Traits](./02-ownership-types-traits/)                | Lifetimes, generics, trait bounds, dynamic dispatch, `Box`/`Rc`/`Arc`, interior mutability            | Type-safe state machine library                                           | ✅     |
| 03  | [Idiomatic Error Handling & API Design](./03-error-handling-api-design/) | `thiserror`, `anyhow`, error enums, newtype patterns, builder pattern, public API ergonomics          | A small open-source crate, published to crates.io                         | ✅     |
| 04  | [Concurrency & Parallelism](./04-concurrency/)                           | Threads, `Send`/`Sync`, channels (`mpsc`, `crossbeam`), `Mutex`/`RwLock`, work-stealing, `rayon`      | Parallel log analyzer                                                     | ⬜     |
| 05  | [Async Rust & Tokio](./05-async-tokio/)                                  | Futures, pinning, executors, `tokio` runtime, `select!`, cancellation, structured concurrency         | Async port scanner / TCP proxy                                            | ⬜     |
| 06  | [Web Services in Rust](./06-web-services/)                               | `axum`, `tower`, `sqlx` (Postgres), migrations, JWT auth, request validation, OpenAPI                 | Production-grade REST API with auth & Postgres                            | ⬜     |
| 07  | [Systems Programming](./07-systems-programming/)                         | TCP/UDP from scratch, custom binary protocols, `mio`, file I/O, memory mapping, signals               | Implement a wire protocol (Redis RESP or HTTP/1.1)                        | ⬜     |
| 08  | [Performance Engineering](./08-performance/)                             | `criterion` benchmarks, `perf`/`flamegraph`, allocation profiling, zero-copy, SIMD, branch prediction | Optimize a real codebase from baseline → 10×+                             | ⬜     |
| 09  | [Unsafe Rust & FFI](./09-unsafe-ffi/)                                    | `unsafe` invariants, raw pointers, `repr(C)`, `bindgen`, calling Rust from C and vice versa           | Rust library with C ABI, consumed by a C/Python program                   | ⬜     |
| 10  | [Database Internals](./10-database-internals/)                           | B-trees, LSM trees, WAL, MVCC, page cache, query planning                                             | Mini key-value store with persistence and crash safety                    | ⬜     |
| 11  | [Distributed Systems](./11-distributed-systems/)                         | gRPC (`tonic`), protobuf, Raft basics, replication, partitioning, consistent hashing                  | Replicated KV store on top of Module 10                                   | ⬜     |
| 12  | [Production & Observability](./12-production/)                           | `tracing`, OpenTelemetry, metrics (Prometheus), structured logging, Docker, graceful shutdown, CI/CD  | Containerized service with full observability                             | ⬜     |
| 13  | [Capstone](./13-capstone/)                                               | Everything                                                                                            | Ship one polished, deployed Rust service that demonstrates the full stack | ⬜     |

---

## How to use this course

1. **Do modules in order.** Each builds on the last.
2. **Ship every module.** Public repo, README, screenshots, a short writeup. The portfolio is the point.
3. **Read the standard library docs.** They are the best Rust resource. Get fluent navigating them.
4. **Don't skip benchmarks.** From Module 4 onward, every project should have at least one benchmark that proves it does what it claims.

---

## Stack

- **Language:** Rust (stable, latest edition)
- **Async runtime:** `tokio`
- **Web:** `axum` + `tower`
- **DB:** Postgres via `sqlx`; custom storage in Module 10
- **Tracing/metrics:** `tracing` + OpenTelemetry + Prometheus
- **Bench:** `criterion`
- **Deploy:** Docker + Fly.io / Railway / Hetzner (cheap and globally reachable)

---

## Setup

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Useful tooling
rustup component add clippy rustfmt
cargo install cargo-watch cargo-edit cargo-expand
```

Each module is its own Cargo project — `cd` into it and `cargo run` / `cargo test`.

---

## Career strategy (Brazil → $250k+ remote)

The $250k+ Rust market is real but narrow. The pattern that works:

- **Target the right companies.** Database/infra (TigerBeetle, ClickHouse, Materialize, Turso), fintech/crypto infra (Solana, Aptos, Polygon, Jump, Jane Street-adjacent), AI infra (Modal, Hugging Face, Anyscale, Together), edge/cloud (Cloudflare, Fastly, Discord), trading firms with remote desks.
- **Skip generic "Rust backend" gigs.** They top out around $150k. The premium is paid for systems depth: storage, networking, performance, distributed systems.
- **Portfolio over resume.** A single deeply-engineered project (a working KV store, a custom protocol, a service that survives a load test) is worth more than five CRUD apps.
- **Open-source contribution.** A merged PR to `tokio`, `axum`, `sqlx`, or a database crate is a hiring signal that beats most credentials.
- **Where to look:** `rustjobs.dev`, `weworkremotely.com`, `wellfound.com`, company careers pages directly. Avoid recruiter-saturated boards.
- **Comp model:** PJ contractor at $100–150/hr USD, or full-time remote $200–280k base + equity. Most $250k+ offers are at AI infra or crypto.
- **Resume keywords that matter:** `tokio`, `axum`, `sqlx`, distributed systems, gRPC, performance optimization, low-latency, observability, Linux internals.

After Module 8, hireable for a senior Rust role. After Module 12, hireable for staff-level / specialized infra roles where the $250k+ band lives.

---

## Resources

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) — for unsafe and FFI
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Jon Gjengset's videos](https://www.youtube.com/@jonhoo) — deep, advanced Rust
- [Designing Data-Intensive Applications](https://dataintensive.net/) — required reading for Modules 10–11
