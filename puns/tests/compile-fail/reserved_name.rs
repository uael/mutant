//! A type of the input of `#[derive(Pun)]` cannot be named like the items that it generates,
//! which would stand for it in the generated code: here the tag of the enum `Holder`, which is
//! not the `__PunTag` that its variant holds.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum __PunTag {
  A,
  B,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Holder {
  X(__PunTag),
}

fn main() {}
