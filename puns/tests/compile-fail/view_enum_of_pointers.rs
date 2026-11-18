//! An enum of references to what bytes may be is made by `from_bytes`: it cannot refer to a value
//! that holds a pointer.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

#[derive(puns::Pun)]
#[repr(C)]
struct Node {
  id: u32,
  next: Opt<Node>,
}

#[derive(puns::Pun)]
enum Any<'a> {
  Node(&'a Node),
  Bytes(&'a [u8]),
}

fn main() {
  let _ = Any::from_bytes(&[0; 8]);
}
