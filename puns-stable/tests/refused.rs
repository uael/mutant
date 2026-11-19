//! What does not compile without the `unstable` feature, and the message that tells why; and
//! what Rust keeps, which compiles (a passing case also makes trybuild build rather than check:
//! refusals are evaluated at monomorphization).
//!
//! Not built under Miri: trybuild runs the compiler, which Miri cannot.

#![cfg(not(miri))]

#[test]
fn refused_without_the_feature() {
  if puns::layout::UNSTABLE {
    eprintln!("`puns` has the `unstable` feature: nothing to check (`cargo test -p puns-stable`)");
    return;
  }
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/refused/*.rs");
  t.pass("tests/kept/*.rs");
}
