//! Runs `examples/link.rs`.

#[path = "../examples/link.rs"]
mod link;

#[test]
fn link() {
  link::main();
}
