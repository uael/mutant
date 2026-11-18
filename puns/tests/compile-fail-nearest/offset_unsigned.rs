//! An offset is a signed integer: after a move, a pointer may aim backward.
use nearest::Near;
use puns::num::u32le;

fn main() {
  let _ = size_of::<Near<u8, u32le>>();
}
