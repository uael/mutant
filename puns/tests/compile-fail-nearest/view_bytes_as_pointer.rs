//! `view_ref` needs a pointer for a pointer: bytes are not one.
use nearest::Near;

fn main() {
  let _: &Near<u32> = puns::view_ref(&[4u8, 0, 0, 0]);
}
