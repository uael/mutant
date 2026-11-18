//! By value, a pointer is nothing else and nothing is one: a value that holds a pointer is not
//! `Copy`, which every function that takes or makes a value requires.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let none: Opt<u32> = Opt::none();
  let _: u32 = puns::pun(none);
  let _ = puns::zeroed::<Opt<u32>>();
}
