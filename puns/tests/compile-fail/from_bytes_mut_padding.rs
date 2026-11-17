//! The padding byte of `(u8, u16)` would de-initialize a byte of the source when the target is
//! written.
fn main() {
  let _ = puns::from_bytes_mut::<(u8, u16)>(&mut [0; 4]);
}
