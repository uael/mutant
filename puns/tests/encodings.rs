//! Runs `examples/encodings.rs`.

#[path = "../examples/encodings.rs"]
mod encodings;

#[test]
fn encodings() {
  encodings::main();
}
