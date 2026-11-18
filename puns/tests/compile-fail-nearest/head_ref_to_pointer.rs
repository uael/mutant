//! The first bytes of a value are not a pointer either.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
struct Pair {
  first: Near<u32>,
  second: Near<u32>,
}

fn main() {
  let region = Region::new(Pair::new(1, 2));
  let _: &Near<u32> = puns::head_ref(&*region);
}
