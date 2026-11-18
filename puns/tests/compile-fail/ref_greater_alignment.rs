//! Sizes or alignments that do not match: a `u32` is more aligned than bytes.
fn main() {
  let _: &u32 = puns::pun_ref(&[0u8; 4]);
}
