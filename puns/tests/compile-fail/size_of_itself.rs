//! A size is read from the value, but not from its size, which it defines.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(size = size_of_val(self) - 1)]
struct Circular {
  length: u8,
  data: [u8],
}

fn main() {}
