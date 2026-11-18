//! The types of the model, with their oracles. Each oracle is written from what the type is
//! declared to be, not from what the library makes of it.

use core::{mem::offset_of, num::NonZero};

use puns::{
  Pun,
  num::{
    u8c, u8in, u16be, u16bec, u16le, u16lec, u24be, u24bec, u24le, u24lec, u32be, u32bec, u32le,
    u32lec,
  },
};

use crate::{Bytes, Oracle, Uninit, Valid, all, int};

/// A struct whose oracle is the one of its fields, at the offsets that the compiler gives.
macro_rules! model_struct {
  ($(#[$attr:meta])* struct $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub struct $name { $(pub $field: $ty),* }

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
        all(&[$(<$ty as Oracle>::check(b, off + offset_of!($name, $field), reads, target)),*])
      }
    }
  };
}

/// A fieldless enum: its valid values are the bits of its variants, read from values of it.
macro_rules! model_fieldless {
  ($(#[$attr:meta])* enum $name:ident { $($variant:ident $(= $value:expr)?),+ $(,)? }) => {
    #[derive(Pun, Clone, Copy, PartialEq, Eq, Debug)]
    $(#[$attr])*
    pub enum $name { $($variant $(= $value)?),+ }

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
        let size = size_of::<$name>();
        let v = int(b, off, size, crate::NATIVE_LITTLE, reads)?;
        Ok([$($name::$variant),+].iter().any(|variant| {
          let mut bits = 0u128;
          let bytes: &[u8] =
            // SAFETY: a fieldless enum has no padding.
            unsafe { core::slice::from_raw_parts(core::ptr::from_ref::<$name>(variant).cast(), size) };
          for i in 0..size {
            let byte = if crate::NATIVE_LITTLE { bytes[size - 1 - i] } else { bytes[i] };
            bits = (bits << 8) | u128::from(byte);
          }
          bits == v
        }))
      }
    }
  };
}

// --- Size 1 ---------------------------------------------------------------------------------------

model_fieldless!(
  #[repr(u8)]
  enum E01 {
    A,
    B,
  }
);
model_fieldless!(
  #[repr(u8)]
  enum E37 {
    A = 3,
    B = 7,
    C = 200,
  }
);
model_fieldless!(
  #[repr(u8)]
  enum Eff {
    A = 255,
  }
);
model_fieldless!(
  #[repr(i8)]
  enum Eneg {
    A = -1,
    B = -128,
    C = 0,
  }
);
model_fieldless!(
  enum Erust {
    A,
    B,
    C,
  }
);

model_struct!(
  #[repr(C)]
  struct Flag {
    on: bool,
  }
);
model_struct!(
  struct Byte {
    v: u8,
    nothing: (),
  }
);
model_struct!(
  #[repr(transparent)]
  struct Ascii {
    c: u8in<0, 0x7f>,
  }
);

/// A soft restriction: required of a target, not known of a source.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub struct SoftBit {
  #[pun(valid(0..=1))]
  pub v: u8,
}

impl Oracle for SoftBit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(!target || v <= 1)
  }
}

/// A soft restriction of a type that is restricted already.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub struct SoftTrue {
  #[pun(valid(1))]
  pub v: bool,
}

impl Oracle for SoftTrue {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(v <= 1 && (!target || v == 1))
  }
}

/// A rule over a field.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 3 == 0))]
pub struct Third {
  pub v: u8,
}

impl Oracle for Third {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(!target || v % 3 == 0)
  }
}

pub type Size1 = (
  u8,
  i8,
  bool,
  NonZero<u8>,
  Option<NonZero<u8>>,
  u8c<0>,
  u8c<7>,
  u8c<255>,
  u8in<0, 0x7f>,
  u8in<1, 1>,
  u8in<0x80, 0xff>,
  u8in<2, 5>,
  E01,
  E37,
  Eff,
  Eneg,
  Erust,
  [u8; 1],
  [bool; 1],
  (bool,),
  Flag,
  Byte,
  Ascii,
  SoftBit,
  SoftTrue,
  Third,
  core::mem::ManuallyDrop<bool>,
);

// --- Size 2 ---------------------------------------------------------------------------------------

model_fieldless!(
  #[repr(u16)]
  enum E16 {
    A = 0x0102,
    B = 0x0201,
    C = 0xff00,
  }
);

model_struct!(
  #[repr(C)]
  struct ByteNz {
    a: u8,
    b: NonZero<u8>,
  }
);
model_struct!(
  #[repr(C)]
  struct NzByte {
    a: NonZero<u8>,
    b: u8,
  }
);
model_struct!(
  #[repr(C, align(2))]
  struct Aligned {
    a: u8,
  }
);
model_struct!(
  #[repr(C, align(2))]
  struct AlignedFlag {
    a: bool,
  }
);
model_struct!(
  #[repr(C, packed)]
  struct PackedU16 {
    a: u16,
  }
);
model_struct!(
  struct RustPair {
    a: bool,
    b: u8in<0, 3>,
  }
);

/// Fields and a rule that reads the value beyond them.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 2) != 0x0101))]
pub struct NotOneOne {
  pub a: u8,
  pub b: u8,
}

impl Oracle for NotOneOne {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 2, false, reads)?;
    Ok(!target || v != 0x0101)
  }
}

/// An enum with fields: the tag, then what the variant of that tag holds.
macro_rules! model_tagged {
  // `repr(C)` alone: the discriminants are implicit (0, 1...), and must be given in that order.
  (
    $(#[$attr:meta])* enum $name:ident: tag $tag_size:literal, implicit {
      $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+ $(,)?
    }
  ) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub enum $name { $($variant($($ty),*)),+ }
    model_tagged!(@oracle $name, $tag_size, $($variant($($ty = $sample),*) = $tag),+);
  };
  (
    $(#[$attr:meta])* enum $name:ident: tag $tag_size:literal {
      $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+ $(,)?
    }
  ) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub enum $name { $($variant($($ty),*) = $tag),+ }
    model_tagged!(@oracle $name, $tag_size, $($variant($($ty = $sample),*) = $tag),+);
  };
  (@oracle $name:ident, $tag_size:literal, $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+) => {

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
        let tag = int(b, off, $tag_size, crate::NATIVE_LITTLE, reads)?;
        $(
          if tag == $tag {
            // The offsets of the fields, from a value of the variant.
            let sample = $name::$variant($($sample),*);
            let offsets: Vec<usize> = model_tagged!(@offsets sample, $name::$variant, $($ty),*);
            let checks: Vec<fn(&Bytes, usize, &mut [bool], bool) -> Valid> =
              vec![$(<$ty as Oracle>::check),*];
            let results: Vec<Valid> =
              offsets.iter().zip(&checks).map(|(o, check)| check(b, off + o, reads, target)).collect();
            return all(&results);
          }
        )+
        Ok(false)
      }
    }
  };
  (@offsets $sample:ident, $path:path, ) => {{
    let _ = &$sample;
    vec![]
  }};
  (@offsets $sample:ident, $path:path, $a:ty) => {{
    let $path(x) = &$sample else { unreachable!() };
    vec![core::ptr::from_ref::<$a>(x).addr() - core::ptr::from_ref(&$sample).addr()]
  }};
  (@offsets $sample:ident, $path:path, $a:ty, $b:ty) => {{
    let $path(x, y) = &$sample else { unreachable!() };
    let base = core::ptr::from_ref(&$sample).addr();
    vec![core::ptr::from_ref::<$a>(x).addr() - base, core::ptr::from_ref::<$b>(y).addr() - base]
  }};
}

model_tagged!(#[repr(u8)] enum Opt: tag 1 { None() = 0, Some(u8 = 0) = 1 });
model_tagged!(#[repr(u8)] enum OptFlag: tag 1 { None() = 0, Some(bool = false) = 1 });
model_tagged!(#[repr(u8)] enum Either: tag 1 { L(NonZero<u8> = NonZero::new(1).unwrap()) = 0, R(u8in<0, 3> = u8in::new(0).unwrap()) = 1 });
model_tagged!(#[repr(C, u8)] enum EitherC: tag 1 { L(NonZero<u8> = NonZero::new(1).unwrap()) = 0, R(u8in<0, 3> = u8in::new(0).unwrap()) = 1 });
model_tagged!(#[repr(u8)] enum Skewed: tag 1 { A(u8 = 0) = 2, B(bool = false) = 9, C() = 4 });

/// The same, with a soft restriction in a variant: what the type system does not enforce.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum OptSoftBit {
  None,
  Some(#[pun(valid(0..=1))] u8),
}

impl Oracle for OptSoftBit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    match int(b, off, 1, true, reads)? {
      0 => Ok(true),
      1 => {
        let v = int(b, off + 1, 1, true, reads)?;
        Ok(!target || v <= 1)
      }
      _ => Ok(false),
    }
  }
}

pub type Size2 = (
  u16,
  i16,
  NonZero<u16>,
  Option<NonZero<u16>>,
  u16le,
  u16be,
  u16lec<0x1234>,
  u16bec<0x1234>,
  u16lec<1>,
  u16bec<0x100>,
  u16bec<0x3412>,
  [u8; 2],
  [bool; 2],
  (u8, bool),
  (bool, u8),
  [u8in<0, 0x7f>; 2],
  E16,
  ByteNz,
  NzByte,
  Aligned,
  AlignedFlag,
  PackedU16,
  RustPair,
  NotOneOne,
  Opt,
  OptFlag,
  Either,
  EitherC,
  Skewed,
  OptSoftBit,
  [E01; 2],
  (u8c<1>, u8),
);

// --- Size 3 ---------------------------------------------------------------------------------------

model_struct!(
  #[repr(C, packed)]
  struct PackedU16Bool {
    a: u16,
    b: bool,
  }
);
model_struct!(
  #[repr(C, packed)]
  struct PackedBoolNz {
    a: bool,
    b: NonZero<u16>,
  }
);
model_struct!(
  #[repr(C)]
  struct Triple {
    a: u8,
    b: bool,
    c: u8in<1, 0xfe>,
  }
);
model_tagged!(#[repr(u8)] enum Wide: tag 1 { A(u16le = u16le::new(0)) = 0, B(bool = false, bool = false) = 1 });
model_tagged!(#[repr(u8)] enum WideBe: tag 1 { A(u16be = u16be::new(0)) = 0, B(u8 = 0, u8 = 0) = 1 });
model_tagged!(#[repr(u8)] enum Lit: tag 1 { A(u16lec<0x1234> = u16lec::new()) = 0, B(u16bec<0x1234> = u16bec::new()) = 1 });

pub type Size3 = (
  [u8; 3],
  [bool; 3],
  (u8, u8, bool),
  u24le,
  u24be,
  u24lec<0x10>,
  u24bec<0x10>,
  PackedU16Bool,
  PackedBoolNz,
  Triple,
  Wide,
  WideBe,
  Lit,
  [E01; 3],
  (u8, [u8in<0x80, 0xff>; 2]),
);

// --- Size 4 ---------------------------------------------------------------------------------------

model_struct!(
  #[repr(C)]
  struct PadMid {
    a: u8,
    b: u16,
  }
);
model_struct!(
  #[repr(C)]
  struct PadEnd {
    a: u16,
    b: u8,
  }
);
model_struct!(
  #[repr(C)]
  struct PadEndFlag {
    a: u16,
    b: bool,
  }
);
model_fieldless!(
  #[repr(C)]
  enum CEnum {
    A,
    B = 0x10_ffff,
    C = 0xd800,
  }
);
model_tagged!(#[repr(u8)] enum Shape: tag 1 { Circle(u16 = 0) = 0, Rect(u8 = 0, bool = false) = 1 });
model_tagged!(#[repr(u8)] enum Drawing: tag 1 { Circle(u16 = 0) = 0, Rect(u8 = 0, u8 = 0) = 1, Nothing() = 2 });
model_tagged!(#[repr(C)] enum CTagged: tag 4, implicit { A(u8 = 0) = 0, B(bool = false) = 1 });
model_tagged!(#[repr(C, u8)] enum CU8: tag 1 { A(u16 = 0) = 0, B(u8 = 0) = 1 });
model_tagged!(#[repr(u16)] enum Tag16: tag 2 { A(u8 = 0) = 0x100, B(u16 = 0) = 3 });

/// A rule that reads padding: the oracle says that it reads it.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(le(1, 1) == 0))]
pub struct ReadsPad {
  pub a: u8,
  pub b: u16,
}

impl Oracle for ReadsPad {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let fields = all(&[u8::check(b, off, reads, target), u16::check(b, off + 2, reads, target)]);
    if !target {
      return fields;
    }
    // What the rule reads.
    let pad = int(b, off + 1, 1, true, reads);
    match (fields, pad) {
      (Err(Uninit), _) | (_, Err(Uninit)) => Err(Uninit),
      (Ok(valid), Ok(pad)) => Ok(valid && pad == 0),
    }
  }
}

pub type Size4 = (
  u32,
  i32,
  f32,
  char,
  NonZero<u32>,
  Option<NonZero<u32>>,
  u32le,
  u32be,
  u32lec<0x10_ffff>,
  u32bec<0xd800>,
  [u8; 4],
  [bool; 4],
  [u16be; 2],
  (u8, u16),
  PadMid,
  PadEnd,
  PadEndFlag,
  CEnum,
  Shape,
  Drawing,
  CU8,
  Tag16,
  ReadsPad,
  [u8in<0, 0x7f>; 4],
  (char,),
  [NonZero<u16>; 2],
);

// --- Size 8 ---------------------------------------------------------------------------------------

model_tagged!(#[repr(C)] enum CTaggedB: tag 4, implicit { A(bool = false) = 0, B(u8 = 0) = 1, C(u8in<0, 1> = u8in::new(0).unwrap()) = 2 });

pub type Size8 = (
  u64,
  [u8; 8],
  [bool; 8],
  [u32; 2],
  (u32, u32),
  CTagged,
  CTaggedB,
  [Shape; 2],
  [Drawing; 2],
  (u16, u8, bool, u32),
);

// --- Uninitialized bytes --------------------------------------------------------------------------

/// A byte that a target requires to be 0, and of which a source knows nothing.
///
/// Not even that it is initialized: as a type, it is any byte, since it holds a
/// `MaybeUninit<u8>`. This is what `Spec::maybe_uninit` describes.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Zero(pub core::mem::MaybeUninit<u8>);

// SAFETY: every value of `SPEC.valid` (0) is a value of a `MaybeUninit<u8>`, and what a source of
// it is known to be (anything, uninitialized too) is what a `MaybeUninit<u8>` is.
unsafe impl puns::Scalar for Zero {
  const SPEC: puns::layout::Spec =
    puns::layout::Spec::ranges::<u8>(puns::layout::Endian::Little, &[(0, 0)]).maybe_uninit();
}

impl Oracle for Zero {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // As a source, anything; what a target requires, a requirement of the library.
    if !target {
      return Ok(true);
    }
    Ok(int(b, off, 1, true, reads)? == 0)
  }
}

impl<T: Copy + 'static> Oracle for core::mem::MaybeUninit<T> {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

model_struct!(
  /// A byte, then one that may be uninitialized.
  #[repr(C)]
  struct ByteZero {
    a: u8,
    z: Zero,
  }
);

// --- Types without values -------------------------------------------------------------------------

/// A variant that holds a type without values: no value is that variant.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum Never {
  A(u8),
  B(core::convert::Infallible),
}

impl Oracle for Never {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    // `A(u8)`: the tag, then the byte; the tag of `B` is no value.
    let tag = int(b, off, 1, true, reads)?;
    if tag != 0 {
      return Ok(false);
    }
    int(b, off + 1, 1, true, reads)?;
    Ok(true)
  }
}
