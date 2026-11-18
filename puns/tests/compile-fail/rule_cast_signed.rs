//! A rule computes on unsigned integers: `as i8` would mean a negative number.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert((a as i8) < 0))]
struct Signed {
  a: u8,
}

fn main() {}
