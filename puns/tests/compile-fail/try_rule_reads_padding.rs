//! A rule that reads the bytes of its value (`le`, `be`, `crc32`, `sum16`) reads its padding:
//! where it may be checked, the padding of its struct must be initialized in the source, which
//! the byte after the `u8` of `Plain` is not.
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

fn main() {
  let _ = puns::try_pun_ref::<Plain, ReadsPadding>(&Plain { a: 0, b: 0 });
}
