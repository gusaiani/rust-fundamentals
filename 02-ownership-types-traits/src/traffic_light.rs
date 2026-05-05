//! Runtime FSM. The current state is a `Box<dyn State>`; transitions consume
//! the box and return a new one (possibly of a different concrete type).

use std::cell::RefCell;
use std::rc::Rc;

use crate::observer::Observer;

pub trait State {
    fn name(&self) -> &'static str;
    fn next(self: Box<self>) -> Box<dyn State>;
}

pub struct Red;
pub struct Green;
pub struct Yellow;

impl State for Red {
    fn name(&self) -> &'static str { "Red" }
    fn next(self: Box<self>) -> Box<dyn State> { Box::new(Green) }
}

impl State for Green {
    fn name(&self) -> &'static str { "Green" }
    fn next(self: Box<self>) -> Box<dyn State> { Box::new(Yellow) }
}

impl State for Yellow {
    fn name(&self) -> &'static str { "Yellow" }
    fn next(self: Box<self>) -> Box<dyn State> { Box::new(Red) }
}

pub struct TrafficLight {
    state: Option<Box<dyn State>>,
    observers: Vec<Rc<RefCell<dyn Observer>>,
}

impl TrafficLight {
    pub fn new() -> Self {
        Self {
            state: Some(Box::new(Red)),
            observers: Vec::new(),
        }
    }

    pub fn current(&self) -> &'static str {
        self.state.as_ref().expect("state always present between ticks").name()
    }

    pub fn subscribe(&mut self, observer: Rc<RefCell<dyn Observer>>) {
        self.observers.push(observer);
    }
}

// TODO

// Suppress unused-import warnings until the real types reference these.
#[allow(dead_code)]
fn _imports_used(_: Option<Box<dyn State>>, _: Vec<Rc<RefCell<dyn Observer>>>) {}
