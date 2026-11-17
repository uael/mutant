//! Runs `examples/images.rs`.

#[path = "../examples/images.rs"]
mod images;

#[test]
fn images() {
  images::main();
}
