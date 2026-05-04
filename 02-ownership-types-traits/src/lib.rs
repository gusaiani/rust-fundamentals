// Crate root. Two flavors of state machine:
//
//   - `door`          — typestate (compile-time) FSM.
//   - `traffic_light` — trait-object (runtime) FSM with observers.
//
// Pull the public types up to the crate root so users can write
// `use fsm::{Door, Locked, Unlocked, TrafficLight, Observer, LogObserver};`.

pub mod door;
pub mod observer;
pub mod traffic_light;

// TODO: re-export the public items from the submodules here.
// pub use door::{...};
// pub use observer::{...};
// pub use traffic_light::{...};
