// Through a `&mut MaybeUninit<u32>`, uninitialized bytes could be written over a `u32`.
fn main() {
  let mut value = 0u32;
  let _: &mut core::mem::MaybeUninit<u32> = puns::pun_mut(&mut value);
}
