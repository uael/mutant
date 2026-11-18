//! As in `nearest`, a pointer does not lead to a narrower one: the pointers of the root bound the
//! region.
use nearest::{Near, flat};
use puns::{
  Pun,
  num::{i16le, i64le},
};

#[flat]
#[derive(Pun)]
struct Narrow {
  text: Near<str, i16le>,
}

#[flat]
#[derive(Pun)]
struct Wide {
  narrow: Near<Narrow, i64le>,
}

fn main() {
  let _ = puns::nearest::view::<Wide>(&[]);
}
