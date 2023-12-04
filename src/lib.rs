// This crate provides a set of traits and utilities for handling types that can be
// efficiently packed, safely transmuted to/from byte representations, and those that
// are Mutable with well-defined structures. It is designed for no_std environments.
#![cfg_attr(not(test), no_std)]
// #![feature(associated_type_bounds)]
#![feature(inline_const)]
// #![feature(const_option)]
#![feature(associated_type_defaults)]
#![feature(str_internals)]

use core::convert::Infallible;
use core::fmt::{Debug, Formatter};
use core::marker::{PhantomData, PhantomPinned};
use core::mem::{self, ManuallyDrop};
use core::num::{
  NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU128,
  NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize,
};
use core::ops::Deref;
use core::slice;

// Re-export derive crate.
pub use mutant_derive::*;

mod tests;
mod unalign;
pub use unalign::*;

/// Error types for byte manipulation operations.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error<E>{
  SizeMismatch,
  AlignmentMismatch,
  InvalidBytePattern(E),
}

pub type Result<T, E = Infallible> = core::result::Result<T, Error<E>>;

pub unsafe trait Transmutable {
  type Error;

  /// Returns the size in memory (maybe uninitialized).
  fn size(&self) -> usize;

  /// Attempts to split input `bytes` into a segment validated for `Self` and the remaining bytes.
  fn split_cast(bytes: &[u8]) -> Result<Validated<'_, Self>, Self::Error>;
}

/// Represents types with a safe byte representation for transmutation, but with potential
/// limitations on valid byte patterns and memory initialization.
///
/// Types implementing [`Mutant`] can be safely transmuted from their byte representation,
/// but not all byte patterns may represent the same memory size (**not necessarily** [`Dense`])
/// nor is their memory fully initialized (**not necessarily** [`Dense`]). [`Mutant`] types are
/// suitable for general transmutation from bytes, but only [`Invariant`] types support safe
/// transmutation from mutable references due to their consistent memory size.
pub unsafe trait Mutant: Transmutable {

}

/// Represents types with fully initialized memory that can be safely transmuted to bytes.
///
/// Types implementing [`Dense`] have all their memory initialized (without padding bytes), thus
/// they can be safely transmuted to bytes. However, their memory size might not be
/// consistent across all valid byte patterns, meaning they are **not necessarily** [`Invariant`].
///
/// ## Safety
///
/// Implementors must ensure full memory initialization.
pub unsafe trait Dense: Mutant {}

/// Represents types with consistent memory size across all valid byte patterns.
///
/// Types implementing [`Invariant`] have a memory size that remains unchanged across all
/// valid byte patterns, as determined by [`mem::size_of`]. However, they are **not necessarily**
/// [`Dense`], meaning their memory may not be fully initialized.
///
/// [`Mutant`] types allow for safe transmutation from bytes,
/// but only [`Invariant`] types can be safely transmuted from a mutable reference, as
/// mutating the type will not change the underlying memory size.
///
/// ## Safety
///
/// Implementors must ensure that all valid byte patterns keep a consistent memory size.
pub unsafe trait Invariant: Mutant + Copy + 'static {}

pub unsafe trait Unstable: Mutant {
  fn size(&self) -> usize;
}

/// Represents [`Invariant`] types where all bytes patterns are valid byte representations.
///
/// Types implementing [`Stateless`] have a consistent memory size and accepts all byte patterns,
/// hence its bytes can be safely mutated.
///
/// ## Safety
///
/// Implementors must ensure that all bytes patterns are valid byte representations.
pub unsafe trait Stateless: Invariant {}

/// Represents types where only some bytes patterns are valid.
pub unsafe trait Statefull: Mutant {
  type Error;

  /// Attempts to partition the input `bytes` into a segment validated for `Self` and the remaining
  /// bytes.
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, Self::Error>;
}

/// Represents a valid state of `T`, typically a union field or an enum variant.
///
/// The `State` trait is used to define a valid state of a type `T` that implements [`OldMutant`]. This
/// trait is particularly useful for union fields or enum variants, providing a way to treat these
/// states uniformly as `T` through upcasting.
///
/// ## Safety
///
/// Implementors of this trait must ensure that the state represented is a valid state of `T`. The
/// `upcast` method should only be used when it's guaranteed that the state can be safely treated as
/// the more general type `T`.
pub unsafe trait State<T: Statefull>: Statefull {
  /// Upcast the state to the more general `T` type.
  #[inline(always)]
  fn upcast(&self) -> &T {
    unsafe { mem::transmute(self) }
  }
}

/// Represents the head of a struct with trailing bytes.
///
/// The `Head` trait is used to define [`Rack`] structs where the head is followed by trailing
/// bytes (without padding in-between). The [`Invariant`] requirement ensure consistent memory size
/// of the head, hence the complete size of the [`Rack`] including the trailing bytes is always
/// [`mem::size_of`]`::<Self> + self.tail_size()`.
pub trait Head: Stateless {
  fn tail_size(self) -> usize;
}

#[repr(transparent)]
pub struct Rack<H: Head, T: Statefull = u8> {
  head: H,
  tail: PhantomData<[T]>,
}

/// Represents two chunks of a byte slice. The first chunk (`validated`) is a valid byte
/// representation of `Self`, and the second chunk (`remaining`) contains the remaining bytes.
pub struct Validated<'a, T: Transmutable> {
  pub validated: &'a [u8],
  pub remaining: &'a [u8],
  _marker: PhantomData<T>,
}

impl<'a, T: Transmutable> Validated<'a, T> {
  /// Constructs `Partition` from a byte slice and a size.
  /// Splits the slice into two parts: the first of the specified size and the rest.
  /// Returns an error if the slice is smaller than the specified size.
  #[inline(always)]
  pub fn try_from(
    bytes: &'a [u8],
    mid: usize,
    validate: impl FnOnce(&'a [u8]) -> core::result::Result<(), T::Error>
  ) -> Result<Self, T::Error> {
    let is_aligned =
      mem::align_of::<T>() == 1 || bytes.as_ptr().align_offset(mem::align_of::<T>()) == 0;
    if is_aligned && mid <= bytes.len() {
      let (validated, remaining) = bytes.split_at(mid);
      validate(validated)?;
      Ok(Self { validated, remaining, _marker: PhantomData })
    } else if !is_aligned {
      Err(Error::AlignmentMismatch)
    } else {
      Err(Error::SizeMismatch)
    }
  }
}

// -------------------------------------------------------------------------------------------------

#[inline(always)]
pub fn pack<T: Dense + Invariant>(value: &T) -> &[u8] {
  unsafe { slice::from_raw_parts(value as *const T as *const u8, mem::size_of::<T>()) }
}

#[inline(always)]
pub fn pack_mut<T: Dense + Stateless>(value: &T) -> &[u8] {
  unsafe { slice::from_raw_parts_mut(value as *mut T as *mut u8, mem::size_of::<T>()) }
}

#[inline(always)]
pub fn pack_initialized<T: Dense + Unstable>(value: &T) -> &[u8] {
  unsafe { slice::from_raw_parts(value as *const T as *const u8, value.size()) }
}

#[inline(always)]
pub fn transmute_prefix<T: Stateless>(bytes: &[u8]) -> Result<(&T, &[u8])> {
  Validated::try_from(bytes, mem::size_of::<T>(), |_| Ok(()))
    .map(|Validated { validated, remaining }| (
      unsafe { &*(validated.as_ptr() as *const T) },
      remaining
    ))
}

#[inline(always)]
pub fn transmute_prefix_mut<T: Stateless>(bytes: &mut [u8]) -> Result<(&mut T, &mut [u8])> {
  unsafe { mem::transmute(transmute_prefix::<T>(bytes)) }
}

#[inline(always)]
pub fn unpack_prefix<T: Statefull>(bytes: &[u8]) -> Result<(&T, &[u8]), T::Error> {
  T::partition(bytes)
    .map(|Validated { validated, remaining }| (
      unsafe { &*(validated.as_ptr() as *const T) },
      remaining
    ))
}

#[inline(always)]
pub fn unpack_prefix_mut<T: Statefull + Invariant>(bytes: &mut [u8])
  -> Result<(&mut T, &mut [u8]), T::Error>
{
  unsafe { mem::transmute(unpack_prefix::<T>(bytes)) }
}

#[inline(always)]
pub fn transmute<T: Stateless>(bytes: &[u8]) -> Result<&T> {
  transmute_prefix::<T>(bytes)
    .and_then(|(value, remaining)| if remaining.is_empty() {
      Ok(value)
    } else {
      Err(Error::SizeMismatch)
    })
}

#[inline(always)]
pub fn transmute_mut<T: Stateless>(bytes: &mut [u8]) -> Result<&mut T> {
  unsafe { mem::transmute(transmute::<T>(bytes)) }
}

#[inline(always)]
pub fn unpack<T: Statefull>(bytes: &[u8]) -> Result<&T, T::Error> {
  unpack_prefix::<T>(bytes)
    .and_then(|(value, remaining)| if remaining.is_empty() {
      Ok(value)
    } else {
      Err(Error::SizeMismatch)
    })
}

#[inline(always)]
pub fn unpack_mut<T: Statefull + Invariant>(bytes: &mut [u8]) -> Result<&mut T, T::Error> {
  unsafe { mem::transmute(unpack(bytes)) }
}

// -------------------------------------------------------------------------------------------------

unsafe impl<H: Head, T: Dense> Dense for Rack<H, T> {}

unsafe impl<H: Head, T: Statefull> Statefull for Rack<H, T> {
  type Error = T::Error;

  #[inline(always)]
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, T::Error> {
    unpack_prefix::<H>(bytes)
      .and_then(|(head, mut tail)| {
        let tail_size = head.tail_size();
        let full = Validated::from_split(bytes, mem::size_of::<H>() + tail_size)?;
        tail = &tail[..tail_size];
        while !tail.is_empty() {
          (_, tail) = T::unpack_prefix(tail)?;
        }
        Ok(full)
      })
  }
}

impl<H: Head, T: Invariant> Deref for Rack<H, T> {
  type Target = [T];

  fn deref(&self) -> &Self::Target {
    todo!()
  }
}

impl<H: Head, T: Statefull> Rack<H, T> {
  #[inline]
  pub fn head(&self) -> H {
    self.head
  }

  pub fn tail(&self) -> &[u8] where T: Dense {
    &self.pack()[mem::size_of::<H>()..]
  }

  // pub fn tail_mut(&mut self) -> &mut [u8] where T: Dense + Mutable {
  //   &mut pack_mut(self)[mem::size_of::<H>()..]
  // }
}

// #[test]
// fn test() {
//   let mut x = true;
//   pack_mut(&mut x)[0] = 3;
// }

// -------------------------------------------------------------------------------------------------

macro_rules! impl_concat {
  ($name:ident [$($t:ident $n:tt),+]) => {
    #[repr(C, packed)]
    #[derive(Copy, Clone)]
    pub struct $name<$($t),+>($(pub $t),+);

    impl<$($t),+> From<($($t),+)> for $name<$($t),+> {
      #[inline(always)]
      fn from(value: ($($t),+)) -> Self {
        #[allow(non_snake_case)]
        let ($($t),+) = value;
        Self($($t),+)
      }
    }

    unsafe impl<$($t: Dense),+> Dense for $name<$($t),+> {}
    unsafe impl<$($t: Invariant),+> Invariant for $name<$($t),+> {}
    unsafe impl<$($t: Stateless),+> Stateless for $name<$($t),+> {}

    impl<$($t: Debug + Copy),+> Debug for $name<$($t),+> {
      fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut l = f.debug_tuple("");
        $(l.field(&{ self.$n });)+
        l.finish()
      }
    }
  }
}

impl_concat!(Concat2  [A 0, B 1]);
impl_concat!(Concat3  [A 0, B 1, C 2]);
impl_concat!(Concat4  [A 0, B 1, C 2, D 3]);
impl_concat!(Concat5  [A 0, B 1, C 2, D 3, E 4]);
impl_concat!(Concat6  [A 0, B 1, C 2, D 3, E 4, F 5]);
impl_concat!(Concat7  [A 0, B 1, C 2, D 3, E 4, F 5, G 6]);
impl_concat!(Concat8  [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7]);
impl_concat!(Concat9  [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 ]);
impl_concat!(Concat10 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9]);
impl_concat!(Concat11 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10]);
impl_concat!(Concat12 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11]);
impl_concat!(Concat13 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11, M 12]);
impl_concat!(Concat14 [A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 , J 9, K 10, L 11, M 12, N 13]);

// -------------------------------------------------------------------------------------------------

macro_rules! impl_all {
  (impl $t:ident for [$($x:ty),+] $b:tt) => { $(impl $t for $x $b)+ };
  (impl $t:ident for [$($x:ty),+]) => { $(impl $t for $x {})+ };
  (unsafe impl $t:ident for [$($x:ty),+] $b:tt) => { $(unsafe impl $t for $x $b)+ };
  (unsafe impl $t:ident for [$($x:ty),+]) => { $(unsafe impl $t for $x {})+ };
}

// impl_all!(
//   unsafe impl Mutant for [
//     (), bool, char,
//     u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64,
//     NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
//     NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
//   ]
// );

impl_all!(
  unsafe impl Dense for [
    (), bool, char,
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64,
    NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
    NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
  ]
);

impl_all!(
  unsafe impl Invariant for [
    (), bool,
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64,
    NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
    NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
  ]
);

impl_all!(
  unsafe impl Stateless for [
    (),
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64
  ]
);

impl_all!(
  impl Head for [
    u8, u16, u32, u64, usize
  ] {
    #[inline]
    fn tail_size(self) -> usize {
      self as usize
    }
  }
);

impl_all!(
  unsafe impl Statefull for [
    NonZeroU8, NonZeroI8, NonZeroU16, NonZeroI16, NonZeroU32, NonZeroI32, NonZeroU64,
    NonZeroI64, NonZeroU128, NonZeroI128, NonZeroUsize, NonZeroIsize
  ] {
    #[inline]
    fn partition(bytes: &[u8]) -> Result<Validated<'_>, &'static str> {
      Validated::try_from(bytes, mem::size_of::<Self>(), |bytes| {
        if bytes.iter().any(|x| *x != 0) {
          Ok(())
        } else {
          Err("non-zero type with a zero value")
        }
      })
    }
  }
);

impl Head for () {
  #[inline]
  fn tail_size(self) -> usize {
    0
  }
}

unsafe impl Statefull for bool {
  type Error = &'static str;

  #[inline]
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, Self::Error> {
    Validated::from_split(bytes, 1).and_then(|x| match x.validated[0] {
      0 | 1 => Ok(x),
      _ => Err(Error::InvalidBytePattern("only bytes `0` and `1` are valid booleans")),
    })
  }
}

unsafe impl Statefull for char {
  type Error = core::str::Utf8Error;
  #[inline]
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, Self::Error> {
    if !bytes.is_empty() {
      Validated::try_from(bytes, utf8_width::get_width(bytes[0]), |bytes| {
        core::str::from_utf8(bytes).map(|_| ())
      })
    } else {
      Err(Error::SizeMismatch)
    }
  }
}

unsafe impl Unstable for char {
  #[inline]
  fn size(&self) -> usize {
    self.len_utf8()
  }
}

unsafe impl Dense for PhantomPinned {}
unsafe impl Invariant for PhantomPinned {}
unsafe impl Stateless for PhantomPinned {}

unsafe impl<T: ?Sized> Dense for PhantomData<T> {}
unsafe impl<T: ?Sized + 'static> Invariant for PhantomData<T> {}
unsafe impl<T: ?Sized> Stateless for PhantomData<T> {}

unsafe impl<T: Dense> Dense for ManuallyDrop<T> {}
unsafe impl<T: Invariant> Invariant for ManuallyDrop<T> {}
unsafe impl<T: Unstable> Unstable for ManuallyDrop<T> {
  #[inline(always)]
  fn size(&self) -> usize {
    T::size(self)
  }
}
unsafe impl<T: Statefull> Statefull for ManuallyDrop<T> {
  type Error = T::Error;

  #[inline(always)]
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, T::Error> {
    unsafe { mem::transmute(T::partition(bytes)) }
  }
}
// unsafe impl<T: Stateless> Stateless for ManuallyDrop<T> {}

unsafe impl<T: Dense, const N: usize> Dense for [T; N] {}
unsafe impl<T: Invariant, const N: usize> Invariant for [T; N] {}
unsafe impl<T: Stateless, const N: usize> Stateless for [T; N] {}
unsafe impl<T: Statefull + Invariant, const N: usize> Statefull for [T; N] {
  type Error = T::Error;

  #[inline]
  fn partition(bytes: &[u8]) -> Result<Validated<'_>, T::Error> {
    Validated::try_from(bytes, mem::size_of::<Self>(), |mut bytes| {
      while !bytes.is_empty() {
        bytes = match T::unpack_prefix(bytes) {
          Ok((_, bytes)) => bytes,
          Err(Error::InvalidBytePattern { details }) => return Some(details),
          _ => unreachable!(),
        }
      }
      None
    })
  }
}
