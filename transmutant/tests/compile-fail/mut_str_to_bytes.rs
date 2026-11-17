//! A `&mut` through which bytes that are not UTF-8 could be written to a `str`.
fn main() {
  let mut text = String::from("text");
  let _: &mut [u8] = transmutant::transmute_mut(text.as_mut_str());
}
