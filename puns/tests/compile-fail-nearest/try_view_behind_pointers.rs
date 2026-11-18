//! `try_view_ref` checks the value, not what it points to: behind a pointer, a `u8` is not read
//! as a `bool`.
use nearest::{Near, Region, flat};

#[flat]
#[derive(puns::Pun)]
struct Bytes {
  first: u8,
  rest: Near<[u8]>,
}

#[flat]
#[derive(puns::Pun)]
struct Flags {
  first: bool,
  rest: Near<[bool]>,
}

fn main() {
  let region = Region::new(Bytes::new(1, [0u8, 1]));
  let _ = puns::try_view_ref::<Bytes, Flags>(&region);
}
