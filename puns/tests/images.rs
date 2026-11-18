//! Runs `examples/images.rs`.
//!
//! Not built under Miri: an example is a declaration, and the unsafe code it reaches (walks,
//! regions, checksums, getters, splits) is that of the library, which its own tests and `ethernet`
//! run there.

#![cfg(not(miri))]

#[path = "../examples/images.rs"]
mod images;

#[test]
fn images() {
  images::main();
}
