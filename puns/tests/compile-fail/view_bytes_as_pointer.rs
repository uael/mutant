//! `view_ref` needs a pointer for a pointer: bytes are not one.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

fn main() {
  let _: &One<u32> = puns::view_ref(&[4u8, 0, 0, 0]);
}
