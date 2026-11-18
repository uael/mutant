//! A rule that reads the whole value reads its trailing padding: the last byte of `Plain`.
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Plain {
  a: u16,
  b: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(crc32(0, size_of_val(self)) != 0))]
struct Summed {
  a: u16,
  b: u8,
}

fn main() {
  let _ = puns::try_pun_ref::<Plain, Summed>(&Plain { a: 0, b: 0 });
}
