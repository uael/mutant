//! Runs `examples/archives.rs`.

#[path = "../examples/archives.rs"]
mod archives;

#[test]
fn archives() {
  archives::main();
}
