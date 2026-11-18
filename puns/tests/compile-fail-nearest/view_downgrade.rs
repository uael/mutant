//! A value is not read as a type that has fewer values: here a variant less.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
enum New {
  Leaf(u32),
  Node(Near<New>),
  More(Near<[New]>),
}

#[flat]
#[derive(puns::Pun)]
enum Old {
  Leaf(u32),
  Node(Near<Old>),
}

fn main() {
  let region = Region::new(New::Leaf(1));
  let _: &Old = puns::view_ref(&*region);
}
