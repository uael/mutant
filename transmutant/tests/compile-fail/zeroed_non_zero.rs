//! Zero is not a valid `NonZero`.
fn main() {
  let _ = transmutant::zeroed::<core::num::NonZero<u8>>();
}
