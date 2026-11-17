//! What a rule cannot name is said to be so, not "not found".
use puns::num::u16be;

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc16(0, 4) == check))]
struct UnknownFunction {
  check: u16be,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(magic[0] == 0x7f))]
struct Element {
  magic: [u8; 4],
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Header {
  length: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(size = header.length)]
struct FieldOfField {
  header: Header,
  data: [u8],
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a.min(b) == 1))]
struct Method {
  a: u8,
  b: u8,
}

fn main() {}
