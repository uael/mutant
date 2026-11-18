//! Where a slice that ends the bytes starts cannot be told from the end.
fn main() {
  let _ = puns::from_tail::<[u16]>(&[0; 4]);
}
