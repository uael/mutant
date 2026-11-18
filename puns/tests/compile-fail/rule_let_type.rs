//! The values of a rule are `u128`s and `bool`s: `let x: u8` would not be what Rust makes of it.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ let sum: u8 = a + b; sum == 0 }))]
struct Annotated {
  a: u8,
  b: u8,
}

fn main() {}
