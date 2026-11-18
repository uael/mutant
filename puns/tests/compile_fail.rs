//! What must not compile, and the message that tells why.
//!
//! Not built under Miri: trybuild runs the compiler, which Miri cannot.

#![cfg(not(miri))]

#[test]
fn compile_fail() {
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/compile-fail/*.rs");
  // A passing case makes trybuild build instead of check, so that the errors of the constants
  // evaluated at monomorphization (every `cannot pun`) are reported too.
  t.pass("tests/compile-pass/*.rs");
}
