//! Pointers are paired through the fields of structs, in the order of their declarations: a
//! struct that only holds a pointer is not that pointer.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
struct Wrapped {
  pointer: Near<u32>,
}

fn main() {
  let region = Region::<Near<u32>>::new(Near::new(1));
  let _: &Wrapped = puns::view_ref(&*region);
}
