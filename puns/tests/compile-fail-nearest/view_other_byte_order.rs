//! A pointer is read as a pointer whose offset is the same integer: not one of another byte
//! order.
use nearest::{Near, Region};
use puns::num::{i32be, i32le};

fn main() {
  let region = Region::<Near<u32, i32le>>::new(Near::new(1));
  let _: &Near<u32, i32be> = puns::view_ref(&*region);
}
