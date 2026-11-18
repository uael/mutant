//! Nor are they one after a check: no check of a value tells what its pointers aim at.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let _ = puns::try_pun_ref::<[u32; 2], (u32, Opt<u32>)>(&[1, 0]);
}
