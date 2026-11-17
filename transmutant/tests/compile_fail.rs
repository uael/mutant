#[test]
fn compile_fail() {
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/compile-fail/*.rs");
  // A passing case makes trybuild build instead of check, so that the errors of the constants
  // evaluated at monomorphization (every `cannot transmute`) are reported too.
  t.pass("tests/compile-pass/*.rs");
}
