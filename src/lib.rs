// This crate provides a set of traits and utilities for handling types that can be
// efficiently packed, safely transmuted to/from byte representations, and those that
// are concrete with well-defined structures. It is designed for no_std environments.
#![cfg_attr(not(test), no_std)]
// #![feature(associated_type_bounds)]
#![feature(inline_const)]
// #![feature(const_option)]
#![feature(associated_type_defaults)]
#![feature(str_internals)]

use core::fmt::{Debug, Formatter};
use core::marker::{PhantomData, PhantomPinned};
use core::mem::{self, ManuallyDrop};
use core::num::{
  NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU128,
  NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize,
};
use core::slice;

// Re-export derive crate.
pub use mutant_derive::*;

mod tests;
mod unalign;
pub use unalign::*;

/// Error types for byte manipulation operations.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
  AlignmentMismatch,
  InsufficientBytes,
  InvalidBytePattern,
}

/// Represents types with consistent memory size across all valid byte patterns.
///
/// Types implementing [`Invariant`] have a memory size that remains unchanged across all
/// valid byte patterns, as determined by [`mem::size_of`]. However, they are **not necessarily**
/// [`Compact`], meaning their memory may not be fully initialized.
///
/// Invariant types, combined with [`Mutant`], allow for safe transmutation from bytes,
/// but only [`Invariant`] types can be safely transmuted from a mutable reference, as
/// mutating the type will not change the underlying memory size.
///
/// ## Safety
///
/// Implementors must ensure that all valid byte patterns keep a consistent memory size.
pub unsafe trait Invariant: Mutant + Copy + 'static {}

/// Represents types with fully initialized memory that can be safely transmuted to bytes.
///
/// Types implementing [`Compact`] have all their memory initialized (without padding bytes), thus
/// they can be safely transmuted to bytes. However, their memory size might not be
/// consistent across all valid byte patterns, meaning they are **not necessarily** [`Invariant`].
///
/// ## Safety
///
/// Implementors must ensure full memory initialization.
pub unsafe trait Compact: Mutant + 'static {
  /// Returns the size in memory (initialized).
  #[inline(always)]
  fn size(&self) -> usize {
    mem::size_of::<Self>()
  }
}

/// Represents types with a safe byte representation for transmutation, but with potential
/// limitations on valid byte patterns and memory initialization.
///
/// Types implementing [`Mutant`] can be safely transmuted from their byte representation,
/// but not all byte patterns may represent the same memory size (**not necessarily** [`Compact`])
/// nor is their memory fully initialized (**not necessarily** [`Compact`]). [`Mutant`] types are
/// suitable for general transmutation from bytes, but only [`Invariant`] types support safe
/// transmutation from a mutable references due to their consistent memory size.
///
/// ## Safety
///
/// Implementors must ensure that the `partition` method accurately reflects the
/// type's constraints and safe byte patterns for transmutation.
pub unsafe trait Mutant: Sized {
  /// Attempts to partition the input `bytes` into a segment validated for `Self` and the remaining
  /// bytes.
  #[inline(always)]
  fn partition(bytes: &[u8]) -> Result<Partition<'_>, Error> {
    Partition::from_split(bytes, mem::size_of::<Self>())
  }
}

/// Represents a valid state of `T`, typically a union field or an enum variant.
///
/// The `State` trait is used to define a valid state of a type `T` that implements [`Mutant`]. This
/// is particularly useful for union fields or enum variants, providing a way to treat these states
/// uniformly as `T`.
///
/// ## Safety
///
/// Implementors must ensure that the state represented is a valid state of `T`.
pub unsafe trait State<T: Mutant>: Mutant {
  /// Provides a uniform representation of the state as `T`.
  #[inline(always)]
  fn uniformize(&self) -> &T {
    unsafe { mem::transmute(self) }
  }
}

/// Represents two chunks of a byte slice. The first chunk (`validated`) is a valid byte
/// representation of `Self`, and the second chunk (`remaining`) contains the remaining bytes.
pub struct Partition<'a> {
  validated: &'a [u8],
  remaining: &'a [u8],
}

impl<'a> Partition<'a> {
  /// Constructs `Partition` from a byte slice and a size.
  /// Splits the slice into two parts: the first of the specified size and the rest.
  /// Returns `None` if the slice is smaller than the specified size.
  #[inline(always)]
  pub fn from_split(buf: &'a [u8], mid: usize) -> Result<Self, Error> {
    if mid <= buf.len() {
      let (chunk, rest) = buf.split_at(mid);
      Ok(Self { validated: chunk, remaining: rest })
    } else {
      Err(Error::InsufficientBytes)
    }
  }
}

pub struct Unsized<P: Invariant, T: ?Sized, S: Mutant = ()> {
  prefix: P,
  value: PhantomData<T>,
  suffix: PhantomData<S>,
}

// -------------------------------------------------------------------------------------------------

#[inline]
fn pack<T: Compact>(value: &T) -> &[u8] {
  match value.size() {
    0 => &[],
    n => unsafe { slice::from_raw_parts(value as *const T as *const u8, n) },
  }
}

#[inline]
fn pack_mut<T: Compact>(value: &mut T) -> &mut [u8] {
  match value.size() {
    0 => &mut [],
    n => unsafe { slice::from_raw_parts_mut(value as *mut T as *mut u8, n) },
  }
}

#[inline]
fn unpack<T: Mutant>(bytes: &[u8]) -> Result<&T, Error> {
  unpack_prefix(bytes).map(|(x, _)| x)
}

#[inline]
fn unpack_mut<T: Invariant>(bytes: &mut [u8]) -> Result<&mut T, Error> {
  unsafe { mem::transmute(unpack::<T>(bytes)) }
}

#[inline]
fn unpack_prefix<T: Mutant>(bytes: &[u8]) -> Result<(&T, &[u8]), Error> {
  let is_aligned =
    mem::align_of::<T>() == 1 || bytes.as_ptr().align_offset(mem::align_of::<T>()) == 0;
  if is_aligned {
    T::partition(bytes).map(|x| (unsafe { &*(x.validated.as_ptr() as *const T) }, x.remaining))
  } else {
    Err(Error::AlignmentMismatch)
  }
}

#[inline]
fn unpack_prefix_mut<T: Invariant>(
  bytes: &mut [u8],
) -> Result<(&mut T, &mut [u8]), Error> {
  unsafe { mem::transmute(unpack_prefix::<T>(bytes)) }
}

// -------------------------------------------------------------------------------------------------

// -------------------------------------------------------------------------------------------------

macro_rules! impl_rack {
  ($name:ident [$($t:ident $n:tt),+]) => {
    #[repr(C, packed)]
    #[derive(Copy, Clone)]
    pub struct $name<$($t),+>($(pub $t),+);

    unsafe impl<$($t: Mutant),+> Mutant for $name<$($t),+> {}
    unsafe impl<$($t: Compact),+> Compact for $name<$($t),+> {}
    unsafe impl<$($t: Invariant),+> Invariant for $name<$($t),+> {}

    impl<$($t: Debug + Copy),+> Debug for $name<$($t),+> {
      fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut l = f.debug_tuple("");
        $(l.field(&{ self.$n });)+
        l.finish()
      }
    }
  }
}

impl_rack!(Rack2  [A 0, B 1]);
impl_rack!(Rack3  [A 0, B 1, C 2]);
impl_rack!(Rack4  [A 0, B 1, C 2, D 3]);
impl_rack!(Rack5  [A 0, B 1, C 2, D 3, E 4]);
impl_rack!(Rack6  [A 0, B 1, C 2, D 3, E 4, F 5]);
impl_rack!(Rack7  [A 0, B 1, C 2, D 3, E 4, F 5, G 6]);
impl_rack!(Rack8  [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7]);
impl_rack!(Rack9  [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 ]);
impl_rack!(Rack10 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9]);
impl_rack!(Rack11 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10]);
impl_rack!(Rack12 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11]);
impl_rack!(Rack13 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11, M 12]);
impl_rack!(Rack14 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11, M 12, N 13]);

// -------------------------------------------------------------------------------------------------

macro_rules! impl_all {
  (unsafe impl $t:ident for [$($x:ty),+] $b:tt) => { $(unsafe impl $t for $x $b)+ };
  (unsafe impl $t:ident for [$($x:ty),+]) => { $(unsafe impl $t for $x {})+ };
}

impl_all!(
  unsafe impl Mutant for [
    (),
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64
  ]
);

impl_all!(
  unsafe impl Compact for [
    (), bool, char,
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64,
    NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
    NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
  ]
);

impl_all!(
  unsafe impl Invariant for [
    (),
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64
  ]
);

impl_all!(
  unsafe impl Mutant for [
    NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
    NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
  ] {
    fn partition(bytes: &[u8]) -> Result<Partition<'_>, Error> {
      Partition::from_split(bytes, mem::size_of::<Self>())
        .and_then(|x| if x.validated.iter().any(|x| *x != 0) {
          Ok(x)
        } else {
          Err(Error::InvalidBytePattern)
        })
    }
  }
);

unsafe impl Mutant for bool {
  fn partition(bytes: &[u8]) -> Result<Partition<'_>, Error> {
    Partition::from_split(bytes, 1).and_then(|x| match x.validated[0] {
      0 | 1 => Ok(x),
      _ => Err(Error::InvalidBytePattern),
    })
  }
}

unsafe impl Mutant for char {
  fn partition(bytes: &[u8]) -> Result<Partition<'_>, Error> {
    if !bytes.is_empty() {
      Partition::from_split(bytes, utf8_width::get_width(bytes[0]))
    } else {
      Err(Error::InsufficientBytes)
    }
  }
}

unsafe impl Mutant for PhantomPinned {}
unsafe impl Compact for PhantomPinned {}
unsafe impl Invariant for PhantomPinned {}

unsafe impl<T: ?Sized> Mutant for PhantomData<T> {}
unsafe impl<T: ?Sized + 'static> Compact for PhantomData<T> {}
unsafe impl<T: ?Sized + 'static> Invariant for PhantomData<T> {}

unsafe impl<T: Mutant> Mutant for ManuallyDrop<T> {}
unsafe impl<T: Compact> Compact for ManuallyDrop<T> {}
unsafe impl<T: Invariant> Invariant for ManuallyDrop<T> {}

unsafe impl<T: Mutant, const N: usize> Mutant for [T; N] {}
unsafe impl<T: Compact, const N: usize> Compact for [T; N] {}
unsafe impl<T: Invariant, const N: usize> Invariant for [T; N] {}
