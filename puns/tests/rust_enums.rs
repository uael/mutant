//! Enums whose layout `rustc` chose (no `repr`), found by experiments in const evaluation, and
//! types without values.

use core::convert::Infallible;

use puns::{Error, Pun};

/// A variant that holds a type without values is one that no value is.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
#[expect(dead_code, reason = "no value is `B`, which is the point")]
enum Never {
  A(u8),
  B(Infallible),
}

#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Unit {
  A(u8),
  B(()),
}

/// An enum without variants: no value.
#[derive(Pun, Clone, Copy)]
#[expect(clippy::empty_enums, reason = "a type without values, which is the point")]
enum Void {}

#[test]
fn uninhabited() {
  assert_eq!(puns::try_pun::<[u8; 2], Never>([0, 5]), Ok(Never::A(5)));
  // The tag of `B`, whose field has no value.
  assert_eq!(puns::try_pun::<[u8; 2], Never>([1, 5]), Err(Error::Validity));
  // Variant by variant: `A` is proven, `B` is never a `Never`.
  assert_eq!(puns::try_pun::<Unit, Never>(Unit::A(3)), Ok(Never::A(3)));
  assert_eq!(puns::try_pun::<Unit, Never>(Unit::B(())), Err(Error::Validity));
  // A `Never` is always a `Unit`: its `B` is never there.
  assert_eq!(puns::pun::<Never, Unit>(Never::A(3)), Unit::A(3));
  // A source without values proves anything, as nothing is ever made of one.
  let _ = puns::pun::<(u8, Void), (u8, Void)>;
}
