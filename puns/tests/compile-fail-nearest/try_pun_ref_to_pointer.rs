//! Nor are they one after a check: no check of a value tells what its pointers aim at.
use nearest::Near;

fn main() {
  let _ = puns::try_pun_ref::<[u32; 2], (u32, Option<Near<u32>>)>(&[1, 0]);
}
