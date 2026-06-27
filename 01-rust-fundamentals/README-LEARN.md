# Rust in 5-Minute Pills

## Goal
Go from zero to writing a working CLI tool in Rust, one short pill at a time.

## Time estimate
~3 hours total (15-20 pills × 5 minutes each)

## What you'll learn
- Ownership, borrowing, and lifetimes — Rust's core mental model
- Pattern matching and enums as a replacement for null/exceptions
- Structs, traits, and impl blocks
- Error handling with `Result` and `Option`
- Iterators and closures
- Reading files and CLI arguments
- Building a real tool: a personal expense tracker CLI

## Concepts

### Pill 1: Hello, Cargo
Rust's build tool is `cargo`. Every project is a "crate."

```bash
# You already have this crate — just run:
cargo run
```

The entry point is always `fn main()` in `src/main.rs`. Rust compiles to a native binary — no runtime, no VM.

### Pill 2: Variables and Mutability
Variables are **immutable by default**. You opt into mutability.

```rust
let x = 5;        // immutable
let mut y = 10;   // mutable — can be reassigned
y = 20;           // ok
// x = 6;         // compile error!
```

`let` bindings can shadow previous ones — this is idiomatic:
```rust
let x = "42";
let x: i32 = x.parse().unwrap(); // shadow with a new type
```

### Pill 3: Types and Functions
Rust is statically typed. Type annotations use `:` after the name.

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b  // no semicolon = this is the return value
}
```

Common types: `i32`, `f64`, `bool`, `String`, `&str` (string slice), `Vec<T>`, `Option<T>`, `Result<T, E>`.

### Pill 4: Ownership — The Big Idea
Every value has exactly **one owner**. When the owner goes out of scope, the value is dropped (freed).

```rust
let s1 = String::from("hello");
let s2 = s1;          // s1 is MOVED to s2
// println!("{s1}");   // compile error — s1 is gone
println!("{s2}");      // ok
```

This is how Rust avoids garbage collection AND use-after-free. No runtime cost.

### Pill 5: Borrowing
Instead of moving, you can **borrow** with `&`:

```rust
fn print_len(s: &String) {  // borrows s, doesn't own it
    println!("len = {}", s.len());
}

let s = String::from("hello");
print_len(&s);    // lend s
println!("{s}");   // still valid — we only lent it
```

Rules: you can have **many `&T`** (shared borrows) OR **one `&mut T`** (exclusive borrow), never both at the same time.

### Pill 6: Structs
```rust
struct Expense {
    description: String,
    amount: f64,
    category: String,
}

impl Expense {
    fn new(description: &str, amount: f64, category: &str) -> Self {
        Self {
            description: description.to_string(),
            amount,
            category: category.to_string(),
        }
    }
}
```

### Pill 7: Enums and Pattern Matching
Enums in Rust can hold data — they replace union types, nulls, and exceptions.

```rust
enum Command {
    Add { description: String, amount: f64, category: String },
    List,
    Total,
    Quit,
}
```

Use `match` to handle every variant — the compiler forces exhaustiveness:
```rust
match command {
    Command::Add { description, amount, category } => { /* ... */ }
    Command::List => { /* ... */ }
    Command::Total => { /* ... */ }
    Command::Quit => break,
}
```

### Pill 8: Option and Result
Rust has no `null`. Instead:

```rust
// Option<T> = Some(value) | None
let maybe: Option<i32> = "42".parse().ok();

// Result<T, E> = Ok(value) | Err(error)
let result: Result<i32, _> = "42".parse();
```

The `?` operator propagates errors — like early return on Err:
```rust
fn read_file(path: &str) -> Result<String, std::io::Error> {
    let content = std::fs::read_to_string(path)?; // returns Err if it fails
    Ok(content)
}
```

### Pill 9: Vectors and Iterators
```rust
let expenses = vec![100.0, 50.0, 75.0];

// Iterator chain — lazy, zero-cost abstraction
let total: f64 = expenses.iter().sum();
let big: Vec<&f64> = expenses.iter().filter(|&&x| x > 60.0).collect();
```

Common iterator methods: `.map()`, `.filter()`, `.find()`, `.any()`, `.fold()`, `.collect()`.

### Pill 10: String Types
Two main string types:
- `String` — owned, heap-allocated, growable
- `&str` — borrowed slice, usually a reference into a `String` or a string literal

```rust
let owned: String = String::from("hello");
let slice: &str = &owned;        // borrow as slice
let literal: &str = "hello";     // string literals are &str
```

Rule of thumb: accept `&str` in function parameters, store `String` in structs.

### Pill 11: Reading User Input
```rust
use std::io::{self, Write};

fn prompt(msg: &str) -> String {
    print!("{msg}");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}
```

### Pill 12: File I/O and Serde
For persistence, we'll use JSON via the `serde` crate:

```rust
// In Cargo.toml:
// [dependencies]
// serde = { version = "1", features = ["derive"] }
// serde_json = "1"

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Expense {
    description: String,
    amount: f64,
    category: String,
}
```

Then save/load:
```rust
let json = serde_json::to_string_pretty(&expenses)?;
std::fs::write("expenses.json", &json)?;

let data = std::fs::read_to_string("expenses.json")?;
let expenses: Vec<Expense> = serde_json::from_str(&data)?;
```

## Project: Expense Tracker CLI

A command-line expense tracker that reads commands interactively, stores expenses in a JSON file, and prints summaries. Something you'd actually use day-to-day.

### Requirements
1. Add an expense with a description, amount, and category
2. List all expenses in a formatted table
3. Show total spending, optionally filtered by category
4. Persist expenses to `expenses.json` between runs
5. Handle bad input gracefully (no panics on invalid numbers)

### Starter files
- `src/main.rs` — entry point with a REPL loop and command parsing stubs
- `src/expense.rs` — the `Expense` struct and file I/O stubs
- `Cargo.toml` — project config with serde dependencies

### Your task
1. Implement `Expense::new()` and the `Display` trait for `Expense`
2. Implement `save_expenses()` and `load_expenses()` using serde_json
3. Implement `parse_command()` to turn user input into a `Command` enum
4. Wire up the REPL: match each command and execute the right action
5. Implement `total_by_category()` using iterators
6. Add error handling — replace `.unwrap()` calls with proper `Result` handling

### Hints

<details>
<summary>Hint for step 1</summary>
Implement `std::fmt::Display` for `Expense` so you can use it in `println!("{expense}")`. Format it like: `Coffee — $4.50 [food]`
</details>

<details>
<summary>Hint for step 3</summary>
Split the input line by whitespace. The first word is the command name. For `add`, the format is: `add <amount> <category> <description...>`. Use `.splitn(4, ' ')` to limit splits.
</details>

<details>
<summary>Hint for step 5</summary>
Use `.iter().filter().map().sum()` chain. Filter by category, map to amount, then sum.
</details>

<details>
<summary>Hint for step 6</summary>
Create a custom error type or use `Box<dyn std::error::Error>` as your error type. Change `main()` to return `Result<(), Box<dyn std::error::Error>>`.
</details>

## Stretch goals
- Add a `delete <index>` command
- Add date tracking with `chrono` crate and a `summary` command that groups by month
- Export to CSV

## Key questions
- Why does Rust distinguish between `String` and `&str`? When would you use each?
- What happens if you try to use a variable after it's been moved?
- How does `?` differ from `.unwrap()`?
- Why can't you have `&T` and `&mut T` at the same time?
- What's the advantage of `match` being exhaustive?

## Resources
- [The Rust Book](https://doc.rust-lang.org/book/) — the official guide, excellent
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) — learn by reading small programs
- [std library docs](https://doc.rust-lang.org/std/) — searchable standard library reference
