//! Zero is not a valid `NonZero`.
fn main() {
  let _ = puns::zeroed::<core::num::NonZero<u8>>();
}
