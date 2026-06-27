# expense-tracker

An interactive command-line expense tracker.

A REPL that records expenses, persists them to JSON, and prints totals, monthly summaries, and CSV exports. Built as the capstone project for a Rust fundamentals course.

## What it does

Reads commands from a prompt, stores each expense (description, amount, category, timestamp), and saves to `expenses.json` after every change. Expenses survive between runs.

## Features

- Add, list, and delete expenses by index.
- Total spending, optionally filtered by category.
- Monthly summary grouped by `YYYY-MM`.
- Export all expenses to a CSV file.
- Persists to `expenses.json` automatically; loads it on startup.
- Bad input is rejected without panicking.

## Example

```
$ cargo run
Expense Tracker 💰
Type 'help' for commands.

> add 4.50 food Coffee
Added: 2026-06-26 Coffee — $4.50 [food]
> list
0: 2026-06-26 Coffee — $4.50 [food]
> total food
Total for [food]: $4.50
> summary
2026-06: $4.50
> quit
Bye!
```

## Commands

| Command | Description |
| --- | --- |
| `add <amount> <category> <description>` | Add an expense |
| `list` (`ls`) | List all expenses with their index |
| `total [category]` | Show total, optionally filtered by category |
| `delete <index>` (`del`, `rm`) | Delete an expense by index |
| `summary` | Totals grouped by month |
| `export <path>` | Export all expenses to a CSV file |
| `help` (`h`) | Show command help |
| `quit` (`q`) | Exit |

## Running it

```bash
# Build
cargo build

# Run the REPL (loads expenses.json from the working directory)
cargo run

# Drive it non-interactively
printf 'list\ntotal\nsummary\nquit\n' | cargo run

# Export the current expenses to CSV
printf 'export out.csv\nquit\n' | cargo run
```

The repo ships sample `expenses.json` (loaded on startup) and `expenses.csv` (an export example). There are no automated tests.

## How it works

`main.rs` runs the REPL: it reads a line, calls `parse_command` to turn it into a `Command` enum, and `match`es each variant to an action. Parsing uses `splitn` and `.parse().ok()?` so malformed input returns `None` rather than crashing.

`expense.rs` holds the `Expense` struct and the data operations. `Expense` derives `serde`'s `Serialize`/`Deserialize` and implements `Display` for the `… — $X.XX [category]` line. Totals use an iterator chain (`filter`/`map`/`sum`); the monthly summary accumulates into a `BTreeMap` via `entry().or_insert()`. Persistence is `serde_json` to `expenses.json`; CSV export uses the `csv` crate. Records loaded from JSON without a `date` field default to `Utc::now()`.

## Project layout

```
src/
  main.rs      REPL loop, Command enum, command parsing
  expense.rs   Expense struct, JSON/CSV I/O, totals, summary
expenses.json  sample data, loaded on startup
expenses.csv   sample CSV export
```

## Status

Implemented and runnable. A teaching project, not a published tool. The store path (`expenses.json`) is hardcoded to the working directory.

The concept pills and the step-by-step build that produced this — covering ownership and borrowing, enums and exhaustive `match`, `Option`/`Result` and the `?` operator, iterators, and `serde` file I/O — live in [`README-LEARN.md`](./README-LEARN.md).
