//! Observer trait + a logging observer.
//!
//! Observers are stored inside the FSM as `Rc<RefCell<dyn Observer>>` so that
//! the user can keep their own handle to the same observer and read its log
//! after the FSM has been driven.

// TODO: define `pub trait Observer` with a single method:
//   fn on_transition(&mut self, from: &str, to: &str);
// pub trait Observer { ... }

// TODO: define `pub struct LogObserver` with one private field:
//   entries: Vec<String>
// pub struct LogObserver { ... }

// TODO: `impl LogObserver` with:
//   - `pub fn new() -> Self`
//   - `pub fn log(&self) -> &[String]`

// TODO: `impl Observer for LogObserver` — push `format!("{from}->{to}")`
// into `self.entries` on every call.
