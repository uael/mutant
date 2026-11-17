//! Runs `examples/media.rs`.

#[path = "../examples/media.rs"]
mod media;

#[test]
fn media() {
  media::main();
}
