//! A check that can never succeed: a byte that is always 2 is never a `bool`.
fn main() {
  let _ = transmutant::try_transmute::<transmutant::num::u8c<2>, bool>(Default::default());
}
