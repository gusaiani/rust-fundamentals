use std::cell::RefCell;
use std::rc::Rc;

use fsm::{Door, Locked, LogObserver, Observer, TrafficLight};

#[test]
fn door_locks_and_unlocks() {
    let door = Door::new();

    let result = door.unlock("definitely-not-the-key");
    assert!(result.is_err(), "wrong key should not unlock");

    // Recover the locked door from the Err arm — `unlock` consumed it
    let door = match result {
        Err(locked) => locked,
        Ok(_) => unreachable!(), // we just asserted is_err()
    };

    // Right key — should give us a Door<Unlocked>.
    let unlocked = match door.unlock("skeleton") {
        Ok(d) => d,
        Err(_) => panic!("right key should unlock"),
    };

    // lock() consumes the unlocked door and returns Door<Locked>.
    // We're not asserting anything about it — the test passes if this
    // compiles and runs. The TYPE (Door<Locked>) is the assertion;
    // the binding makes that explicit.
    let _relocked: Door<Locked> = unlocked.lock();
}

#[test]
fn traffic_light_cycles_and_notifies() {
    let mut light = TrafficLight::new();
    assert_eq!(light.current(), "Red", "should start at Red");

    // Concrete-typed handle so we can call `.log()` later.
    let observer: Rc<RefCell<LogObserver>> = Rc::new(RefCell::new(LogObserver::new()));

    // Coerce to the trait-object handle the FSM expects.
    let subscriber: Rc<RefCell<dyn Observer>> = observer.clone();
    light.subscribe(subscriber);

    // Drive a full cycle.
    for _ in 0..3 {
        light.tick();
    }

    // Borrow the inner LogObserver shared (read-only). Bind it so the
    // `Ref` lives long enough for the comparison below.
    let log = observer.borrow();
    let expected = ["Red->Green", "Green->Yellow", "Yellow->Red"];
    assert_eq!(log.log(), expected);

    // The cycle wraps — after three tickes we're back on Red.
    assert_eq!(light.current(), "Red");
}
