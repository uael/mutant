//! The padding byte of `(u8, u16)` would de-initialize a byte when the target is written.
fn main() {
  let _ = puns::from_head_mut::<(u8, u16)>(&mut [0; 4]);
}
