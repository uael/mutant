//! An enum of references to what bytes may be is made by `from_bytes`: it cannot refer to a value
//! that holds a pointer.
use nearest::{Near, flat};

#[flat]
#[derive(puns::Pun)]
struct Node {
  id: u32,
  next: Option<Near<Node>>,
}

#[derive(puns::Pun)]
enum Any<'a> {
  Node(&'a Node),
  Bytes(&'a [u8]),
}

fn main() {
  let _ = Any::from_bytes(&[0; 8]);
}
