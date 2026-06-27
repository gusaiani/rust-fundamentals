# Systems Programming in Rust — in 5-Minute Pills

## Goal

Build a **minimal Redis-compatible server** — one you can talk to with the real `redis-cli` — on a **hand-written event loop**, not an async runtime. You'll accept many clients on a single thread with non-blocking I/O multiplexed through `mio`, frame a byte stream into messages with a wire protocol you parse yourself (RESP), keep an in-memory keyspace with expiry, persist it to a snapshot file, load that file back with `mmap`, and shut down gracefully on a signal. By the end you can explain — by having built it — exactly what `tokio` does for you, because here you do it by hand.

## Time estimate

~1 day (13 pills × 5 min + project)

## What you'll learn

- Why TCP is a **byte stream, not a message stream**, and what "framing" means
- **Blocking vs non-blocking I/O** and the *readiness* model: `O_NONBLOCK`, `EWOULDBLOCK`, and the read-until-it-blocks loop
- The **event loop** with `mio` — one `Poll`, `Token`s, `Interest`, and `epoll`/`kqueue` underneath — the machine `tokio` is built on
- Designing and parsing a **wire protocol** (RESP): prefix tags, CRLF framing, length-prefixing, and *incremental* parsing that survives partial reads
- **Read and write buffers**, partial writes, and backpressure (re-arming writable interest only when you have output)
- **Expiry** done the way Redis does it: lazy (on access) plus active (a timer sweep)
- **Durability**: snapshots vs append-only logs, and why `fsync` is the line between "the kernel has it" and "the disk has it"
- **Memory-mapped I/O** (`mmap`): loading a file as a slice via the page cache, and the `unsafe` that honesty requires
- **Signals**: async-signal-safety, the self-pipe trick, and folding signals into the same event loop as your sockets for a clean graceful shutdown

## Concepts

### Pill 1: From `tokio` to the Bare Metal

In Module 5 you wrote `async fn` and `.await` and a runtime scheduled thousands of tasks across a thread pool. That runtime is not magic — it is an **event loop** sitting on an OS readiness API (`epoll` on Linux, `kqueue` on macOS), plus a state machine per task. This module removes the runtime and leaves you holding the loop.

Everything a program does to the outside world is a **syscall** — `read`, `write`, `accept`, `epoll_wait`. A syscall crosses the boundary from your process into the kernel; it is comparatively expensive (hundreds of nanoseconds to microseconds), which is why the whole design below is organized around *making fewer of them and never blocking on one*. The mental shift for this module: you are not "calling functions that do I/O," you are *asking the kernel what's ready and reacting*. That inversion — from "I decide when to read" to "the kernel tells me when reading won't block" — is the entire idea.

### Pill 2: TCP Is a Byte Stream, Not a Message Stream

This is the misconception that breaks every first network server. A TCP connection is an ordered stream of **bytes**, with **no message boundaries**. If a client sends `SET foo bar` and then `GET foo`, your `read()` might return:

- both commands at once, or
- `SET fo`, then `o bar\r\nGET foo`, or
- one byte at a time.

TCP guarantees order and delivery; it guarantees *nothing* about how bytes are grouped into `read()` calls. "One `read` = one message" is false. The job of a protocol (Pill 5) and a parser (Pill 6) is **framing**: deciding, from the bytes alone, where one message ends and the next begins. You accept connections with a listener:

```rust
let listener = TcpListener::bind("127.0.0.1:6379".parse()?)?;
let (stream, peer) = listener.accept()?;   // a new byte-stream socket per client
```

…but `accept` returning a `stream` is just the start. Everything interesting is turning that stream's bytes into commands.

### Pill 3: Blocking vs Non-Blocking I/O

A normal socket is **blocking**: call `read()` and your thread *sleeps* in the kernel until at least one byte arrives. That's fine for one connection per thread — but you want one thread serving thousands. The fix is to make the socket **non-blocking** (`fcntl` with `O_NONBLOCK`; `mio` sockets are non-blocking from birth). Now `read()` *never* sleeps. It either returns bytes, returns `0` (the peer closed), or returns the error **`WouldBlock`** (`EWOULDBLOCK`/`EAGAIN`) meaning "nothing right now, come back later."

```rust
match stream.read(&mut buf) {
    Ok(0)  => { /* peer closed */ }
    Ok(n)  => { /* got n bytes */ }
    Err(e) if e.kind() == io::ErrorKind::WouldBlock => { /* drained for now */ }
    Err(e) if e.kind() == io::ErrorKind::Interrupted => { /* EINTR: retry */ }
    Err(e) => return Err(e),
}
```

`WouldBlock` is **not an error** — it's the normal "that's all for now" signal. The whole event loop is built around it: read until you see `WouldBlock`, write until you see `WouldBlock`, and let the readiness API tell you when to try again. Treating `WouldBlock` as fatal, or forgetting to loop until you reach it, are the two classic bugs.

### Pill 4: The Event Loop and `mio`

If sockets never block, how do you avoid a busy-loop burning 100% CPU asking "anything yet? anything yet?" You ask the kernel to tell you. That's **readiness multiplexing**: register N sockets, then make *one* blocking call that sleeps until *any* of them is ready, and returns the ready set.

`mio` is the thin, cross-platform wrapper over that mechanism (`epoll`/`kqueue`/IOCP). The vocabulary:

```rust
let mut poll = Poll::new()?;
let mut events = Events::with_capacity(256);

poll.registry().register(&mut listener, Token(0), Interest::READABLE)?;
// ... register each client socket with a unique Token ...

loop {
    poll.poll(&mut events, None)?;        // sleeps until something is ready
    for event in events.iter() {
        match event.token() {             // which source woke us?
            Token(0) => accept_connections(),
            token    => service_client(token),
        }
    }
}
```

- **`Poll`** — the readiness selector (one per thread).
- **`Token`** — a `usize` *you* assign to each source so you know which one fired. (We use `0` for the listener, `1` for signals, `16+` for clients.)
- **`Interest`** — `READABLE`, `WRITABLE`, or both: what you want to be told about.
- **`Events`** — the buffer the ready set is written into each `poll()`.

This *is* the loop `tokio` runs internally; the difference is `tokio` adds a task scheduler and futures on top so you can write straight-line `async` code. Here, you are the scheduler. One thread, one `Poll`, every socket. (Real Redis is exactly this: single-threaded, one event loop — and it's one of the fastest servers ever written, because for an in-memory store the bottleneck is I/O and syscalls, not parallel CPU.)

### Pill 5: Designing a Wire Protocol — RESP

A wire protocol is an agreement about how to turn a structured message into bytes and back. We implement **RESP** (the REdis Serialization Protocol) so the real `redis-cli` can talk to us. RESP is small and self-describing: the **first byte tags the type**, and `\r\n` ("CRLF") frames each line.

```text
+OK\r\n                      Simple String  (status reply)
-ERR bad command\r\n         Error
:1000\r\n                    Integer
$5\r\nhello\r\n              Bulk String    $<len>CRLF<bytes>CRLF  (binary-safe)
$-1\r\n                      Null Bulk      (the "nil" reply)
*2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n   Array     *<count>CRLF then <count> values
```

A client request is always an **Array of Bulk Strings**: `SET foo bar` is `*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\nbar\r\n`. Two framing strategies appear here and they're worth naming: **delimiter-framed** (Simple/Error/Integer end at the next CRLF) and **length-prefixed** (a Bulk String says "$5" so you know to read exactly 5 bytes, then a trailing CRLF). Length-prefixing is what makes Bulk Strings *binary-safe* — the value can contain `\r`, `\n`, or any byte, because you're counting, not scanning. Designing real protocols is mostly choosing between these two for each field.

### Pill 6: Incremental Parsing and the Read Buffer

Because TCP gives you arbitrary byte chunks (Pill 2), your parser must handle a **partial message**: it has to say "I don't have a whole command yet, give me more bytes" without losing what it already saw. The shape that makes this clean is a parser that reports **how many bytes it consumed**:

```rust
fn parse(buf: &[u8]) -> Result<Option<(Resp, usize)>>
//                                  ^^^^^^^^^^^^^^^^^^  Some((value, n)) = parsed, drop n bytes
//                                  None                = incomplete, read more and retry
```

The connection keeps a growing **read buffer**. The loop is: append whatever `read()` returned, then call `parse` repeatedly, draining `consumed` bytes each time, until it returns `None` (need more) — *one read can contain several commands, or half of one*:

```rust
loop {
    match Resp::parse(&self.read_buf)? {
        Some((value, consumed)) => { self.read_buf.drain(..consumed); /* execute */ }
        None => break,                 // partial frame: wait for the next read
    }
}
```

This separation — "fill the buffer from the socket" vs "parse complete frames out of the buffer" — is the single most important structure in a network server. Get it right and partial reads, pipelining, and giant values all just work.

### Pill 7: The Write Buffer, Partial Writes, and Backpressure

The mirror image happens on output. A non-blocking `write()` can also return `WouldBlock` — or accept only *some* of your bytes — when the kernel's send buffer is full (a slow client that isn't reading fast enough). So you cannot assume a `write` sends everything. Each connection keeps a **write buffer**: replies are appended to it, and you flush as much as the socket takes, keeping the rest:

```rust
let mut sent = 0;
while sent < self.write_buf.len() {
    match self.stream.write(&self.write_buf[sent..]) {
        Ok(n)  => sent += n,
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,  // buffer full
        Err(e) => return Err(e),
    }
}
self.write_buf.drain(..sent);
```

When output remains, you **register `WRITABLE` interest** so the event loop wakes you when the socket can take more; when the buffer empties, you drop back to `READABLE` only. That conditional re-arming *is* backpressure: a slow consumer naturally throttles you instead of forcing an unbounded memory blow-up or a blocking stall. Asking for `WRITABLE` *all the time* is a classic bug — the socket is almost always writable, so you'd spin.

### Pill 8: The Keyspace and the Command Pattern

With framing handled, the database itself is almost boring — and that's the point. The store is a `HashMap<Vec<u8>, Entry>` (keys and values are raw bytes; RESP is binary-safe, so keys aren't necessarily UTF-8). Requests become a typed **`Command`** enum, and execution is one exhaustive `match`:

```rust
enum Command { Get(Vec<u8>), Set { key, value, expire_at }, Del(Vec<Vec<u8>>), /* ... */ }

impl Command {
    fn execute(self, store: &mut Store) -> Resp { match self { /* one arm per command */ } }
}
```

This "command pattern" is why adding a command is local and the compiler keeps you honest (a new variant without a `match` arm won't build). It also cleanly separates *parsing* (bytes → `Command`, which can fail with a protocol error) from *execution* (`Command` → `Resp`, which can't fail in the same way) — the same boundary discipline as Module 6's extractors-then-handler.

### Pill 9: Expiry — Lazy and Active Together

A TTL'd key (`SET k v EX 10`, `EXPIRE k 10`) must disappear after its deadline. There are two strategies and Redis — and you — use **both**:

- **Lazy expiry**: when a command *touches* a key, check its deadline first; if it's past, treat the key as absent (and delete it). This is cheap and correct for any key someone reads, but a key that's never touched again would leak memory forever.
- **Active expiry**: periodically sweep and drop expired keys. Here, the event loop's timer **tick** calls `purge_expired()` every 100 ms (`poll` returns on a timeout even with no I/O). This reclaims the keys lazy expiry never revisits.

Deadlines are stored as **absolute timestamps** (`now_ms() + ttl`), not durations, so they survive a snapshot/reload (Pill 10) — a relative "10 seconds" would silently reset on every load; an absolute "expire at epoch-ms 1_718_000_000_000" doesn't.

### Pill 10: Persistence — Snapshots vs Append-Only Logs

An in-memory store loses everything on restart unless it writes to disk. Two classic designs (Redis offers both, RDB and AOF):

- **Snapshot** (what you'll build): periodically dump the *entire* keyspace to a file — a point-in-time image. Simple, compact, fast to load. The cost: anything written since the last snapshot is lost on a crash.
- **Append-only log**: append every *write command* to a file as it happens; rebuild state by replaying the log on boot. Far better durability (you can fsync per write), but the file grows and needs compaction. (This is a stretch goal — and it's the same append-only-log idea as a database WAL, coming in Module 10.)

The non-negotiable detail is **`fsync`**. `write()` returning `Ok` only means the bytes reached the OS **page cache** — they may still be in RAM when the power dies. `file.sync_all()` (an `fsync` syscall) forces them to the physical disk. "Durable" means *after* the fsync returns. Skipping it is the most common way a "persistent" store silently isn't.

### Pill 11: Memory-Mapped I/O (`mmap`)

To load the snapshot back, you *could* `read()` the whole file into a `Vec`. Instead you'll **memory-map** it: ask the kernel to map the file's bytes directly into your process's address space, so the file *is* a `&[u8]` you index into. Pages are faulted in from disk by the virtual-memory system **on demand** as you touch them — no explicit read loop, no second copy in your heap.

```rust
let file = File::open(path)?;
let mmap = unsafe { Mmap::map(&file)? };   // the file as a slice
let bytes: &[u8] = &mmap;                  // parse straight out of this
```

The `unsafe` is honest, not ceremonial: the borrow checker cannot prevent *another process* from truncating the file while you hold the mapping, which would turn a read into a `SIGBUS` crash. For a load-once-at-boot snapshot, that risk is acceptable and the win is real: zero-copy, and the OS page cache is shared, so two processes mapping the same file share physical RAM. mmap shines for *random access over a large, mostly-read file* (databases lean on it heavily); it's a poor fit for streaming sequential writes or tiny files, where the page-fault and TLB overhead doesn't pay off.

### Pill 12: Signals and Graceful Shutdown

A signal (`SIGINT` from Ctrl-C, `SIGTERM` from `kill` or an orchestrator) interrupts your process asynchronously. The trap: a signal handler runs *between two arbitrary machine instructions*, so it may **only** call **async-signal-safe** functions — it cannot lock a mutex, allocate, or touch your `HashMap`, because the main code might be halfway through doing exactly that. Doing real work in a handler is a classic source of deadlocks and corruption.

The portable fix is the **self-pipe trick**: the handler does the one safe thing it can — `write()` a byte to a pipe — and your normal code reads that pipe to learn "a signal happened," doing the actual work outside handler context. `signal-hook` implements this correctly, and `signal-hook-mio` exposes the read end as a `mio` source, so **a signal becomes just another readable event in your event loop**:

```rust
let mut signals = Signals::new([SIGINT, SIGTERM])?;
poll.registry().register(&mut signals, Token(1), Interest::READABLE)?;
// in the loop:
Token(1) => for sig in signals.pending() { /* break the loop, snapshot, exit */ }
```

That uniformity is the elegant part: no separate signal-handling path, no `volatile` flag polled in a busy loop — shutdown is handled by the same `poll()` that handles sockets. **Graceful** shutdown then means: stop the loop, finish the snapshot, close cleanly — instead of dying mid-write and corrupting the file.

### Pill 13: Testing a Network Server and the Benchmark

You can test this server **without mocks** because the real interface *is* a socket: bind on port 0 (the OS picks a free port), start the loop on a thread, and connect a plain `TcpStream` client that speaks RESP. The tests assert on exact reply bytes (`+PONG\r\n`, `$3\r\nbar\r\n`), and one test sends two commands in a single `write` to prove **pipelining** works — that your parse loop (Pill 6) drains *all* buffered commands per read, not just the first.

**The benchmark is the deliverable, and it argues for the architecture.** Measure RESP **parse throughput** (commands/sec one core can frame) against **store GET/SET** cost. The store ops are nanoseconds; parsing and the syscalls around it dominate. That contrast is the whole reason an in-memory database is **I/O-bound, not CPU-bound**, which is the whole reason Redis is single-threaded — adding cores wouldn't help the bottleneck. The benchmark turns that claim into two numbers.

## Project: `rudis` — a Redis-compatible server

A single-threaded, event-driven key-value server that speaks enough RESP for `redis-cli`:

```text
PING [msg]                 -> +PONG  (or the message echoed)
ECHO msg                   -> msg
SET key value [EX s|PX ms] -> +OK
GET key                    -> value  (or nil)
DEL key [key ...]          -> (integer) number removed
EXISTS key [key ...]       -> (integer) number present
INCR key                   -> (integer) new value
EXPIRE key seconds         -> (integer) 1 if set, 0 if missing
TTL key                    -> (integer) seconds left, -1 no expiry, -2 missing
DBSIZE                     -> (integer) key count
SAVE                       -> +OK     (writes a snapshot)
COMMAND ...                -> (array)  (handshake redis-cli sends on connect)
```

Why it's the right vehicle for this module:

- **Every skill is load-bearing.** The event loop (`mio`), non-blocking buffered I/O, a hand-parsed wire protocol, expiry, snapshot persistence, mmap load, and signal-driven shutdown — drop any one and you don't have a server, you have a fragment.
- **It's a real protocol, not a toy one.** You test against the actual `redis-cli`. "Speaks a published wire protocol correctly" is a portfolio sentence; "parses my custom format" isn't.
- **It makes `tokio` legible.** Having written the loop by hand, you'll read async Rust knowing exactly which part is the readiness poll, which is the buffer management, and which is the sugar.
- **Testable without a fake.** The interface is a socket; the tests use a socket. No mock of the network, the real thing in-process.

### Requirements

1. **Error type** (`error.rs`, *given*): one `Error` enum (`Protocol`/`Snapshot`/`Io`) with `#[from]`, and a `Result` alias.
2. **RESP** (`resp.rs`): the `Resp` enum + constructors are *given*; implement **`parse`** (incremental, returns bytes-consumed) and **`encode`** (Pills 5 & 6).
3. **Store** (`store.rs`): the type, clock, active-expiry sweep, and snapshot hooks are *given*; implement `get`/`set`/`del`/`exists`/`incr`/`expire`/`ttl` with **lazy expiry** (Pills 8 & 9).
4. **Commands** (`command.rs`): the `Command` enum + `args_of` helper are *given*; implement **`parse`** (dispatch + arity) and **`execute`** (Pill 8).
5. **Connection** (`connection.rs`): the buffers are *given*; implement `read_into_buf`, `process`, `flush_write_buf` — the non-blocking I/O core (Pills 6 & 7).
6. **Persistence** (`persistence.rs`): the format + LE helpers are *given*; implement **`save`** (write + fsync) and **`load`** (mmap + parse) (Pills 10 & 11).
7. **Server** (`server.rs`, *given*): the `mio` event loop — accept, readiness dispatch, interest re-arming, timer tick, signal shutdown (Pills 4 & 12).
8. **Binary** (`src/bin/rudis.rs`, *given*): the worked entrypoint — config, load snapshot, bind, run.
9. **Benchmark** (`benches/resp.rs`, *given*): parse throughput vs store ops — the deliverable (Pill 13).
10. **Tests** (`tests/integration.rs`, *given*): a real RESP client over a real socket, green as you implement each step.

### Starter files

- `Cargo.toml` — `mio`, `signal-hook`(+`-mio`), `memmap2`, `thiserror`; `criterion` dev-dep; `[[bin]]` and `[[bench]] harness = false` wired.
- `src/lib.rs` — module declarations + re-exports.
- `src/error.rs` — the shared `Error`/`Result` (*given*).
- `src/resp.rs` — `Resp` enum + constructors (*given*); `parse`/`encode` **(stubbed)**.
- `src/store.rs` — `Store`/`Entry`, clock, `purge_expired`, snapshot hooks (*given*); the ops **(stubbed)**.
- `src/command.rs` — `Command` enum + `args_of` (*given*); `parse`/`execute` **(stubbed)**.
- `src/connection.rs` — the buffers (*given*); `read_into_buf`/`process`/`flush_write_buf` **(stubbed)**.
- `src/persistence.rs` — format + LE helpers (*given*); `save`/`load` **(stubbed)**.
- `src/server.rs` — the `mio` event loop (*given*).
- `src/bin/rudis.rs` — the worked entrypoint (*given*).
- `benches/resp.rs` — parse vs store benchmark (*given*).
- `tests/integration.rs` — full-stack tests over a socket (*given*).

### Local setup

No database, no services — just the Rust toolchain. For manual testing you'll want `redis-cli` (from the `redis` package: `brew install redis` / `apt install redis-tools`), but it's optional — the integration tests use a built-in client.

```bash
cargo test          # spins up the server on an ephemeral port and drives it over TCP
cargo run           # starts the server on BIND_ADDR (default 127.0.0.1:6379)
cargo bench         # RESP parse throughput vs store ops — the deliverable

# with the server running, in another terminal:
redis-cli -p 6379 ping
redis-cli -p 6379 set foo bar
redis-cli -p 6379 get foo
```

### Your task

Implement in this order — each step makes more of the test suite pass:

1. **RESP (`resp.rs`)**: `parse` (return `Some((value, consumed))` / `None` for partial / `Err` for bad) and `encode`. Get *bytes-consumed* exactly right.
2. **Store (`store.rs`)**: `get`/`set`/`del`/`exists`/`incr`/`expire`/`ttl`, each honouring lazy expiry.
3. **Commands (`command.rs`)**: `parse` (uppercase the name, check arity, fold `EX`/`PX` into an absolute deadline) and `execute` (one `match` → `Resp`).
4. **Connection (`connection.rs`)**: `read_into_buf` (read until `WouldBlock`), `process` (the parse→execute→encode loop), `flush_write_buf` (write until `WouldBlock`).
5. **Persistence (`persistence.rs`)**: `save` (serialize + `sync_all`) and `load` (mmap + parse with the bounds-checked helpers).
6. **Run it**: `cargo run`, then drive it with `redis-cli`. **Green the tests**: `cargo test`. **Read the benchmark**: `cargo bench`, and re-read Pill 13.

### Hints

<details>
<summary>Hint for step 1 (the RESP parser)</summary>

A CRLF-finder keeps `parse` readable, and the bulk/array cases reuse it:

```rust
fn find_crlf(b: &[u8]) -> Option<usize> {
    b.windows(2).position(|w| w == b"\r\n")   // index of the '\r'
}

pub fn parse(buf: &[u8]) -> Result<Option<(Resp, usize)>> {
    if buf.is_empty() { return Ok(None); }
    let line_end = match find_crlf(buf) { Some(i) => i, None => return Ok(None) };
    let line = &buf[1..line_end];                  // payload after the type byte
    let after_line = line_end + 2;                 // skip the CRLF

    match buf[0] {
        b'+' => Ok(Some((Resp::Simple(str_of(line)?), after_line))),
        b':' => Ok(Some((Resp::Integer(int_of(line)?), after_line))),
        b'$' => {
            let n: i64 = int_of(line)?;
            if n < 0 { return Ok(Some((Resp::Null, after_line))); }
            let n = n as usize;
            let end = after_line + n;
            if buf.len() < end + 2 { return Ok(None); }   // need value + CRLF
            Ok(Some((Resp::Bulk(buf[after_line..end].to_vec()), end + 2)))
        }
        b'*' => {
            let count: i64 = int_of(line)?;
            if count < 0 { return Ok(Some((Resp::Null, after_line))); }
            let mut consumed = after_line;
            let mut items = Vec::new();
            for _ in 0..count {
                match Resp::parse(&buf[consumed..])? {
                    Some((v, used)) => { items.push(v); consumed += used; }
                    None => return Ok(None),               // array incomplete
                }
            }
            Ok(Some((Resp::Array(items), consumed)))
        }
        b'-' => Ok(Some((Resp::Error(str_of(line)?), after_line))),
        other => Err(Error::protocol(format!("bad type byte: {other:#x}"))),
    }
}
```

`str_of`/`int_of` are one-liners over `std::str::from_utf8` + `.parse()`, mapping errors to `Error::protocol`.
</details>

<details>
<summary>Hint for step 1 (encode)</summary>

```rust
pub fn encode(&self, out: &mut Vec<u8>) {
    match self {
        Resp::Simple(s) => { out.push(b'+'); out.extend_from_slice(s.as_bytes()); out.extend_from_slice(b"\r\n"); }
        Resp::Error(s)  => { out.push(b'-'); out.extend_from_slice(s.as_bytes()); out.extend_from_slice(b"\r\n"); }
        Resp::Integer(i)=> { out.push(b':'); out.extend_from_slice(i.to_string().as_bytes()); out.extend_from_slice(b"\r\n"); }
        Resp::Bulk(b)   => {
            out.push(b'$'); out.extend_from_slice(b.len().to_string().as_bytes()); out.extend_from_slice(b"\r\n");
            out.extend_from_slice(b); out.extend_from_slice(b"\r\n");
        }
        Resp::Null      => out.extend_from_slice(b"$-1\r\n"),
        Resp::Array(v)  => {
            out.push(b'*'); out.extend_from_slice(v.len().to_string().as_bytes()); out.extend_from_slice(b"\r\n");
            for item in v { item.encode(out); }
        }
    }
}
```
</details>

<details>
<summary>Hint for step 3 (lazy expiry in get)</summary>

Do the expiry check on read, and delete in place so the map stays clean:

```rust
pub fn get(&mut self, key: &[u8]) -> Option<&[u8]> {
    let now = now_ms();
    let expired = matches!(self.map.get(key), Some(e) if e.expires_at.map_or(false, |t| now >= t));
    if expired { self.map.remove(key); return None; }
    self.map.get(key).map(|e| e.value.as_slice())
}
```

`exists` is then just `self.get(key).is_some()`. `incr`: `get` → parse ASCII i64 (default 0) → `set` back, preserving the TTL by reading `expires_at` first.
</details>

<details>
<summary>Hint for step 4 (the non-blocking loops)</summary>

`read_into_buf` and `flush_write_buf` are the same shape — loop until `WouldBlock`:

```rust
pub fn read_into_buf(&mut self) -> io::Result<bool> {
    let mut tmp = [0u8; 16 * 1024];
    loop {
        match self.stream.read(&mut tmp) {
            Ok(0) => return Ok(true),                                   // EOF
            Ok(n) => self.read_buf.extend_from_slice(&tmp[..n]),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}
```

`process` ties it together:

```rust
pub fn process(&mut self, store: &mut Store) -> Result<()> {
    loop {
        match Resp::parse(&self.read_buf)? {
            Some((value, consumed)) => {
                self.read_buf.drain(..consumed);
                match Command::parse(value) {
                    Ok(cmd) => cmd.execute(store).encode(&mut self.write_buf),
                    Err(e)  => Resp::error(format!("ERR {e}")).encode(&mut self.write_buf),
                }
            }
            None => return Ok(()),
        }
    }
}
```
</details>

<details>
<summary>Hint for step 5 (save & the mmap load)</summary>

```rust
pub fn save(store: &Store, path: &Path) -> Result<()> {
    use std::io::Write;
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    for (k, v, exp) in store.snapshot_iter() {
        put_u32(&mut buf, k.len() as u32); buf.extend_from_slice(k);
        put_u32(&mut buf, v.len() as u32); buf.extend_from_slice(v);
        put_u64(&mut buf, exp);
    }
    let mut f = File::create(path)?;
    f.write_all(&buf)?;
    f.sync_all()?;                 // <-- durability (Pill 10)
    Ok(())
}

pub fn load(path: &Path, store: &mut Store) -> Result<()> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let bytes: &[u8] = &mmap;
    if bytes.len() < 8 || &bytes[..8] != MAGIC {
        return Err(Error::Snapshot("bad magic".into()));
    }
    let mut off = 8;
    while off < bytes.len() {
        let (klen, o) = get_u32(bytes, off).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (key, o)  = get_bytes(bytes, o, klen as usize).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (vlen, o) = get_u32(bytes, o).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (val, o)  = get_bytes(bytes, o, vlen as usize).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        let (exp, o)  = get_u64(bytes, o).ok_or_else(|| Error::Snapshot("truncated".into()))?;
        store.load_entry(key.to_vec(), val.to_vec(), exp);
        off = o;
    }
    Ok(())
}
```
</details>

## Stretch goals

- **Append-only persistence (AOF).** Append each write command to a log and fsync it; rebuild by replaying on boot. Add a rewrite/compaction pass. This is a WAL — the on-ramp to Module 10.
- **More types.** Lists (`LPUSH`/`LRANGE`) or hashes (`HSET`/`HGET`) — the keyspace value becomes an enum, and RESP arrays in replies start earning their keep.
- **`MULTI`/`EXEC` transactions.** Queue commands per connection, execute atomically on `EXEC`. Forces you to track per-connection state beyond buffers.
- **Pub/Sub.** `SUBSCRIBE`/`PUBLISH` — now a command on one connection writes to *another* connection's write buffer, which means the event loop must re-arm a *different* token's `WRITABLE` interest. A great event-loop stress test.
- **A CRC per snapshot record**, and a temp-file-plus-`rename` for atomic snapshot replacement (the standard durable-write pattern).
- **`io_uring`** (Linux) instead of `epoll` — completion-based rather than readiness-based I/O. The frontier of high-performance server I/O.

## Key questions

- A client sends `SET foo bar` but your server's `read()` returns only `SET fo`. Walk what your parser and read buffer do, and what happens on the next readiness event. Why does returning `None` from `parse` (not an error) matter here?
- `WouldBlock` shows up on both `read` and `write`. State what it means in each case and what the event loop does in response to each.
- Why register `WRITABLE` interest only when the write buffer is non-empty? Describe the bug if you leave it always on, and the bug if you never set it.
- Your snapshot stores expiry as an absolute timestamp, not a remaining duration. Construct the bug that would appear if you stored the duration instead.
- `file.write_all(&buf)` returned `Ok(())` and then the machine lost power and the data was gone. Explain precisely where the bytes were, and the one call that would have prevented it.
- A signal handler "may only call async-signal-safe functions." Give a concrete corruption that occurs if a handler locked the same mutex your main loop holds — and explain how the self-pipe trick sidesteps it.
- Your benchmark shows store `GET` at tens of nanoseconds and RESP parse at a few hundred nanoseconds per command. Turn that into a one-sentence argument for why this server is single-threaded.

## Resources

- [`mio` docs](https://docs.rs/mio/latest/mio/) and the [TCP server example](https://github.com/tokio-rs/mio/blob/master/examples/tcp_server.rs) — `Poll`, `Token`, `Interest`, the accept/readiness pattern
- [RESP protocol specification](https://redis.io/docs/latest/develop/reference/protocol-spec/) — the wire format, authoritative
- [The Linux Programming Interface](https://man7.org/tlpi/) (Kerrisk) — chapters on `epoll`, non-blocking I/O, and signals; the definitive systems reference
- [`epoll(7)`](https://man7.org/linux/man-pages/man7/epoll.7.html) and [`kqueue(2)`](https://man.freebsd.org/cgi/man.cgi?kqueue) — the readiness APIs `mio` wraps; read about edge- vs level-triggered
- [`signal-hook` docs](https://docs.rs/signal-hook/latest/signal_hook/) — async-signal-safety and the self-pipe trick, done correctly
- [`memmap2` docs](https://docs.rs/memmap2/latest/memmap2/) — the `unsafe` contract on `Mmap::map`
- [Redis persistence](https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/) — RDB (snapshot) vs AOF (append-only log), from the source
- [Redis is single-threaded — why it's fast](https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/) — the I/O-bound argument behind Pill 13
