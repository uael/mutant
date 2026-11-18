//! A pointer that may be null is not one that is not, without a check.
use nearest::Near;

fn main() {
  let none: Option<Near<u32>> = None;
  let _: &Near<u32> = puns::view_ref(&none);
}
