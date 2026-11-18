//! A pointer is not read as a pointer by `pun_ref`, even one of the same type: only `view_ref`
//! proves what a pointer aims at.
use nearest::{Near, Region};

fn main() {
  let region = Region::<Near<u32>>::new(Near::new(1));
  let _: &Near<[u8; 4]> = puns::pun_ref(&*region);
}
