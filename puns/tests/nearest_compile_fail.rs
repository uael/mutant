//! What is refused of values that hold pointers (feature `nearest`), with the messages.
//!
//! Not for Miri: trybuild runs the compiler.

#![cfg(not(miri))]
#![cfg(feature = "nearest")]

#[test]
fn compile_fail() {
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/compile-fail-nearest/*.rs");
  // A passing case makes trybuild build instead of check, so that the errors of the constants
  // evaluated at monomorphization (every `cannot pun`) are reported too.
  t.pass("tests/compile-pass-nearest/*.rs");
}
