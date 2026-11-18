//! What a pointer aims at is read as something that is not larger.
use nearest::{Near, Region};

fn main() {
  let region = Region::<Near<u16>>::new(Near::new(1));
  let _: &Near<[u8; 4]> = puns::view_ref(&*region);
}
