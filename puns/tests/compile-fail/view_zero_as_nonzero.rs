//! A pointer whose offset can be zero (it then aims at its own address) is not proven to be one of
//! the same scheme whose offset cannot (a `NonZero`): `try_view_ref` checks it.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::{Nz32le, One};

#[derive(puns::Pun)]
#[repr(C)]
struct Plain {
  pointer: One<u8>,
}

#[derive(puns::Pun)]
#[repr(C)]
struct NeverZero {
  pointer: One<u8, Nz32le>,
}

fn view(plain: &Plain) -> &NeverZero {
  puns::view_ref(plain)
}

fn main() {
  let _: fn(&Plain) -> &NeverZero = view;
}
