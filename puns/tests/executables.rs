//! Runs `examples/executables.rs`.
//!
//! Not built under Miri: an example is a declaration, and the unsafe code it reaches (walks,
//! regions, checksums, getters, splits) is that of the library, which its own tests and `ethernet`
//! run there.

#![cfg(not(miri))]

// As a module of a test, its items are not exported: the names of the fields of ELF headers are
// the ones of the specification.
#[expect(clippy::struct_field_names, reason = "the names of the ELF specification")]
#[path = "../examples/executables.rs"]
mod executables;

#[test]
fn executables() {
  executables::main();
}
