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

#[test]
fn device_record_cannot_expose_reconciliation_results_or_a_reported_key() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/device_record_does_not_expose_identity_or_device_id.rs");
}

#[test]
fn unreachable_presence_requires_since() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/presence_unreachable_requires_since.rs");
}

#[test]
fn match_evidence_only_cannot_expose_a_device_key() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/match_evidence_only_cannot_expose_device_key.rs");
}
