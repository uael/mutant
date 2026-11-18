//! Uninitialized bytes: `MaybeUninit<T>`, and scalars that a target requires while a source knows
//! nothing of them (`Spec::maybe_uninit`).

use core::mem::MaybeUninit;

use puns::{
  Error, Scalar,
  layout::{Endian, Spec},
};

/// A byte that a target requires to be 0, and that a source may hold uninitialized: what a byte
/// of a tag that may be padding is.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Zero(MaybeUninit<u8>);

// SAFETY: 0 is a value of a `MaybeUninit<u8>`, and a `MaybeUninit<u8>` is known to be anything.
unsafe impl Scalar for Zero {
  const SPEC: Spec = Spec::ranges::<u8>(Endian::Little, &[(0, 0)]).maybe_uninit();
}

#[test]
fn maybe_uninit_takes_anything() {
  // Any value, padding included, is a `MaybeUninit` of its size: by value, by reference.
  let value: MaybeUninit<u32> = puns::pun(7u32);
  // SAFETY: made of a `u32`.
  assert_eq!(unsafe { value.assume_init() }, 7);
  let padded = (1u8, 2u16);
  let erased: &MaybeUninit<(u8, u16)> = puns::pun_ref(&padded);
  // SAFETY: the same value.
  assert_eq!(unsafe { erased.assume_init_ref() }, &(1, 2));
  let _: &MaybeUninit<[u8; 4]> = puns::pun_ref(&padded);
  // Between two `MaybeUninit`s, by `&mut` too: anything written through one is the other.
  let mut words = MaybeUninit::<u32>::new(0x0102_0304);
  let bytes: &mut MaybeUninit<[u8; 4]> = puns::pun_mut(&mut words);
  bytes.write([0; 4]);
  // SAFETY: written.
  assert_eq!(unsafe { words.assume_init() }, 0);
  // SAFETY: zeroed.
  assert_eq!(unsafe { puns::zeroed::<MaybeUninit<u16>>().assume_init() }, 0);
}

#[test]
fn maybe_uninit_proves_nothing() {
  // Nothing proven of its bytes, a source of only what accepts uninitialized bytes (see the
  // compile-fail tests `maybe_uninit_*`).
  let _: MaybeUninit<u8> = puns::pun(MaybeUninit::<u8>::uninit());
  let _: [MaybeUninit<u8>; 2] = puns::pun(MaybeUninit::<u16>::uninit());
}

#[test]
fn maybe_uninit_scalar() {
  // As a target: required to be 0, checked from what is not known to be.
  assert_eq!(puns::try_pun::<u8, Zero>(0).map(|_| ()), Ok(()));
  assert_eq!(puns::try_pun::<u8, Zero>(1).map(|_| ()), Err(Error::Validity));
  assert!(puns::from_bytes::<Zero>(&[0]).is_ok());
  assert!(puns::from_bytes::<Zero>(&[3]).is_err());
  // Proven by what is known to be 0.
  let _: Zero = puns::pun(puns::num::u8c::<0>::new());
  // As a source: nothing that needs the byte, not even the same type (two types can have the
  // same `Spec` of this kind while one may be padding where the other may not).
  let _: MaybeUninit<u8> = puns::pun(Zero(MaybeUninit::uninit()));
}
