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

/// A rule that reads the bytes of the value (`le`, `be`, `crc32`, `sum16`) reads its padding too:
/// it can only be checked if the source has initialized bytes there.
#[derive(Data, Debug, Clone, Copy)]
#[repr(C)]
#[data(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

#[derive(Data, Debug, Clone, Copy)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Data, Debug, Clone, Copy)]
#[repr(u8)]
enum HoldsReadsPadding {
  A(ReadsPadding),
}

#[test]
fn a_rule_reads_padding_only_if_it_is_initialized() {
  // From bytes, the rule is checked, padding included.
  let ok = [u16::from_ne_bytes([7, 0]), 9];
  let ok: &[u8; 4] = transmutant::transmute_ref(&ok);
  assert_eq!(
    transmutant::try_transmute_ref::<[u8; 4], ReadsPadding>(ok).map(|r| (r.a, r.b)),
    Ok((7, 9))
  );
  let bad = [u16::from_ne_bytes([1, 5]), 0];
  let bad: &[u8; 4] = transmutant::transmute_ref(&bad);
  assert_eq!(
    transmutant::try_transmute_ref::<[u8; 4], ReadsPadding>(bad).unwrap_err(),
    Error::Validity
  );
  // In the variant of an enum, the same.
  let mut bytes = [0u8; 6];
  bytes[2] = 7;
  bytes[4..].copy_from_slice(&9u16.to_ne_bytes());
  let HoldsReadsPadding::A(value) =
    transmutant::try_transmute::<[u8; 6], HoldsReadsPadding>(bytes).unwrap();
  assert_eq!((value.a, value.b), (7, 9));
  bytes[3] = 5;
  assert_eq!(
    transmutant::try_transmute::<[u8; 6], HoldsReadsPadding>(bytes).err(),
    Some(Error::Validity)
  );
  // The same rule, at the same place, proves it: nothing is checked, nothing is read.
  let mut value =
    transmutant::transmute::<HoldsReadsPadding, HoldsReadsPadding>(HoldsReadsPadding::A(value));
  let HoldsReadsPadding::A(inner) = &mut value;
  assert_eq!(transmutant::transmute_ref::<ReadsPadding, ReadsPadding>(inner).b, 9);
  // A `&mut` is about what the type system enforces: rules are not invariants.
  let plain: &mut Plain = transmutant::transmute_mut(inner);
  plain.a = 3;
  assert_eq!(inner.a, 3);
}

/// An unsized struct whose slice has elements of no size is as large as its alignment says,
/// which may be more than the bytes it is made from.
#[test]
fn an_unsized_target_fits_in_the_source_with_its_trailing_padding() {
  #[derive(Data)]
  #[repr(C, align(4))]
  struct Aligned {
    a: u8,
    nothing: [()],
  }
  // One byte at an address that is a multiple of 4, at the end of its allocation.
  let words = [0u32; 2];
  let bytes: &[u8; 8] = transmutant::transmute_ref(&words);
  let one: &[u8] = &bytes[4..5];
  // A `&Aligned` is 4 bytes, 3 of which would be out of `one`.
  assert_eq!(transmutant::try_transmute_ref::<[u8], Aligned>(one).err(), Some(Error::Size));
  // Nor can any other number of bytes be one: its size without padding is 1.
  let four: &[u8] = &bytes[4..8];
  assert_eq!(transmutant::try_transmute_ref::<[u8], Aligned>(four).err(), Some(Error::Size));
}
