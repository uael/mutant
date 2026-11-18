//! Where the last bytes of a slice start depends on its length: `from_tail` does it at run time.
fn main() {
  let bytes: &[u8] = &[1, 2, 3];
  let _: &u8 = puns::tail_ref::<[u8], u8>(bytes);
}
