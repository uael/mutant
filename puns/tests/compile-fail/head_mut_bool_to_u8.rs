//! A `u8` written through the `&mut` can be 2, which is not a `bool`.
fn main() {
  let _: &mut u8 = puns::head_mut(&mut [true, false]);
}
