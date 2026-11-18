//! By value, a pointer is nothing else and nothing is one: a value that holds a pointer is not
//! `Copy`, which every function that takes or makes a value requires.
use nearest::Near;

fn main() {
  let none: Option<Near<u32>> = None;
  let _: u32 = puns::pun(none);
  let _ = puns::zeroed::<Option<Near<u32>>>();
}
