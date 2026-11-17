//! Reading padding: the byte after the `u8` of `(u8, u16)` is padding.
fn main() {
  let _: &[u8; 4] = puns::pun_ref(&(0u8, 0u16));
}
