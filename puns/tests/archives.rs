//! Runs `examples/archives.rs`.
//!
//! Not built under Miri: an example is a declaration, and the unsafe code it reaches (walks,
//! regions, checksums, getters, splits) is that of the library, which its own tests and `ethernet`
//! run there.

#![cfg(not(miri))]

#[path = "../examples/archives.rs"]
mod archives;

#[test]
fn archives() {
  archives::main();
}
