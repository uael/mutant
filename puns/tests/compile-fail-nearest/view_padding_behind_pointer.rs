//! Behind a pointer as elsewhere, padding is not read.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
struct Padded {
  byte: u8,
  word: u32,
}

fn main() {
  let region = Region::<Near<Padded>>::new(Near::new(Padded { byte: 1, word: 2 }));
  let _: &Near<[u8; 8]> = puns::view_ref(&*region);
}
