//! A field of a type that is not a `Pun` type: the message says what a `Pun` type is.
#[derive(puns::Pun)]
#[repr(C)]
struct Named {
  id: u8,
  name: String,
}

fn main() {}
