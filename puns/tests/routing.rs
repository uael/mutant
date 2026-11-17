//! Runs `examples/routing.rs`.

#[path = "../examples/routing.rs"]
mod routing;

#[test]
fn routing() {
  routing::main();
}
