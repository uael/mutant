// A type without values is made of nothing.
fn main() {
  let _ = puns::try_pun::<(), core::convert::Infallible>(());
}
