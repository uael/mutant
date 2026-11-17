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
