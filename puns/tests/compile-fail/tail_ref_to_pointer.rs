//! Nor are the last ones.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

#[derive(puns::Pun)]
#[repr(C)]
struct Pair {
  first: One<u32>,
  second: One<u32>,
}

fn second(pair: &Pair) -> &One<u32> {
  puns::tail_ref(pair)
}

fn main() {
  let _: fn(&Pair) -> &One<u32> = second;
}
