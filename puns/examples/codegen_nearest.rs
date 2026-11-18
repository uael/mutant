//! Functions of the feature `nearest` whose machine code is worth looking at:
//!
//! ```text
//! cargo rustc -p puns --release --features nearest --example codegen_nearest -- --emit asm -C llvm-args=-x86-asm-syntax=intel
//! ```
//!
//! and look for their names in `target/release/**/codegen_nearest*.s`. See `DESIGN.md` for what
//! to expect: a view is the address it is given, and the check of a region is what a checker
//! written by hand for the type does (the one of `nearest` is next to it, for the same type).
#![expect(
  clippy::no_mangle_with_rust_abi,
  reason = "the code of the Rust ABI is what is measured; `no_mangle` only names it in the assembly"
)]

use nearest::{Near, flat};
use puns::{
  Pun,
  num::{i32le, u16le, u32le},
};

/// An expression, and the next version of it: a `bool` read as a `u8`, one more variant.
#[flat]
#[derive(Pun)]
pub enum Old {
  Leaf(bool),
  Neg(Near<Old, i32le>),
  Call(Near<str, i32le>, Near<[Old], i32le>),
}

#[flat]
#[derive(Pun)]
pub enum New {
  Leaf(u8),
  Neg(Near<New, i32le>),
  Call(Near<str, i32le>, Near<[New], i32le>),
  Pair(Near<New, i32le>, Near<New, i32le>),
}

/// A pointer that may be null, read as one that is not.
#[flat]
#[derive(Pun)]
pub struct Maybe {
  pub id: u32le,
  pub owner: Option<Near<Old, i32le>>,
}

#[flat]
#[derive(Pun)]
pub struct Owned {
  pub id: u32le,
  pub owner: Near<New, i32le>,
}

/// A table: a name, rows that hold a pointer and a `bool`, and numbers.
#[flat]
#[derive(Pun)]
pub struct Table {
  pub version: u32le,
  pub name: Near<str, i32le>,
  pub rows: Near<[Row], i32le>,
  pub numbers: Near<[u16le], i32le>,
}

#[flat]
#[derive(Pun)]
pub struct Row {
  pub public: bool,
  pub label: Near<str, i32le>,
}

/// Numbers, and only numbers: nothing in them can be invalid.
#[flat]
#[derive(Pun)]
pub struct Numbers {
  pub count: u32le,
  pub values: Near<[u32le], i32le>,
}

// Proven, pointers followed through a recursive type: nothing is compiled.
#[unsafe(no_mangle)]
pub fn view_new(old: &Old) -> &New {
  puns::view_ref(old)
}

#[unsafe(no_mangle)]
pub fn view_slice(old: &[Old]) -> &[New] {
  puns::view_ref(old)
}

// What is not proven of the value itself: here that a pointer is not null.
#[unsafe(no_mangle)]
pub fn try_view_owned(maybe: &Maybe) -> Option<&Owned> {
  puns::try_view_ref(maybe).ok()
}

// The check of a region, with the memory of the caller: this crate's, from the layout...
#[unsafe(no_mangle)]
pub fn load_table<'b>(
  bytes: &'b [u8],
  scratch: &mut [puns::nearest::Pending],
) -> Option<&'b Table> {
  puns::nearest::view_with(bytes, scratch).ok()
}

// ...and the one of `nearest`, which `#[flat]` writes for the type.
#[unsafe(no_mangle)]
pub fn load_table_nearest<'b>(
  bytes: &'b [u8],
  scratch: &mut [nearest::Pending],
) -> Option<&'b Table> {
  nearest::view_with(bytes, scratch).ok()
}

// A list of numbers is not walked: its pointer, its length, and that is all.
#[unsafe(no_mangle)]
pub fn load_numbers<'b>(
  bytes: &'b [u8],
  scratch: &mut [puns::nearest::Pending],
) -> Option<&'b Numbers> {
  puns::nearest::view_with(bytes, scratch).ok()
}

#[unsafe(no_mangle)]
pub fn load_numbers_nearest<'b>(
  bytes: &'b [u8],
  scratch: &mut [nearest::Pending],
) -> Option<&'b Numbers> {
  nearest::view_with(bytes, scratch).ok()
}

fn main() {}
