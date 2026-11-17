//! Runs `examples/executables.rs`.

// As a module of a test, its items are not exported: the names of the fields of ELF headers are
// the ones of the specification.
#[expect(clippy::struct_field_names, reason = "the names of the ELF specification")]
#[path = "../examples/executables.rs"]
mod executables;

#[test]
fn executables() {
  executables::main();
}
