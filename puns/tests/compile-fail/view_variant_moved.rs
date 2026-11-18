//! Pointers are paired through the variants of enums, in the order of their declarations: a
//! variant that holds a pointer keeps its place, even when its tag says which one it is.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

#[derive(puns::Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum Old {
  Boxed(One<u32>) = 1,
}

#[derive(puns::Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum New {
  Plain([u8; 4]) = 0,
  Boxed(One<u32>) = 1,
}

fn view(source: &Old) -> &New {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&Old) -> &New = view;
}
