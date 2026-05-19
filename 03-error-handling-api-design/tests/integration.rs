use envguard::{Error, Loader};

#[test]
fn happy_path_loads_typed_values() {
    // Each test owns its own prefix so parallel runs don't collide.
    std::env::set_var("T1_PORT", "8080");
    std::env::set_var("T1_LABEL", "hello");

    let env = Loader::new()
        .require::<u16>("T1_PORT")
        .require::<String>("T1_LABEL")
        .load()
        .expect("schema should load with both vars set");

    let port: u16 = env.get("T1_PORT").expect("PORT in bag");
    let label: String = env.get("T1_LABEL").expect("LABEL in bag");

    assert_eq!(port, 8080);
    assert_eq!(label, "hello");
}

#[test]
fn missing_required_var_is_reported() {
    // Belt and suspenders — parallel tests share process env, so make sure
    // a leftover value from elsewhere isn't masking the "missing" case.
    std::env::remove_var("T2_PORT");

    let result = Loader::new().require::<u16>("T2_PORT").load();

    let errors = result.expect_err("missing var should fail load");

    // One required var, one problem.
    assert_eq!(errors.len(), 1);

    // Pattern-match on the single error to assert *which kind* it is and
    // that it carries the right var name. `matches!` returns a bool — handy
    // when you don't need to bind the inner fields.
    assert!(
        matches!(&errors[0], Error::Missing { var } if var == "T2_PORT"),
        "expected Error::Missing for T2_PORT, got {:?}",
        errors[0],
    );
}

#[test]
fn parse_failure_is_reported_with_source() {
    std::env::set_var("T3_PORT", "not-a-number");

    let result = Loader::new().require::<u16>("T3_PORT").load();

    let errors = result.expect_err("garbage value should fail load");
    assert_eq!(errors.len(), 1);

    // let-else binds the inner fields or bails — cleaner than `match` with
    // one real arm, and lets us reach into `source` (matches! can't)
    let Error::Parse { var, source } = &errors[0] else {
        panic!("expected Error::Parse for T3_PORT, got {:?}", errors[0]);
    };

    assert_eq!(var, "T3_PORT");
    // ParseIntError's Display is "invalid digit found in string".
    assert!(
        source.to_string().contains("invalid digit"),
        "source should mention the integer parse failure: got: {source}"
    );
}
