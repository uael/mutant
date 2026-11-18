//! No `&mut` to the bytes of a pointer: a null one would become one that aims at anything.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let mut none: Opt<u32> = Opt::none();
  let bytes: &mut [u8; 4] = puns::pun_mut(&mut none);
  bytes[0] = 4;
}
