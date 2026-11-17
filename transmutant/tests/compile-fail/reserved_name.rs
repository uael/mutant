//! A type of the input of `#[derive(Data)]` cannot be named like the items that it generates,
//! which would stand for it in the generated code: here the tag of the enum `Holder`, which is
//! not the `__DataTag` that its variant holds.
use transmutant::Data;

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum __DataTag {
  A,
  B,
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum Holder {
  X(__DataTag),
}

fn main() {}
