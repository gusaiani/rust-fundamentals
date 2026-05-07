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