//! The last two bytes of a `[u8; 4]` are not always where a `u16` can be.
fn main() {
  let _: &u16 = puns::tail_ref(&[0u8; 4]);
}
