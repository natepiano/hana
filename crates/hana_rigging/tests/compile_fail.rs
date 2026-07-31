//! Compile-time API boundaries that providers must not bypass.

#[test]
fn constructor_cannot_bypass_validated_constructors() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/constructor_bypass.rs");
}

#[test]
fn device_kind_requires_a_wildcard_arm() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/device_kind_requires_a_wildcard_arm.rs");
}

#[test]
fn device_identity_cannot_be_compared() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/device_identity_cannot_be_compared.rs");
}

#[test]
fn device_identity_requires_a_wildcard_arm() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/device_identity_requires_a_wildcard_arm.rs");
}
