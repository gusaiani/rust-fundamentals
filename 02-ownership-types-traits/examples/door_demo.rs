// Run with: `cargo run --example door_demo`

use fsm::{Door, Locked, Unlocked};

fn main() {
    // 1. Brand-new doors are always locked. The type tells us so: `Door<Locked>`.
    let door: Door<Locked> = Door::new();
    println!("door created (locked)");

    // 2. Wrong key - `unlock` consumed the door, so we get it back via Err.
    let door = match door.unlock("wrong") {
        Ok(_unlocked) => unreachable!("wrong key should not unlock"),
        Err(still_locked) => {
            println!("rejected wrong key");
            still_locked // shadow `door` with the locked one we got back
        }
    };

    // 3. Right key — `Ok` gives us a `Door<Unlocked>`.
    let door: Door<Unlocked> = match door.unlock("skeleton") {
        Ok(unlocked) => unlocked,
        Err(_) => unreachable!("skeleton is the right key"),
    };
    println!("unlocked");

    // 4. `.open()` only exists on `Door<Unlocked>`. Calling it on a
    //    `Door<Locked> would be a compile error — that's the point.
    door.open();

    // 5. Re-lock. `lock(self)` consumes the unlocked door and gives a `Door<Locked>` back.
    let _door: Door<Locked> = door.lock();
    println!("locked again");
}
