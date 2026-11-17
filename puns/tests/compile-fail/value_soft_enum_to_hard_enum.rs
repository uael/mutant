//! A `valid(..)` restriction is a requirement, not a guarantee: safe code can write 5 to the `u8`
//! of a `Soft`, which is not a `bool`. Two layouts are the same only if they require the same and
//! guarantee the same.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Soft {
  V(#[pun(valid(0..=1))] u8),
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Hard {
  V(bool),
}

fn main() {
  let _: Hard = puns::pun(Soft::V(1));
}
