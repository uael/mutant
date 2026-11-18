//! How many bytes an unsized target takes from the end cannot be told.
fn main() {
  let _: &[u8] = puns::tail_ref::<[u8; 4], [u8]>(&[1, 2, 3, 4]);
}
