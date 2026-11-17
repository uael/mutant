//! Making a value that may not be valid, without a check: not all bytes are UTF-8.
fn main() {
  let _: &str = puns::pun_ref(&[0u8; 4]);
}
