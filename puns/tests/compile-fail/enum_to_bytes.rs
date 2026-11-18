//! A `Shape` is not 4 bytes: the byte after the tag of a `Circle` is padding, and nothing tells
//! a `Circle` from a `Rect` at compile time.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

fn main() {
  let _: [u8; 4] = puns::pun(Shape::Rect(3, true));
}
