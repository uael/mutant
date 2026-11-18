//! A rule reads unsigned integer fields: the bits of a signed or floating-point number, of a
//! `bool` or of an enum are not their value as an unsigned integer.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x + 1 > 0))]
struct Signed {
  x: i8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > 0))]
struct Float {
  x: f32,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x == 1))]
struct Bool {
  x: bool,
}

fn main() {}
