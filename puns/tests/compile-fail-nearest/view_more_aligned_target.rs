//! Nor more aligned.
use nearest::{Near, Region};

fn main() {
  let region = Region::<Near<[u8; 4]>>::new(Near::new([1, 2, 3, 4]));
  let _: &Near<u32> = puns::view_ref(&*region);
}
