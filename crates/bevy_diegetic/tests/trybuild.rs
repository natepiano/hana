//! Compile-pass coverage for the public layout builder API.

#[test]
#[ignore = "slow compile-test diagnostic; run when changing public layout typestate helpers"]
fn typestate_helper_signatures_compile() {
    let test_cases = trybuild::TestCases::new();
    test_cases.pass("tests/trybuild/pass/typestate_helpers.rs");
    test_cases.compile_fail("tests/trybuild/fail/overlay_*.rs");
}
