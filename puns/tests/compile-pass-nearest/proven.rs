//! What is proven, next to what is refused in `compile-fail-nearest`: a passing case also makes
//! trybuild build instead of check, so that the errors of the constants evaluated at
//! monomorphization are reported.
use nearest::{Near, Region, flat};
use puns::{Pun, num::i32le};

#[flat]
#[derive(Pun)]
enum Old {
  Leaf(bool),
  Node(Near<Old, i32le>),
}

#[flat]
#[derive(Pun)]
enum New {
  Leaf(u8),
  Node(Near<New, i32le>),
  More(Near<[New], i32le>),
}

fn main() {
  let region = Region::new(Old::node(Old::Leaf(true)));
  let new: &New = puns::view_ref(&*region);
  assert!(matches!(new, New::Node(inner) if matches!(**inner, New::Leaf(1))));
  // A pointer is read as bytes, and a value that holds one is checked as a whole.
  let Old::Node(pointer) = &*region else { panic!() };
  assert_eq!(puns::as_bytes(pointer), [4, 0, 0, 0]);
  assert!(puns::nearest::view::<Old>(region.as_bytes().unwrap()).is_ok());
}
