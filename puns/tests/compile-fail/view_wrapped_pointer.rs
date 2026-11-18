//! Pointers are paired through the fields of structs, in the order of their declarations: a
//! struct that only holds a pointer is not that pointer.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

#[derive(puns::Pun)]
#[repr(C)]
struct Wrapped {
  pointer: One<u32>,
}

fn view(source: &One<u32>) -> &Wrapped {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<u32>) -> &Wrapped = view;
}
