// End-to-end tests against the public API.
//
// Run with: `cargo test`
//
// Each test sets/unsets its own env vars with a unique prefix to avoid
// stomping on the other tests when `cargo test` runs them in parallel.

// TODO: import what you need from envguard.
// use envguard::{Error, Loader};

#[test]
fn happy_path_loads_typed_values() {
    // TODO:
    //   - std::env::set_var("T1_PORT", "8080");
    //   - std::env::set_var("T1_LABEL", "hello");
    //   - Build a Loader requiring both, call .load().
    //   - Assert env.get::<u16>("T1_PORT") == 8080 and env.get::<String>("T1_LABEL") == "hello".
    todo!()
}

#[test]
fn missing_required_var_is_reported() {
    // TODO:
    //   - Make sure T2_PORT is unset (`std::env::remove_var`).
    //   - Build a Loader requiring T2_PORT, call .load(), expect Err(errors).
    //   - Assert exactly one error and that it's `Error::Missing { var }` for T2_PORT.
    todo!()
}

#[test]
fn parse_failure_is_reported_with_source() {
    // TODO:
    //   - std::env::set_var("T3_PORT", "not-a-number");
    //   - Build a Loader requiring T3_PORT as u16, expect Err(errors).
    //   - Assert one error, `Error::Parse { var, source }`, and that
    //     `source.to_string()` mentions an integer parsing failure.
    todo!()
}
