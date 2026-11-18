//! No bytes are a value that holds a pointer: where a pointer aims is not a property of its
//! bytes.
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

fn main() {
  let _ = puns::from_bytes::<Node>(&[0; 8]);
}
