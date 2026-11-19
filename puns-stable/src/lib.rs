//! What `puns` does without its `unstable` feature: the same, whatever layout `rustc` chooses
//! where Rust lets it choose (section 12 of `puns/DESIGN.md`).
//!
//! `tests/seeds.rs` records the verdict of every function of `puns` on pairs of types, and which
//! bytes a checked pun accepts, and compares the record with `tests/seeds.txt`: CI runs it with
//! `-Zrandomize-layout` and several seeds, and without, and it must be the same each time.
//! `tests/refused.rs` holds what does not compile without the feature, and the message.
//!
//! Built alone (`cargo test -p puns-stable`): built with the rest of the workspace, `puns` has
//! the `unstable` feature (`puns-model` turns it on), and `tests/seeds.rs` checks nothing.
