//! A `Drawing` cannot be written where a `Shape` is: `Rect(3, 2)` and `Nothing` are not shapes.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

fn main() {
  let mut shape = Shape::Rect(3, true);
  let _: &mut Drawing = puns::pun_mut(&mut shape);
}
