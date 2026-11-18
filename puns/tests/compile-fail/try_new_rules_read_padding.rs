//! The functions of rules that read the bytes of the value (variable-length integers, CRCs,
//! checksums, patterns) read its padding: where such a rule may be checked, the padding of its
//! struct must be initialized in the source.
use puns::{Pun, layout::Dfa};

#[derive(Pun)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(leb128(0) < 0x80))]
struct Varint {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(crc::<8, 0x07, 0, false, 0>(0, 4) == 0 || adler32(0, 4) == 0))]
struct Checksums {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(Dfa::ASCII, 0, 4)))]
struct Pattern {
  a: u8,
  b: u16,
}

fn main() {
  let plain = Plain { a: 0, b: 0 };
  let _ = puns::try_pun_ref::<Plain, Varint>(&plain);
  let _ = puns::try_pun_ref::<Plain, Checksums>(&plain);
  let _ = puns::try_pun_ref::<Plain, Pattern>(&plain);
}
