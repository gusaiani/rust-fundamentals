//! Observer trait + a logging observer.
//!
//! Observers are stored inside the FSM as `Rc<RefCell<dyn Observer>>` so that
//! the user can keep their own handle to the same observer and read its log
//! after the FSM has been driven.

pub trait Observer {
  fn on_transition(&mut self, from: &str, to: &str);
}

pub struct LogObserver {
  entries: Vec<String>,
}

impl LogObserver {
  pub fn new() -> Self {
    Self { entries: Vec::new() }
  }

  pub fn log(&self) -> &[String] {
    &self.entries
  }
}

impl Observer for LogObserver {
  fn on_transition(&mut self, from: &str, to:&str) {
    self.entries.push(format!("{from}->{to}"));
  }
}

// TODO: `impl LogObserver` with:
//   - `pub fn new() -> Self`
//   - `pub fn log(&self) -> &[String]`

// TODO: `impl Observer for LogObserver` — push `format!("{from}->{to}")`
// into `self.entries` on every call.
