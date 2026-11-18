//! A check that can never succeed: a byte that is always 2 is never a `bool`.
fn main() {
  let _ = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
}
