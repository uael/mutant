//! A rule reads integers: a pointer is not one.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(next != 0))]
struct Node {
  id: u32,
  next: Opt<Node>,
}

fn view(node: &Node) -> &Node {
  puns::view_ref(node)
}

fn main() {
  let _: fn(&Node) -> &Node = view;
}
