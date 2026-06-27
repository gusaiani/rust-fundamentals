# rudis

A minimal Redis-compatible server on a hand-written `mio` event loop.

`rudis` accepts many clients on a single thread with non-blocking I/O, speaks enough of the RESP wire protocol that the real `redis-cli` can drive it, keeps an in-memory keyspace with expiry, and persists to a snapshot file it loads back via `mmap`. Shutdown is folded into the same event loop as the sockets, so a signal drains cleanly.

## What it does

- Serves `redis-cli` (and any RESP client) over TCP from a single thread.
- Stores byte-string keys and values in an in-memory keyspace.
- Supports per-key expiry (`EX`/`PX` on `SET`, `EXPIRE`, `TTL`), both lazy and active.
- Persists the keyspace to a snapshot file (`SAVE`, and on graceful shutdown) and reloads it on boot.

## Features

- Hand-written event loop on `mio` (`epoll`/`kqueue`/IOCP) — no async runtime.
- Non-blocking, buffered I/O with per-connection read/write buffers and `WRITABLE`-interest backpressure.
- An incremental RESP parser that survives partial reads and pipelined commands.
- Snapshot persistence with `fsync`, loaded back with a memory-mapped file.
- Signal-driven graceful shutdown (`SIGINT`/`SIGTERM`) via the self-pipe trick.

## Commands

| Command | Reply |
| --- | --- |
| `PING [message]` | `+PONG`, or the message echoed |
| `ECHO message` | the message |
| `GET key` | the value, or nil |
| `SET key value [EX seconds \| PX millis]` | `+OK` |
| `DEL key [key ...]` | integer: number removed |
| `EXISTS key [key ...]` | integer: number present |
| `INCR key` | integer: the new value |
| `EXPIRE key seconds` | integer: `1` if set, `0` if missing |
| `TTL key` | integer: seconds left, `-1` no expiry, `-2` missing |
| `DBSIZE` | integer: key count |
| `SAVE` | `+OK` (writes a snapshot) |
| `COMMAND ...` | empty array (the handshake `redis-cli` sends on connect) |

## Example session

```text
$ redis-cli -p 6379
127.0.0.1:6379> ping
PONG
127.0.0.1:6379> set foo bar
OK
127.0.0.1:6379> get foo
"bar"
127.0.0.1:6379> set session token EX 10
OK
127.0.0.1:6379> ttl session
(integer) 10
127.0.0.1:6379> incr counter
(integer) 1
127.0.0.1:6379> dbsize
(integer) 3
127.0.0.1:6379> del foo
(integer) 1
```

## Running it

```bash
# build
cargo build --release

# run on the default 127.0.0.1:6379, no persistence
cargo run --bin rudis

# run with persistence: BIND_ADDR and SNAPSHOT_PATH are env-configured
BIND_ADDR=127.0.0.1:6379 SNAPSHOT_PATH=rudis.rdb cargo run --bin rudis
```

In another terminal, drive it with `redis-cli`:

```bash
redis-cli -p 6379 ping            # PONG
redis-cli -p 6379 set foo bar     # OK
redis-cli -p 6379 get foo         # "bar"
redis-cli -p 6379 save            # OK  (only if SNAPSHOT_PATH is set)
```

With `SNAPSHOT_PATH` set, `SAVE` (or a clean shutdown) writes the keyspace to that file, and the next boot loads it back before accepting traffic. Stop the server with `Ctrl-C` (`SIGINT`) or `kill` (`SIGTERM`) — it snapshots, if configured, then exits.

Configuration (both optional):

| Variable | Default | Purpose |
| --- | --- | --- |
| `BIND_ADDR` | `127.0.0.1:6379` | listen address and port |
| `SNAPSHOT_PATH` | _(unset)_ | snapshot file; persistence is disabled when unset |

Tests and benchmark:

```bash
cargo test    # spins up the server on an ephemeral port and drives it over a real socket
cargo bench   # RESP parse throughput vs store GET/SET cost (bench: resp)
```

## How it works

One thread runs one `mio::Poll`. The listener and the signal source get fixed tokens; each client gets its own. `poll()` blocks until something is ready or a 100 ms tick elapses, then dispatches: accept new connections, read from readable clients, flush writable ones.

Each connection owns a read buffer and a write buffer. Bytes are read until `WouldBlock`, the RESP parser drains as many complete frames as the buffer holds (so pipelining works), each frame becomes a `Command` and runs against the store, and replies are encoded into the write buffer. The loop re-arms `WRITABLE` interest only while output is pending — that conditional is the backpressure.

The store is a `HashMap` of byte keys to entries with optional absolute-timestamp deadlines. Expiry is lazy on access plus an active sweep on each tick. Persistence writes a length-prefixed dump and `fsync`s it; loading memory-maps the file and parses straight out of the page cache. Signals arrive as a readable event in the same loop, so shutdown is just breaking the loop, snapshotting, and exiting.

## Project layout

| File | Role |
| --- | --- |
| `src/bin/rudis.rs` | entrypoint: config, snapshot load, bind, run |
| `src/server.rs` | the `mio` event loop |
| `src/connection.rs` | per-connection buffers and non-blocking I/O |
| `src/resp.rs` | RESP parse (incremental) and encode |
| `src/command.rs` | `Command` enum, parse and execute |
| `src/store.rs` | keyspace, clock, expiry |
| `src/persistence.rs` | snapshot save (`fsync`) and load (`mmap`) |
| `benches/resp.rs` | parse throughput vs store ops |
| `tests/integration.rs` | full-stack tests over a real socket |

## Status

Implemented and runnable; speaks to the real `redis-cli`, with integration tests and a benchmark.

The concept pills and the step-by-step build that produced this — covering the `mio` event loop, RESP framing, non-blocking buffered I/O, expiry, `mmap` snapshot persistence, and signal-driven shutdown — live in [`README-LEARN.md`](./README-LEARN.md).

## License

Licensed under either of [MIT license](https://opensource.org/licenses/MIT) or [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
