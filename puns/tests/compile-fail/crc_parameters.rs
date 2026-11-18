//! A CRC has its 5 parameters, which fit its width.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc::<16, 0x8005>(0, 2) == 0))]
struct TwoParameters {
  a: u16,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc::<8, 0x107, 0, false, 0>(0, 2) == 0))]
struct PolynomialTooWide {
  a: u16,
}

fn main() {}
