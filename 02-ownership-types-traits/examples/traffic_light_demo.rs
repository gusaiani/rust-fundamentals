// Run with: `cargo run --example traffic_light_demo`

use std::cell::RefCell;
use std::rc::Rc;

use fsm::{LogObserver, Observer, TrafficLight};

fn main() {
    let mut light = TrafficLight::new();
    println!("starting at: {}", light.current());

    // keep the concrete type on the original handle so we can call .log() later
    let observer: Rc<RefCell<LogObserver>> = Rc::new(RefCell::new(LogObserver::new()));

    // explicit target type → coercion site → Rc<RefCell<LogObserver>> becomes
    // Rc<RefCell<dyn Observer>>. Refcount bump only; no deep clone.
    let subscriber: Rc<RefCell<dyn Observer>> = observer.clone();
    light.subscribe(subscriber);

    // Drive the light through a full cycle.
    for _ in 0..3 {
        light.tick();
    }

    // `borrow()` gives a runtime-checked shared borrow of the inner LogObserver.
    // It panics only if a `borrow_mut()` is currently outstanding — none is here.
    let log = observer.borrow();
    for entry in log.log() {
        println!("{entry}");
    }
}
