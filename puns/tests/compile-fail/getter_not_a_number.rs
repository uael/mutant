//! A getter reads unsigned integer fields as `u128`s: the bits of a signed number are not its
//! value as one.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(get(value: i8 = x as i8))]
struct Getter {
  x: i8,
}

fn main() {
  let _ = puns::from_bytes::<Getter>(&[0]).map(|getter| getter.value());
}
