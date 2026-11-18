//! A pointer that may be null is not one that is not, without a check.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::{One, Opt};

fn main() {
  let none: Opt<u32> = Opt::none();
  let _: &One<u32> = puns::view_ref(&none);
}
