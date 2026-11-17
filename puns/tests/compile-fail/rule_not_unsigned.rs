//! A rule computes on unsigned integers: a float, a negative number, a signed constant or a
//! character would mean something else.
const HALF: f64 = 0.5;
const MINUS: i32 = -1;

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x * 2 >= 0.5 * 4))]
struct Float {
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > -1))]
struct Negative {
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > 1i8))]
struct Signed {
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x != 'a'))]
struct Character {
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > HALF))]
struct FloatConstant {
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > MINUS))]
struct SignedConstant {
  x: u8,
}

fn main() {}
