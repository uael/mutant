//! No `&mut` to the bytes of a pointer (I2 of `nearest`): a `None` on the stack would become a
//! `Near` that aims at nothing.
use nearest::Near;

fn main() {
  let mut none: Option<Near<u32>> = None;
  let bytes: &mut [u8; 4] = puns::pun_mut(&mut none);
  bytes[0] = 4;
}
