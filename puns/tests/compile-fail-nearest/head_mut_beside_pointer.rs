//! Nor to a part of a value that holds a pointer, wherever the pointer is in it.
use nearest::{Near, flat};

#[flat]
#[derive(puns::Pun)]
struct Node {
  id: u32,
  next: Option<Near<Node>>,
}

fn main() {
  let mut node = Node { id: 1, next: None };
  let _: &mut u32 = puns::head_mut(&mut node);
}
