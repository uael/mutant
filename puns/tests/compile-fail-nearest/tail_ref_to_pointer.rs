//! Nor are the last ones.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
struct Pair {
  first: Near<u32>,
  second: Near<u32>,
}

fn main() {
  let region = Region::new(Pair::new(1, 2));
  let _: &Near<u32> = puns::tail_ref(&*region);
}
