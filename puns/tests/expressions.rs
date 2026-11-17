//! An expression of `#[pun(..)]` means what the same Rust expression means, where every integer
//! is a `u128`: each test is a case where it did not.

use puns::{Pun, num::u16be, try_pun_ref};

/// `as` truncates, as in Rust.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((a + b) as u8 == 0))]
#[pun(assert((a * 0x101) as u16 == a * 0x101 && (a << 60) as u64 == (a % 16) << 60))]
struct Cast {
  a: u8,
  b: u8,
}

#[test]
fn casts_truncate() {
  assert!(try_pun_ref::<_, Cast>(&[200u8, 56]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[0u8, 0]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[200u8, 55]).is_err());
  assert!(try_pun_ref::<_, Cast>(&[16u8, 240]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[1u8, 254]).is_err());
}

/// A length in a field, rounded up to a multiple of 4.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + ((length + 3) & !3))]
struct Padded {
  kind: u16be,
  length: u16be,
  data: [u8],
}

#[test]
fn not_of_an_integer_is_bitwise() {
  let (padded, rest) = puns::from_head::<Padded>(&[0, 1, 0, 5, 1, 2, 3, 4, 5, 0, 0, 0, 9]).unwrap();
  assert_eq!((padded.kind.get(), padded.data.len(), rest), (1, 8, &[9][..]));
}

/// Constants are unsigned integers, of any type, read as `u128`s; `b'a'` is one.
const SMALL: u8 = 0xff;
const WIDE: u64 = 0x1_0000_0000;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(x + SMALL as u128 > 0xff && x + SMALL > 0xff || x == 0))]
#[pun(assert(x < u8::MAX && (x as u64 + WIDE) as u32 == x as u32 && x != b'!' as u128))]
#[pun(assert(1u8 + x > 0 && 0x1_0000 * x != 1))]
struct Constants {
  x: u8,
}

#[test]
fn constants_are_unsigned_integers() {
  assert!(puns::from_bytes::<Constants>(&[1]).is_ok());
  assert!(puns::from_bytes::<Constants>(&[0]).is_ok());
  assert!(puns::from_bytes::<Constants>(b"!").is_err());
  assert!(puns::from_bytes::<Constants>(&[0xff]).is_err());
}

/// `<<` drops the bits shifted out, as Rust does; a shift by 128 or more fails, where Rust would
/// panic (or, without debug assertions, shift by the amount modulo 128).
#[derive(Pun)]
#[repr(C)]
#[pun(assert((x << 127) << 1 == 0 && x << y > 0 && x >> y < 0x100))]
struct Shifts {
  x: u8,
  y: u8,
}

#[test]
fn shifts_drop_bits_and_fail_beyond_the_width() {
  assert!(puns::from_bytes::<Shifts>(&[1, 0]).is_ok());
  assert!(puns::from_bytes::<Shifts>(&[1, 127]).is_ok());
  // `2 << 127` is `0`.
  assert!(puns::from_bytes::<Shifts>(&[2, 127]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[1, 128]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[0xff, 200]).is_err());
}
