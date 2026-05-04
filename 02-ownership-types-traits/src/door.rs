//! Typestate `Door`. The state lives in the type parameter, so calling
//! `.open()` on a `Door<Locked>` is a compile error — there is no such method.

use std::marker::PhantomData;

pub struct Locked;
pub struct Unlocked;
pub struct Door<State> {
    key: String,
    _state: PhantomData<State>,
}

// The expected key. In a real library you'd take this in `new`; we hard-code
// it to keep the API focused on the types.
const KEY: &str = "skeleton";

impl Door<Locked> {
    pub fn new() -> Door<Locked> {
        Door {
            key: KEY.to_string(),
            _state: PhantomData,
        }
    }

    pub fn unlock(self, key: &str) -> Result<Door<Unlocked>, Door<Locked>> {
        if key == self.key {
            Ok(Door {
                key: self.key,
                _state: PhantomData,
            })
        } else {
            Err(self)
        }
    }
}

impl Door<Unlocked> {
    pub fn open(&self)  {
        println!("creak…");
    }

    pub fn lock(self) -> Door<Locked> {
        Door {
            key: self.key,
            _state: PhantomData,
        }
    }
}

// TODO: `impl Door<Unlocked>` block. Provide:
//   - `pub fn open(&self)` — print "creak..." or similar; takes `&self`, not `self`.
//   - `pub fn lock(self) -> Door<Locked>` — consumes self, returns a locked door.

// Suppress dead-code warnings until you wire up the real fields above.
#[allow(dead_code)]
fn _phantom_example() {
    let _: PhantomData<u8> = PhantomData;
    let _ = KEY;
}
