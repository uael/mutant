//! Runs `examples/media.rs`.
//!
//! Not built under Miri: an example is a declaration, and the unsafe code it reaches (walks,
//! regions, checksums, getters, splits) is that of the library, which its own tests and `ethernet`
//! run there.

#![cfg(not(miri))]

#[path = "../examples/media.rs"]
mod media;

#[test]
fn media() {
  media::main();
}
