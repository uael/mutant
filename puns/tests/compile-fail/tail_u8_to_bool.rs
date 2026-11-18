//! Not every last byte of a `[u8; 2]` is a `bool`: a `try_*` function would check it.
fn main() {
  let _: bool = puns::tail::<[u8; 2], bool>([1, 7]);
}
