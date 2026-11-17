//! Runs `examples/storage.rs`.

#[path = "../examples/storage.rs"]
mod storage;

#[test]
fn storage() {
  storage::main();
}
