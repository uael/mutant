//! Verdicts of the analysis that the soundness audit found wrong, as they must be now.
#![expect(dead_code, reason = "the types are only analysed, never made")]

use transmutant::{Data, hook::verdict};

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum SoftEnum {
  V(#[data(valid(0..=1))] u8),
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum HardEnum {
  V(bool),
}

/// `Layout::equals` compared what scalars require, not what they are known to be.
#[test]
fn soft_enum_is_not_hard_enum() {
  let v = const { verdict::<SoftEnum, HardEnum>() };
  assert!(!v.transmute && v.try_transmute);
  // The other way is fine: a `bool` is 0 or 1.
  assert!(const { verdict::<HardEnum, SoftEnum>() }.transmute);
  assert!(const { verdict::<SoftEnum, SoftEnum>() }.transmute);
}

#[derive(Data, Clone, Copy)]
#[repr(C)]
#[data(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

#[derive(Data, Clone, Copy)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum HoldsReadsPadding {
  A(ReadsPadding),
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum HoldsPlain {
  A(Plain),
}

/// A rule that reads the bytes of the value read the padding of the source (Miri: "using
/// uninitialized data").
#[test]
fn a_rule_that_reads_padding_needs_it_initialized() {
  let v = const { verdict::<Plain, ReadsPadding>() };
  assert!(!v.try_transmute_ref && v.why().contains("padding"), "{}", v.why());
  assert!(const { verdict::<[u8; 4], ReadsPadding>() }.try_transmute_ref);
  // The same rule, at the same place: nothing to check, nothing read.
  assert!(const { verdict::<ReadsPadding, ReadsPadding>() }.transmute_ref);
  // In the variant of an enum, from an enum and from bytes.
  let v = const { verdict::<HoldsPlain, HoldsReadsPadding>() };
  assert!(!v.try_transmute_ref && v.why().contains("padding"), "{}", v.why());
  let v = const { verdict::<(u8, u8, Plain), HoldsReadsPadding>() };
  assert!(!v.try_transmute_ref && v.why().contains("padding"), "{}", v.why());
  assert!(const { verdict::<[u8; 6], HoldsReadsPadding>() }.try_transmute);
  // A `&mut` is about what the type system enforces: rules do not matter.
  assert!(const { verdict::<Plain, ReadsPadding>() }.why().contains("padding"));
  assert!(const { verdict::<ReadsPadding, Plain>() }.transmute_mut);
}
