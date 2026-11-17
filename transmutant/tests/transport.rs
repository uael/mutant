//! Runs `examples/transport.rs`.

#[path = "../examples/transport.rs"]
mod transport;

#[test]
fn transport() {
  transport::main();
}
