//! Compile-pass coverage for the public layout builder API.

#[test]
fn typestate_helper_signatures_compile() {
    let test_cases = trybuild::TestCases::new();
    test_cases.pass("tests/trybuild/pass/typestate_helpers.rs");
}
