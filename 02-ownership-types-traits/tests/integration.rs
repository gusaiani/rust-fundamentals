// Integration tests for the `fsm` crate.
//
// Run with: `cargo test`

use std::cell::RefCell;
use std::rc::Rc;

// TODO: import the public API you need.
// use fsm::{Door, LogObserver, TrafficLight};

#[test]
fn door_locks_and_unlocks() {
    // TODO:
    //   - `Door::new()` starts locked.
    //   - Wrong key on `unlock` returns Err with the locked door.
    //   - Right key on `unlock` returns Ok with an unlocked door.
    //   - `lock()` on the unlocked door returns a locked door again.
    //
    // (The compile-time guarantee — that `.open()` doesn't exist on a locked
    // door — is checked simply by this file compiling.)
    todo!()
}

#[test]
fn traffic_light_cycles_and_notifies() {
    // TODO:
    //   - Build a TrafficLight (starts on Red).
    //   - Wrap a LogObserver in Rc<RefCell<...>>, keep a clone, subscribe.
    //   - Tick three times.
    //   - Assert the observer recorded:
    //       ["Red->Green", "Green->Yellow", "Yellow->Red"]
    let _ = Rc::new(RefCell::new(0u8));
    todo!()
}
