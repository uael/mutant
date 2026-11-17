//! An expression of `#[pun(..)]` means what the same Rust expression means, where every integer
//! is a `u128`: each test is a case where it did not.

use puns::{Pun, try_pun_ref};

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
