//! Behind a pointer as elsewhere, padding is not read.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

#[derive(puns::Pun)]
#[repr(C)]
struct Padded {
  byte: u8,
  word: u32,
}

fn view(source: &One<Padded>) -> &One<[u8; 8]> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<Padded>) -> &One<[u8; 8]> = view;
}
