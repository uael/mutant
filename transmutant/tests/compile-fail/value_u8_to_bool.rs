//! Not every `u8` is a `bool`.
fn main() {
  let _: bool = transmutant::transmute(2u8);
}
