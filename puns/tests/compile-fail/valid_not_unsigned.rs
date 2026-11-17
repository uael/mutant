//! The bounds of `valid` are unsigned integers, as the values of rules are.
const LOW: f32 = 1.5;
const MINUS: i16 = -1;

#[derive(puns::Pun)]
#[repr(C)]
struct Float {
  #[pun(valid(LOW..=7))]
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Signed {
  #[pun(valid(0..MINUS))]
  x: u16,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Literal {
  #[pun(valid(2.5, 3))]
  x: u8,
}

fn main() {}
