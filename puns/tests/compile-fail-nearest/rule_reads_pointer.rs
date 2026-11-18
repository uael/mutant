//! A rule reads integers: a pointer is not one.
use nearest::{Near, flat};

#[flat]
#[derive(puns::Pun)]
#[pun(assert(next != 0))]
struct Node {
  id: u32,
  next: Option<Near<Node>>,
}

fn main() {
  let _ = puns::nearest::view::<Node>(&[]);
}
