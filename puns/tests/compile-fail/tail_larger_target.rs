//! A target larger than the source.
fn main() {
  let _: [u8; 3] = puns::tail::<[u8; 2], [u8; 3]>([1, 2]);
}
