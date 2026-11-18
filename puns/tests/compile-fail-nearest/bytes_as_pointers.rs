//! No bytes are a value that holds a pointer: where a pointer aims is not a property of its
//! bytes (I1 of `nearest`: a `Near` only exists in a region).
use nearest::{Near, flat};

#[flat]
#[derive(puns::Pun)]
struct Node {
  id: u32,
  next: Option<Near<Node>>,
}

fn main() {
  let _ = puns::from_bytes::<Node>(&[0; 8]);
}
