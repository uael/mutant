//! No `&mut` to a part of a value that holds a pointer, wherever the pointer is in it.
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

fn first(node: &mut Node) -> &mut u32 {
  puns::head_mut(node)
}

fn main() {
  let _: fn(&mut Node) -> &mut u32 = first;
}
