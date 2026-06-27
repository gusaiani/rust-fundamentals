# fsm

A type-safe state machine library demonstrating compile-time and runtime FSMs.

Two flavors of finite state machine in one crate: a **typestate** `Door` whose
state lives in the type (invalid transitions don't compile) and a **trait-object**
`TrafficLight` whose state is chosen at runtime, with an observer hook for side effects.

## Example

```rust
use std::cell::RefCell;
use std::rc::Rc;

use fsm::{Door, Locked, Unlocked, LogObserver, Observer, TrafficLight};

// Typestate FSM: the type tracks the state.
let door: Door<Locked> = Door::new();
let door: Door<Unlocked> = door.unlock("skeleton").expect("right key");
door.open();                 // only exists on Door<Unlocked>
let _locked: Door<Locked> = door.lock();
// door.open() on a Door<Locked> would NOT compile.

// Runtime FSM: state is a Box<dyn State>, transitions picked at runtime.
let mut light = TrafficLight::new();
assert_eq!(light.current(), "Red");

let observer: Rc<RefCell<LogObserver>> = Rc::new(RefCell::new(LogObserver::new()));
let subscriber: Rc<RefCell<dyn Observer>> = observer.clone();
light.subscribe(subscriber);

for _ in 0..3 {
    light.tick();            // Red -> Green -> Yellow -> Red
}

assert_eq!(observer.borrow().log(), ["Red->Green", "Green->Yellow", "Yellow->Red"]);
```

`unlock` consumes the door and returns `Result<Door<Unlocked>, Door<Locked>>` — a
wrong key hands the still-locked door back to the caller via `Err`.

## Features

- **Typestate `Door`**: `Locked` / `Unlocked` states encoded with `PhantomData<State>`.
  `open()` exists only on `Door<Unlocked>`; the state machine is enforced at compile time.
- **Runtime `TrafficLight`**: current state held as `Option<Box<dyn State>>`. `tick()`
  consumes the box via `next(self: Box<Self>)` and swaps in the next concrete state.
- **`State` trait**: `name(&self)` plus `next(self: Box<Self>) -> Box<dyn State>` for
  `Red`, `Green`, `Yellow`.
- **Observer hook**: `subscribe` takes `Rc<RefCell<dyn Observer>>`; every `tick`
  notifies all observers with the from/to state names.
- **`LogObserver`**: records each transition into an internal `Vec<String>`, readable
  via `log() -> &[String]`.
- No external dependencies.

## Running it

```bash
cargo run --example door_demo            # drives the typestate door
cargo run --example traffic_light_demo   # ticks the light, prints the log
cargo test                               # integration tests for both machines
cargo build                              # compile the library
```

## How it works

The two machines contrast Rust's two dispatch strategies.

The `Door` uses the **typestate pattern**: `Door<State>` is generic over a marker
type (`Locked` / `Unlocked`) carried by a zero-sized `PhantomData<State>`. Methods
live in separate `impl Door<Locked>` and `impl Door<Unlocked>` blocks, so the set of
callable methods depends on the type — illegal transitions are simply absent and fail
to compile. No runtime checks, no runtime cost.

The `TrafficLight` uses **trait objects** for runtime dispatch. The current state is a
`Box<dyn State>`, stored inside an `Option` so `tick` can `take()` it, call the
consuming `next(self: Box<Self>)`, and put the new box back. Observers are stored as
`Rc<RefCell<dyn Observer>>` — `Rc` for shared ownership (the caller keeps a concrete
handle to read the log), `RefCell` for runtime-checked interior mutability so each
`tick` can `borrow_mut()` and notify through a shared reference.

## Project layout

- `src/lib.rs` — module declarations and re-exports.
- `src/door.rs` — typestate `Door<State>`.
- `src/traffic_light.rs` — `State` trait, state structs, `TrafficLight`.
- `src/observer.rs` — `Observer` trait and `LogObserver`.
- `examples/` — `door_demo`, `traffic_light_demo`.
- `tests/integration.rs` — runtime assertions on both machines.

## Status

Implemented and runnable: both machines work, examples run, and tests pass.

The concept pills and the step-by-step build that produced this — covering generics,
lifetimes, trait objects, `Box`/`Rc`/`Arc`, `RefCell` interior mutability, `PhantomData`,
and the typestate pattern — live in [`README-LEARN.md`](./README-LEARN.md).
