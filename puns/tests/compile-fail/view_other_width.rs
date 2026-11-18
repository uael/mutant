//! Nor one of another width, in a struct of the same size.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use puns::num::{i16le, i32le};
use relative::One;

#[derive(puns::Pun)]
#[repr(C)]
struct Wide {
  pointer: One<u32, i32le>,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Narrow {
  pointer: One<u32, i16le>,
  more: [u8; 2],
}

fn view(source: &Wide) -> &Narrow {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&Wide) -> &Narrow = view;
}
