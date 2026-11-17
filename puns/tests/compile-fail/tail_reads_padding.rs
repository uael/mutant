//! The last byte of a `repr(C)` struct of a `u16` and a `u8` is padding.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Padded {
  word: u16,
  byte: u8,
}

fn main() {
  let _: u8 = puns::tail(Padded { word: 0, byte: 0 });
}
