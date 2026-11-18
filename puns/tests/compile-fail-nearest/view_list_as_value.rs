//! A list is read as a list, not as a value.
use nearest::{Near, Region};

fn main() {
  let region = Region::<Near<[u32]>>::new(Near::from_iter([1u32]));
  let _: &Near<u32> = puns::view_ref(&*region);
}
