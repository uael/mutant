//! Runs `examples/routing.rs`.
//!
//! Not built under Miri: an example is a declaration, and the unsafe code it reaches (walks,
//! regions, checksums, getters, splits) is that of the library, which its own tests and `ethernet`
//! run there.

#![cfg(not(miri))]

#[path = "../examples/routing.rs"]
mod routing;

#[test]
fn routing() {
  routing::main();
}
