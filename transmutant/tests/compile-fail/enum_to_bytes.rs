//! A `Shape` is not 4 bytes: the byte after the tag of a `Circle` is padding, and nothing tells
//! a `Circle` from a `Rect` at compile time.
use transmutant::Data;

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

fn main() {
  let _: [u8; 4] = transmutant::transmute(Shape::Rect(3, true));
}
