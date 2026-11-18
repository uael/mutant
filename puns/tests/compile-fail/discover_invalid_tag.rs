// What the discovery of the layout of an enum without `repr` rests on (`puns/src/discover.rs`, R2):
// const evaluation refuses to read the variant of a value whose tag is not one, here a `bool` of
// 2 where `rustc` put the niche of `None`. If this ever compiles, an experiment that goes wrong
// could give an answer instead of failing.
use core::mem::{self, MaybeUninit};

const fn variant_of_two() -> mem::Discriminant<Option<bool>> {
  let mut value = MaybeUninit::new(Some(true));
  // SAFETY: the byte of the `bool`, written with a value that is no tag of `Option<bool>` (0 and 1
  // are `Some`, 2 is `None`).
  unsafe {
    value.as_mut_ptr().cast::<u8>().write(3);
    mem::discriminant(&*value.as_ptr())
  }
}

const VARIANT: mem::Discriminant<Option<bool>> = variant_of_two();

fn main() {
  let _ = VARIANT;
}
