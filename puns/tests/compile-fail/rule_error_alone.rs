//! When the derive fails, its error is the only one: not one at each use of the type.
use puns::{Pun, Tag};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(flags & 1))]
#[pun(tag(u8 = puns::num::u8c<1>))]
struct Flags {
  flags: u8,
  data: [u8],
}

#[derive(Pun)]
#[repr(C)]
struct Carried<P: ?Sized + Tag<u8>> {
  tag: P::Tag,
  payload: P,
}

fn main() {
  let _ = puns::from_bytes::<Flags>(&[1, 2]);
  let _: &Flags = puns::try_pun_ref(&[1u8, 2][..]).unwrap();
  let _ = puns::from_bytes::<Carried<Flags>>(&[1, 1, 2]);
}
