//! Pointers are paired through the variants of enums, in the order of their declarations: a
//! variant that holds a pointer keeps its place, even when its tag says which one it is.
use nearest::Near;
use puns::Pun;

#[derive(Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum Old {
  Boxed(Near<u32>) = 1,
}

#[derive(Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum New {
  Plain([u8; 4]) = 0,
  Boxed(Near<u32>) = 1,
}

fn view(old: &Old) -> &New {
  puns::view_ref(old)
}

fn main() {
  // No value of these types can be made here: the function is only named, which builds it.
  let _: fn(&Old) -> &New = view;
}
