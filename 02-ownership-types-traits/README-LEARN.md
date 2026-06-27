# Ownership, Types & Traits in 5-Minute Pills

## Goal

Use Rust's type system as a correctness tool — encode protocols, ownership, and shared state so that bad programs don't compile. Build a small state-machine library that demonstrates both compile-time and runtime polymorphism.

## Time estimate

~4 hours total (15 pills × 5 minutes + project)

## What you'll learn

- Generics, trait bounds, and `where` clauses
- Lifetimes — what they are, why elision exists, when you must write them
- Trait objects (`dyn Trait`) and dynamic dispatch
- `Box<T>`, `Rc<T>`, `Arc<T>` — owned and shared heap allocation
- Interior mutability with `Cell` and `RefCell`
- `PhantomData<T>` and the typestate pattern
- When to reach for static vs. dynamic dispatch

## Concepts

### Pill 1: Generics

Generics are type parameters. The compiler generates a fresh, fully-typed copy of the function for each concrete `T` you call it with (monomorphization — zero runtime cost).

```rust
fn largest<T: PartialOrd>(items: &[T]) -> &T {
    let mut biggest = &items[0];
    for item in items {
        if item > biggest { biggest = item; }
    }
    biggest
}
```

`<T: PartialOrd>` is a **trait bound** — `T` must implement the `PartialOrd` trait. Without it, `>` doesn't compile.

### Pill 2: Where Clauses

Long bound lists go in a `where` clause for readability:

```rust
fn process<T, U>(a: T, b: U) -> String
where
    T: std::fmt::Display + Clone,
    U: IntoIterator<Item = T>,
{
    /* ... */
}
```

Same meaning, easier to read. Use `where` whenever the inline bounds get noisy.

### Pill 3: Traits

A trait is a set of methods a type promises to provide — like an interface, but more powerful.

```rust
trait Greet {
    fn hello(&self) -> String;          // required
    fn shout(&self) -> String {          // default impl
        self.hello().to_uppercase()
    }
}

impl Greet for &str {
    fn hello(&self) -> String { format!("hi, {self}") }
}
```

You can implement your own traits for foreign types (like `&str`) or foreign traits for your own types — but not both at once (orphan rule).

### Pill 4: Lifetimes — The Idea

A lifetime is a compile-time tag on every reference saying _how long it's valid_. The borrow checker uses lifetimes to make sure references never outlive what they point to.

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

`'a` says: "the returned reference lives at least as long as both inputs." You're not creating a lifetime — you're describing one that already exists.

### Pill 5: Lifetime Elision

Most of the time you don't write lifetimes. The compiler fills them in:

```rust
fn first_word(s: &str) -> &str { /* ... */ }
// is shorthand for:
fn first_word<'a>(s: &'a str) -> &'a str { /* ... */ }
```

You only write lifetimes when the compiler can't guess: multiple input refs, refs in structs, refs in trait bounds.

### Pill 6: Lifetimes in Structs

A struct holding a reference must declare the lifetime:

```rust
struct Excerpt<'a> {
    text: &'a str,
}
```

This means: an `Excerpt` cannot outlive the `&str` it points to. The compiler enforces this at every use site.

### Pill 7: Trait Objects (`dyn Trait`)

Generics give you static dispatch — one copy per type. Sometimes you want one heterogeneous collection:

```rust
trait Shape { fn area(&self) -> f64; }

let shapes: Vec<Box<dyn Shape>> = vec![
    Box::new(Circle { r: 1.0 }),
    Box::new(Square { side: 2.0 }),
];
for s in &shapes { println!("{}", s.area()); }
```

`dyn Shape` is a **trait object** — a fat pointer (data ptr + vtable ptr). Method calls go through the vtable. Slightly slower than generics, but lets you mix types at runtime.

### Pill 8: `Box<T>`

`Box<T>` is the simplest heap allocation: one owner, dropped when it goes out of scope. Use it when:

- You need a known size for a recursive type (`enum List { Cons(i32, Box<List>), Nil }`)
- You want to own a trait object (`Box<dyn Trait>`)
- A value is large and you want to move it cheaply (only the pointer moves)

```rust
let b: Box<i32> = Box::new(5);
println!("{b}"); // auto-deref
```

### Pill 9: `Rc<T>` — Shared Ownership

`Rc<T>` ("reference counted") lets multiple owners share one value on the heap. The value is dropped when the **last** `Rc` to it is dropped.

```rust
use std::rc::Rc;
let a = Rc::new(String::from("hi"));
let b = Rc::clone(&a);  // cheap — just bumps the refcount
println!("count = {}", Rc::strong_count(&a)); // 2
```

`Rc<T>` is **single-threaded only**. Sending it across threads is a compile error.

### Pill 10: `Arc<T>` — Shared Across Threads

`Arc<T>` is `Rc<T>` with atomic refcount updates. Use it when shared data crosses thread boundaries.

```rust
use std::sync::Arc;
let data = Arc::new(vec![1, 2, 3]);
let d2 = Arc::clone(&data);
std::thread::spawn(move || println!("{:?}", d2)).join().unwrap();
```

Slightly slower than `Rc<T>` (atomics aren't free). Use `Rc` when you can, `Arc` when you must.

### Pill 11: Interior Mutability — `Cell` and `RefCell`

Rust's borrow rules are checked at compile time. Sometimes you need to mutate through a shared (`&T`) reference. That's interior mutability.

```rust
use std::cell::RefCell;

let counter = RefCell::new(0);
let r1 = &counter;
let r2 = &counter;
*r1.borrow_mut() += 1;       // mutate through &
*r2.borrow_mut() += 1;
assert_eq!(*counter.borrow(), 2);
```

`RefCell` checks borrow rules at **runtime** instead — it panics if you violate them. `Cell<T>` is similar but for `Copy` types and never panics. Both are single-threaded; for thread-safe interior mutability use `Mutex` or `RwLock`.

### Pill 12: The `Rc<RefCell<T>>` Pattern

`Rc` gives shared ownership; `RefCell` gives interior mutability. Together they let multiple owners mutate one value:

```rust
use std::cell::RefCell;
use std::rc::Rc;

let shared = Rc::new(RefCell::new(vec![1, 2, 3]));
let other = Rc::clone(&shared);
other.borrow_mut().push(4);
assert_eq!(shared.borrow().len(), 4);
```

Common in observers, graphs, and any "many things touching one mutable thing" design.

### Pill 13: `PhantomData<T>`

Sometimes you need a struct to be _generic over a type it doesn't actually store_. `PhantomData<T>` is a zero-sized marker that tells the compiler "pretend I hold a `T`".

```rust
use std::marker::PhantomData;

struct Tagged<Tag> {
    value: u64,
    _tag: PhantomData<Tag>,
}
```

No runtime cost. The `Tag` exists only at the type level — perfect for the typestate pattern.

### Pill 14: The Typestate Pattern

Encode the **state** of a value in its **type** so that invalid operations don't compile.

```rust
struct Locked;
struct Unlocked;

struct Door<State> { _state: PhantomData<State> }

impl Door<Locked> {
    fn unlock(self, key: &str) -> Door<Unlocked> { /* ... */ }
}

impl Door<Unlocked> {
    fn open(&self) { println!("creak"); }
    fn lock(self) -> Door<Locked> { /* ... */ }
}
```

Calling `.open()` on a `Door<Locked>` is a compile error — the method doesn't exist on that type. No runtime check needed.

### Pill 15: Static vs. Dynamic Dispatch — Choosing

|                           | Generics (`T: Trait`) | Trait objects (`dyn Trait`) |
| ------------------------- | --------------------- | --------------------------- |
| Dispatch                  | Static (inlined)      | Dynamic (vtable)            |
| Code size                 | One copy per `T`      | One copy total              |
| Heterogeneous collections | No                    | Yes                         |
| Object safety             | N/A                   | Required                    |
| Speed                     | Faster                | A small indirection         |

Default to generics. Reach for `dyn Trait` when you genuinely need a runtime-mixed collection or to break a generic chain.

## Project: `fsm` — A Type-Safe State Machine Library

You'll build a small library crate that provides **two** styles of finite state machine, then exercise both with example programs.

1. **Static FSM (typestate):** transitions checked at compile time. Calling an invalid transition is a compile error.
2. **Dynamic FSM (trait objects):** transitions chosen at runtime, with an observer hook for side effects.

This is the canonical Rust exercise that forces you to use everything above together.

### Requirements

1. A `Door` type with `Locked` / `Unlocked` states encoded via `PhantomData`. Only an `Unlocked` door can `open()`. The wrong key on `unlock()` returns the door unchanged in the locked state.
2. A `TrafficLight` runtime FSM with `Red → Green → Yellow → Red` transitions. The current state is a `Box<dyn State>`.
3. An `Observer` trait. The traffic light holds `Vec<Rc<RefCell<dyn Observer>>>` and notifies every observer on each transition.
4. A `LogObserver` that records every transition into an internal `Vec<String>`, accessible after the run.
5. Examples in `examples/` that drive both machines from `main`.
6. Integration tests in `tests/` that prove (a) the door API rejects invalid calls at compile time (via `compile_fail` doc tests **or** documented examples) and (b) the traffic light cycles correctly and notifies observers.

### Starter files

- `Cargo.toml` — crate config, no external dependencies needed.
- `src/lib.rs` — module declarations and a few re-exports.
- `src/door.rs` — typestate `Door<State>` with `PhantomData`. Stubs for `new`, `unlock`, `lock`, `open`.
- `src/traffic_light.rs` — runtime FSM. `State` trait, three state structs (`Red`, `Green`, `Yellow`), `TrafficLight` holding the current state and a list of observers.
- `src/observer.rs` — `Observer` trait + `LogObserver` implementation.
- `examples/door_demo.rs` — drive the door through a couple of cycles.
- `examples/traffic_light_demo.rs` — register a `LogObserver`, tick the light a few times, print the log.
- `tests/integration.rs` — runtime assertions on both machines.

### Your task

1. **`Door`:** declare `Locked` and `Unlocked` marker structs. Make `Door<State>` generic with `PhantomData<State>`. Implement `Door::new() -> Door<Locked>`. Then, in two separate `impl` blocks, give `Door<Locked>` an `unlock(self, key: &str) -> Result<Door<Unlocked>, Door<Locked>>` and give `Door<Unlocked>` `lock(self) -> Door<Locked>` and `open(&self)`.
2. **`Observer`:** define `trait Observer { fn on_transition(&mut self, from: &str, to: &str); }`. Implement `LogObserver` with an internal `Vec<String>` and a `log(&self) -> Vec<String>` accessor.
3. **`State` trait:** define a trait with `name(&self) -> &'static str` and `next(self: Box<Self>) -> Box<dyn State>`. Note the `self: Box<Self>` receiver — this consumes the box so each transition can return a different concrete type.
4. **State structs:** `Red`, `Green`, `Yellow`, each implementing `State` with the correct cycle.
5. **`TrafficLight`:** holds `state: Option<Box<dyn State>>` (so you can `.take()` the box during a transition) and `observers: Vec<Rc<RefCell<dyn Observer>>>`. Implement `new()`, `subscribe(&mut self, obs: Rc<RefCell<dyn Observer>>)`, and `tick(&mut self)` that records the from/to names, swaps the state, and notifies every observer.
6. **Examples and tests:** wire it all up. The traffic-light test should register a `LogObserver` (wrapped in `Rc<RefCell<…>>`), tick three times, then assert the log contains `Red→Green`, `Green→Yellow`, `Yellow→Red`.

### Hints

<details>
<summary>Hint for step 1 (typestate)</summary>

`PhantomData` is zero-sized — initialize it with `PhantomData`. The `unlock` signature returns `Result<Door<Unlocked>, Door<Locked>>` so a wrong key gives the locked door back to the caller (you consumed `self`, so you can't just return early).

</details>

<details>
<summary>Hint for step 3 (`self: Box<Self>`)</summary>

Without `self: Box<Self>`, you can't return a _different_ concrete type from `next` — the borrow checker won't let you change the type behind a `Box`. Consuming the `Box` lets you drop the old state and return a freshly-allocated one.

</details>

<details>
<summary>Hint for step 5 (`Option<Box<dyn State>>`)</summary>

`tick` needs to call `next(self: Box<Self>)`, which consumes the box. But you only have `&mut self` on the `TrafficLight`. The trick: store the state as `Option<Box<dyn State>>`, then `self.state.take()` to move it out, transition, and put the new one back with `self.state = Some(new_state)`.

</details>

<details>
<summary>Hint for step 6 (notifying observers)</summary>

To call `on_transition` through `Rc<RefCell<dyn Observer>>`, do `observer.borrow_mut().on_transition(from, to)`. Capture the from-name _before_ swapping the state, the to-name _after_.

</details>

## Stretch goals

- Add a third typestate machine: an HTTP `RequestBuilder<NoUrl>` → `RequestBuilder<HasUrl>` → `Request` that only exposes `.send()` once a URL is set.
- Make the dynamic FSM generic over the event type: `trait State<E> { fn handle(self: Box<Self>, event: E) -> Box<dyn State<E>>; }`.
- Swap `Rc<RefCell<…>>` for `Arc<Mutex<…>>` and tick the light from a background thread.
- Add a `WeakObserver` using `Weak<RefCell<dyn Observer>>` so observers can be dropped without leaking.

## Key questions

- When does the compiler require an explicit lifetime annotation, and when does elision cover you?
- Why is `Rc<T>` not `Send`, and what does `Arc<T>` change to make it so?
- What's the cost of `dyn Trait` vs. a generic `T: Trait`? When is each one the right call?
- Why does the typestate pattern use `PhantomData<State>` instead of an enum field?
- Why does the dynamic FSM's `next` method need `self: Box<Self>` rather than `&mut self`?

## Resources

- [The Rust Book — ch. 10, 15, 17, 19](https://doc.rust-lang.org/book/) — generics, smart pointers, OO patterns, advanced traits
- [The Rustonomicon — PhantomData](https://doc.rust-lang.org/nomicon/phantom-data.html)
- [The Embedded Rust Book — Typestate Programming](https://docs.rust-embedded.org/book/static-guarantees/typestate-programming.html)
- [Jon Gjengset — _Crust of Rust: Smart Pointers and Interior Mutability_](https://www.youtube.com/watch?v=8O0Nt9qY_vo)
- [`std::marker::PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html)
