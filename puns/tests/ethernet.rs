//! Runs `examples/ethernet.rs`.

#[path = "../examples/ethernet.rs"]
mod ethernet;

#[test]
fn ethernet() {
  ethernet::main();
}
