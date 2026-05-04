// Drive the runtime traffic light with a logging observer.
//
// Run with: `cargo run --example traffic_light_demo`

use std::cell::RefCell;
use std::rc::Rc;

// TODO: import what you need from the `fsm` crate.
// use fsm::{LogObserver, TrafficLight};

fn main() {
    // TODO:
    //   1. Build a `TrafficLight`.
    //   2. Wrap a `LogObserver` in `Rc::new(RefCell::new(...))`. Keep one
    //      handle for yourself before subscribing — `Rc::clone` it so both
    //      the FSM and `main` own a copy.
    //   3. Subscribe the observer.
    //   4. Tick the light a handful of times.
    //   5. Borrow the observer and print every entry from its log.
    let _ = (Rc::new(RefCell::new(0u8)),); // keep imports alive until you wire it up
    todo!()
}
