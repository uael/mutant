//! A `&mut` through which an invalid value could be written back: a `u8` that is not a `bool`.
fn main() {
  let _: &mut u8 = transmutant::transmute_mut(&mut true);
}
