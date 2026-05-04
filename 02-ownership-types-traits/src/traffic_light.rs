//! Runtime FSM. The current state is a `Box<dyn State>`; transitions consume
//! the box and return a new one (possibly of a different concrete type).

use std::cell::RefCell;
use std::rc::Rc;

use crate::observer::Observer;

// Step 1: define `pub trait State` with two methods.
//   fn name(&self) -> &'static str;
//   fn next(self: Box<Self>) -> Box<dyn State>;
//
// The `self: Box<Self>` receiver consumes the box, which is what lets `next`
// return a different concrete type than the receiver.
//
// TODO: replace this stub.
pub trait State {
    // TODO
}

// Step 2: declare three unit structs `Red`, `Green`, `Yellow` and `impl State`
// for each. Cycle: Red -> Green -> Yellow -> Red.
//
// TODO

// Step 3: define `pub struct TrafficLight` with two private fields:
//   - state: Option<Box<dyn State>>
//   - observers: Vec<Rc<RefCell<dyn Observer>>>
//
// Why `Option`? `tick` needs to call `next(self: Box<Self>)`, which moves the
// box out. Storing the state in an `Option` lets you `.take()` it through
// `&mut self` and put a new one back.
//
// TODO

// Step 4: `impl TrafficLight` with:
//   - `pub fn new() -> Self`                              (start on Red)
//   - `pub fn current(&self) -> &'static str`             (delegates to State::name)
//   - `pub fn subscribe(&mut self, obs: Rc<RefCell<dyn Observer>>)`
//   - `pub fn tick(&mut self)`
//
// `tick` should:
//   1. record the current state name as `from`
//   2. take the box out, call `next` on it, assign the result back
//   3. record the new state name as `to`
//   4. for each observer, call `observer.borrow_mut().on_transition(from, to)`
//
// TODO

// Suppress unused-import warnings until the real types reference these.
#[allow(dead_code)]
fn _imports_used(_: Option<Box<dyn State>>, _: Vec<Rc<RefCell<dyn Observer>>>) {}
