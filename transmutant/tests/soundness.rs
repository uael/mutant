//! Regression tests of the soundness audit: each was a way for safe code to make an invalid
//! value, or to read what it must not. The ones that must not compile are in `compile-fail/`.

use transmutant::{Data, Error};

/// A `valid(..)` restriction is a requirement, not a guarantee: an enum whose variant has a
/// restricted `u8` is not an enum whose variant has a `bool`, even if their layouts look alike.
#[test]
fn a_requirement_does_not_prove_a_guarantee_in_an_enum() {
  #[derive(Data, Clone, Copy)]
  #[repr(u8)]
  enum Soft {
    V(#[data(valid(0..=1))] u8),
  }
  #[derive(Data, Clone, Copy, Debug, PartialEq, Eq)]
  #[repr(u8)]
  enum Hard {
    V(bool),
  }
  let mut soft = Soft::V(0);
  let Soft::V(byte) = &mut soft;
  // Safe code can write anything to the field.
  *byte = 5;
  // This used to be `transmute`, without any check, which made a `bool` of 5.
  assert_eq!(transmutant::try_transmute::<Soft, Hard>(soft), Err(Error::Validity));
  assert_eq!(transmutant::try_transmute::<Soft, Hard>(Soft::V(1)), Ok(Hard::V(true)));
}
