//! Nor to its last bytes.
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

fn last(node: &mut Node) -> &mut [u8; 4] {
  puns::tail_mut(node)
}

fn main() {
  let _: fn(&mut Node) -> &mut [u8; 4] = last;
}
