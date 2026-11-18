//! Nor to its last bytes.
use nearest::{Near, flat};

#[flat]
#[derive(puns::Pun)]
struct Node {
  id: u32,
  next: Option<Near<Node>>,
}

fn main() {
  let mut node = Node { id: 1, next: None };
  let _: &mut [u8; 4] = puns::tail_mut(&mut node);
}
