#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]
pub use mutant_derive::Type;

#[cfg(test)]
mod tests;

use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::num::NonZero;
use core::ptr::{from_mut, from_ref};

/// # mutant
///
///

#[macro_export]
macro_rules! field {
  ($($fields:tt).+: $ty:ty $(,)?) => ((
    $crate::num::LiteralUint<usize, { $crate::offset_of!(Self, $($fields).+) as u128 }>,
    <$ty as $crate::Type>::Layout,
  ));
}

// -------------------------------------------------------------------------------------------------
// marker

pub trait Literal<T> {
  const LITERAL: T;
}

pub trait Default: Sized {
  const DEFAULT: Self;
}

pub trait Zero: Sized {
  const ZERO: Self;
}

pub trait Ones: Sized {
  const ONES: Self;
}

/// # Safety
/// This trait **must** only be implemented for byte array as of `[u8; Self::LEN]`.
pub unsafe trait ByteArray: Copy + Zero + Ones + 'static {
  const LEN: usize;
}

pub const fn bytes_of<T: ByteArray>(val: &T) -> &[u8] {
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &*ptr::from_raw_parts(from_ref(val).cast(), T::LEN) }
}

pub const fn bytes_of_mut<T: ByteArray>(val: &mut T) -> &mut [u8] {
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &mut *ptr::from_raw_parts_mut(from_mut(val).cast(), T::LEN) }
}

#[inline]
const unsafe fn transmute_unchecked<T: Copy, U: Copy>(val: T) -> U {
  union Transmute<T: Copy, U: Copy> {
    val: T,
    dst: U,
  }
  Transmute { val }.dst
}

// impl marker traits.
const _: () = {
  use core::num::NonZero;

  impl Zero for bool {
    const ZERO: Self = false;
  }

  impl Zero for char {
    const ZERO: Self = '\0';
  }

  impl<T: Zero, const N: usize> Zero for [T; N] {
    const ZERO: Self = [T::ZERO; N];
  }

  impl<T: Ones, const N: usize> Ones for [T; N] {
    const ONES: Self = [T::ONES; N];
  }

  unsafe impl<const N: usize> ByteArray for [u8; N] {
    const LEN: usize = N;
  }

  macro_rules! impl_non_zero {
    { $($T:ident)+ } => {$(
      impl Ones for NonZero<$T> {
        const ONES: Self = unsafe { NonZero::<$T>::new_unchecked($T::ONES) };
      }
    )+};
  }

  impl_non_zero! { u8 u16 u32 u64 u128 }
};

// -----------------------------------------------------------------------------------------------
// endian

pub mod endian {
  use crate::Literal;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Le;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Be;

  #[cfg(target_endian = "little")]
  pub type Ne = Le;
  #[cfg(target_endian = "big")]
  pub type Ne = Be;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub enum Endian {
    Le,
    Be,
  }

  pub trait Endianness: Literal<Endian> + sealed::Endianness {}

  mod sealed {
    pub trait Endianness {}
  }

  // impl
  const _: () = {
    use crate::Default;

    impl sealed::Endianness for Le {}
    impl sealed::Endianness for Be {}

    impl Endianness for Le {}
    impl Endianness for Be {}

    impl Literal<Endian> for Le {
      const LITERAL: Endian = Endian::Le;
    }

    impl Literal<Endian> for Be {
      const LITERAL: Endian = Endian::Be;
    }

    impl Default for Endian {
      const DEFAULT: Self = Ne::LITERAL;
    }

    impl Endian {
      #[inline]
      pub const fn is_native(&self) -> bool {
        match self {
          Endian::Le => cfg!(target_endian = "little"),
          Endian::Be => cfg!(target_endian = "big"),
        }
      }

      #[inline]
      pub const fn is_little(&self) -> bool {
        matches!(self, Endian::Le)
      }

      #[inline]
      pub const fn is_big(&self) -> bool {
        matches!(self, Endian::Be)
      }
    }
  };
}

// -----------------------------------------------------------------------------------------------
// sign

pub mod sign {
  use crate::Literal;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Signed;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Unsigned;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub enum Sign {
    Signed,
    Unsigned,
  }

  pub trait Signedness: Literal<Sign> + sealed::Signedness {}

  mod sealed {
    pub trait Signedness {}
  }

  // impl
  const _: () = {
    use crate::Default;

    impl sealed::Signedness for Signed {}
    impl sealed::Signedness for Unsigned {}

    impl Signedness for Signed {}
    impl Signedness for Unsigned {}

    impl Literal<Sign> for Signed {
      const LITERAL: Sign = Sign::Signed;
    }

    impl Literal<Sign> for Unsigned {
      const LITERAL: Sign = Sign::Unsigned;
    }

    impl Default for Sign {
      const DEFAULT: Self = Signed::LITERAL;
    }

    impl Sign {
      #[inline]
      pub const fn is_signed(&self) -> bool {
        matches!(self, Sign::Signed)
      }

      #[inline]
      pub const fn is_unsigned(&self) -> bool {
        matches!(self, Sign::Unsigned)
      }
    }
  };
}

// -------------------------------------------------------------------------------------------------
// num

pub mod num {
  use crate::{
    endian, endian::Endianness, transmute_unchecked, ByteArray, Default, Literal, Ones, Zero,
  };
  use core::cmp::Ordering;
  use core::fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex};
  use core::ops::Add;

  pub use types::*;

  // -----------------------------------------------------------------------------------------------
  // UnsignedLiteral

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct LiteralUint<T: Numeric, const VALUE: u128>(T);

  // impl `LiteralUint`.
  const _: () = {
    impl<T: Numeric, const VALUE: u128> LiteralUint<T, VALUE> {
      #[inline]
      pub const fn new() -> Self {
        Self(Self::LITERAL)
      }

      #[inline]
      pub const fn into_inner(self) -> T {
        Self::LITERAL
      }
    }

    impl<T: Numeric, const VALUE: u128> Literal<T> for LiteralUint<T, VALUE> {
      const LITERAL: T = match cast_checked(VALUE) {
        Some(x) => x,
        _ => panic!("literal `VALUE` is too large for type `T`."),
      };
    }

    impl<T: Numeric, const VALUE: u128> Minimum for LiteralUint<T, VALUE> {
      const MIN: Self = Self::new();
    }

    impl<T: Numeric, const VALUE: u128> Maximum for LiteralUint<T, VALUE> {
      const MAX: Self = Self::new();
    }
  };

  // -----------------------------------------------------------------------------------------------
  // ...

  pub type NativeOf<T> = <T as Numeric>::Native;

  pub unsafe trait Numeric: Zero + Ones + Minimum + Maximum + Copy + 'static {
    type Bytes: ByteArray;
    type Native: Native<Endian = endian::Ne, Bytes = Self::Bytes>;
    type Endian: Endianness;
  }

  pub trait Minimum: Sized {
    const MIN: Self;
  }

  pub trait Maximum: Sized {
    const MAX: Self;
  }

  pub trait Native: Numeric<Endian = endian::Ne, Native = Self> {}

  impl<T: Numeric<Endian = endian::Ne, Native = T>> Native for T {}

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Le<T: Native>(T);

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Be<T: Native>(T);

  #[inline]
  pub const fn swap_bytes<T: Numeric + Default>(n: T) -> T {
    #[inline]
    const fn swap<const N: usize>(bytes: [u8; N]) -> [u8; N] {
      let mut i = 0;
      let mut result = [0; N];
      while i < N {
        result[i] = bytes[N - 1 - i];
        i += 1;
      }
      result
    }

    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    const {
      assert!(size_of::<T>() <= 16);
    }

    macro_rules! swap_bytes {
      ($n:ident $($N:literal)+) => {$(
        if const { size_of::<T::Bytes>() == $N } {
          let bytes: [u8; $N] = unsafe { transmute_unchecked($n) };
          return from_bytes(unsafe { transmute_unchecked(swap::<$N>(bytes)) });
        }
      )+};
    }

    swap_bytes!(n 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
    unsafe { core::hint::unreachable_unchecked() }
  }

  #[inline]
  pub const fn from_bytes<T: Numeric>(bytes: T::Bytes) -> T {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(bytes) }
  }

  #[inline]
  pub const fn to_bytes<T: Numeric>(n: T) -> T::Bytes {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(n) }
  }

  #[inline]
  pub const fn from_ne_bytes<T: Numeric>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_le_bytes<T: Numeric>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_be_bytes<T: Numeric>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_ne_bytes<T: Numeric>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_native() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_le_bytes<T: Numeric>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_little() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_be_bytes<T: Numeric>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_big() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_ne<T: Numeric>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_be<T: Numeric>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_le<T: Numeric>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn cast<T: Numeric, U: Numeric>(n: T) -> U {
    union Cast<T: Copy, U: Copy> {
      src: T,
      dst: U,
    }

    let mut cast = Cast { dst: U::Bytes::ZERO };
    cast.src = to_le_bytes(n); // TODO: I'm sure about that ?
    from_le_bytes(unsafe { cast.dst })
  }

  #[inline]
  pub const fn cast_checked<T: Numeric, U: Numeric>(n: T) -> Option<U> {
    let new_n = cast(n);
    match is_eq(cast(new_n), n) {
      true => Some(new_n),
      _ => None,
    }
  }

  #[inline]
  pub const fn count_zeros<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).count_zeros()
  }

  #[inline]
  pub const fn count_ones<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).count_ones()
  }

  #[inline]
  pub const fn leading_zeros<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).leading_zeros()
  }

  #[inline]
  pub const fn trailing_zeros<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_zeros()
  }

  #[inline]
  pub const fn leading_ones<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).leading_ones()
  }

  #[inline]
  pub const fn trailing_ones<T: Numeric>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_ones()
  }

  #[inline]
  pub const fn rotate_left<T: Numeric>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_left(count))
  }

  #[inline]
  pub const fn rotate_right<T: Numeric>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_right(count))
  }

  #[inline]
  pub const fn not<T: Numeric>(n: T) -> T {
    cast(!cast::<T, u128>(n))
  }

  #[inline]
  pub const fn add<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) + cast::<T, u128>(b))
  }

  #[inline]
  pub const fn sub<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) - cast::<T, u128>(b))
  }

  #[inline]
  pub const fn mul<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) * cast::<T, u128>(b))
  }

  #[inline]
  pub const fn div<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) / cast::<T, u128>(b))
  }

  #[inline]
  pub const fn rem<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) % cast::<T, u128>(b))
  }

  #[inline]
  pub const fn shr<T: Numeric>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) >> b)
  }

  #[inline]
  pub const fn shl<T: Numeric>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) << b)
  }

  #[inline]
  pub const fn and<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) & cast::<T, u128>(b))
  }

  #[inline]
  pub const fn xor<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) ^ cast::<T, u128>(b))
  }

  #[inline]
  pub const fn or<T: Numeric>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) | cast::<T, u128>(b))
  }

  #[inline]
  pub const fn cmp<T: Numeric>(a: T, b: T) -> Ordering {
    let (a, b): (u128, u128) = (cast(a), cast(b));
    if a < b {
      Ordering::Less
    } else if a > b {
      Ordering::Greater
    } else {
      Ordering::Equal
    }
  }

  #[inline]
  pub const fn is_lt<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less)
  }

  #[inline]
  pub const fn is_le<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Equal)
  }

  #[inline]
  pub const fn is_gt<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater)
  }

  #[inline]
  pub const fn is_ge<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater | Ordering::Equal)
  }

  #[inline]
  pub const fn is_eq<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Equal)
  }

  #[inline]
  pub const fn is_ne<T: Numeric>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Greater)
  }

  // TODO: wrapping_add, overflowing_add, ..

  #[allow(non_camel_case_types)]
  mod types {
    use super::*;

    pub type le8 = Le<u8>;
    pub type le16 = Le<u16>;
    pub type le24 = Le<u24>;
    pub type le32 = Le<u32>;
    pub type le40 = Le<u40>;
    pub type le48 = Le<u48>;
    pub type le56 = Le<u56>;
    pub type le64 = Le<u64>;
    pub type le72 = Le<u72>;
    pub type le80 = Le<u80>;
    pub type le88 = Le<u88>;
    pub type le96 = Le<u96>;
    pub type le104 = Le<u104>;
    pub type le112 = Le<u112>;
    pub type le120 = Le<u120>;
    pub type le128 = Le<u128>;

    pub type be8 = Be<u8>;
    pub type be16 = Be<u16>;
    pub type be24 = Be<u24>;
    pub type be32 = Be<u32>;
    pub type be40 = Be<u40>;
    pub type be48 = Be<u48>;
    pub type be56 = Be<u56>;
    pub type be64 = Be<u64>;
    pub type be72 = Be<u72>;
    pub type be80 = Be<u80>;
    pub type be88 = Be<u88>;
    pub type be96 = Be<u96>;
    pub type be104 = Be<u104>;
    pub type be112 = Be<u112>;
    pub type be120 = Be<u120>;
    pub type be128 = Be<u128>;

    pub type u8le = Le<u8>;
    pub type u16le = Le<u16>;
    pub type u24le = Le<u24>;
    pub type u32le = Le<u32>;
    pub type u40le = Le<u40>;
    pub type u48le = Le<u48>;
    pub type u56le = Le<u56>;
    pub type u64le = Le<u64>;
    pub type u72le = Le<u72>;
    pub type u80le = Le<u80>;
    pub type u88le = Le<u88>;
    pub type u96le = Le<u96>;
    pub type u104le = Le<u104>;
    pub type u112le = Le<u112>;
    pub type u120le = Le<u120>;
    pub type u128le = Le<u128>;

    pub type u8be = Be<u8>;
    pub type u16be = Be<u16>;
    pub type u24be = Be<u24>;
    pub type u32be = Be<u32>;
    pub type u40be = Be<u40>;
    pub type u48be = Be<u48>;
    pub type u56be = Be<u56>;
    pub type u64be = Be<u64>;
    pub type u72be = Be<u72>;
    pub type u80be = Be<u80>;
    pub type u88be = Be<u88>;
    pub type u96be = Be<u96>;
    pub type u104be = Be<u104>;
    pub type u112be = Be<u112>;
    pub type u120be = Be<u120>;
    pub type u128be = Be<u128>;

    // TODO
    // pub type i8le = Le<i8>;
    // pub type i16le = Le<i16>;
    // pub type i24le = Le<i24>;
    // pub type i32le = Le<i32>;
    // pub type i48le = Le<i48>;
    // pub type i56le = Le<i56>;
    // pub type i64le = Le<i64>;
    // pub type i72le = Le<i72>;
    // pub type i80le = Le<i80>;
    // pub type i88le = Le<i88>;
    // pub type i96le = Le<i96>;
    // pub type i104le = Le<i104>;
    // pub type i112le = Le<i112>;
    // pub type i120le = Le<i120>;
    // pub type i128le = Le<i128>;

    // TODO
    // pub type i8be = Be<i8>;
    // pub type i16be = Be<i16>;
    // pub type i24be = Be<i24>;
    // pub type i32be = Be<i32>;
    // pub type i48be = Be<i48>;
    // pub type i56be = Be<i56>;
    // pub type i64be = Be<i64>;
    // pub type i72be = Be<i72>;
    // pub type i80be = Be<i80>;
    // pub type i88be = Be<i88>;
    // pub type i96be = Be<i96>;
    // pub type i104be = Be<i104>;
    // pub type i112be = Be<i112>;
    // pub type i120be = Be<i120>;
    // pub type i128be = Be<i128>;
  }

  impl<T: Numeric> Zero for T {
    const ZERO: Self = unsafe { core::mem::zeroed() };
  }

  impl<T: Numeric> Ones for T {
    const ONES: Self = not(Self::ZERO);
  }

  impl<T: Numeric> Minimum for T {
    const MIN: Self = Self::ZERO;
  }

  impl<T: Numeric> Maximum for T {
    const MAX: Self = Self::ONES;
  }

  impl<T: Default> Literal<T> for () {
    const LITERAL: T = T::DEFAULT;
  }

  unsafe impl Numeric for () {
    type Bytes = [u8; 0];
    type Native = Self;
    type Endian = endian::Ne;
  }

  unsafe impl Numeric for usize {
    type Bytes = [u8; size_of::<Self>()];
    type Native = Self;
    type Endian = endian::Ne;
  }

  unsafe impl<T: Native> Numeric for Le<T> {
    type Bytes = T::Bytes;
    type Native = T;
    type Endian = endian::Le;
  }

  unsafe impl<T: Native> Numeric for Be<T> {
    type Bytes = T::Bytes;
    type Native = T;
    type Endian = endian::Be;
  }

  macro_rules! adt_nums {
    { $($name:ident $size:literal $align:literal $raw:ident)+ } => {$(
      #[repr(align($align))]
      #[allow(non_camel_case_types)]
      #[derive(Copy, Clone, PartialEq, Eq, Hash)]
      pub struct $name([u8; $size]);

      impl_adt! { () $name[$raw] ([u8; $size]) }
      impl_adt_from! { $name[$raw] }
    )+};
  }

  macro_rules! impl_num {
    { $($name:ident $size:literal)+ } => {$(
      unsafe impl Numeric for $name {
        type Bytes = [u8; $size];
        type Native = Self;
        type Endian = endian::Ne;
      }
    )+};
  }

  macro_rules! impl_adt_from {
    { $T:ty[$Raw:ident] } => {
      impl From<$T> for $Raw {
        #[inline]
        fn from(value: $T) -> Self {
          cast(value)
        }
      }
    };
  }

  macro_rules! impl_adt {
    (($($generics:tt)*) $T:ty[$Raw:ty] ($Bytes:ty)) => {
      impl <$($generics)*> $T {
        #[inline]
        pub const fn from<U: Numeric>(n: U) -> Self {
          cast(n)
        }

        #[inline]
        pub const fn into<U: Numeric>(self) -> U {
          cast(self)
        }

        #[inline]
        pub const fn from_bytes(bytes: $Bytes) -> Self {
          from_bytes(bytes)
        }

        #[inline]
        pub const fn to_bytes(self) -> $Bytes {
          to_bytes(self)
        }

        #[inline]
        pub const fn swap_bytes(self) -> Self {
          swap_bytes(self)
        }

        #[inline]
        pub const fn from_ne_bytes(bytes: $Bytes) -> Self {
          from_ne_bytes(bytes)
        }

        #[inline]
        pub const fn from_le_bytes(bytes: $Bytes) -> Self {
          from_le_bytes(bytes)
        }

        #[inline]
        pub const fn from_be_bytes(bytes: $Bytes) -> Self {
          from_be_bytes(bytes)
        }

        #[inline]
        pub const fn to_ne_bytes(self) -> $Bytes {
          to_ne_bytes(self)
        }

        #[inline]
        pub const fn to_le_bytes(self) -> $Bytes {
          to_le_bytes(self)
        }

        #[inline]
        pub const fn to_be_bytes(self) -> $Bytes {
          to_be_bytes(self)
        }

        #[inline]
        pub const fn to_ne(self) -> Self {
          to_ne(self)
        }

        #[inline]
        pub const fn to_le(self) -> Self {
          to_le(self)
        }

        #[inline]
        pub const fn to_be(self) -> Self {
          to_be(self)
        }

        #[inline]
        pub const fn count_zeros(self) -> u32 {
          count_zeros(self)
        }

        #[inline]
        pub const fn count_ones(self) -> u32 {
          count_ones(self)
        }

        #[inline]
        pub const fn leading_zeros(self) -> u32 {
          leading_zeros(self)
        }

        #[inline]
        pub const fn trailing_zeros(self) -> u32 {
          trailing_zeros(self)
        }

        #[inline]
        pub const fn leading_ones(self) -> u32 {
          leading_ones(self)
        }

        #[inline]
        pub const fn trailing_ones(self) -> u32 {
          trailing_ones(self)
        }

        #[inline]
        pub const fn rotate_left(self, count: u32) -> Self {
          rotate_left(self, count)
        }

        #[inline]
        pub const fn rotate_right(self, count: u32) -> Self {
          rotate_right(self, count)
        }

        #[inline]
        pub const fn cmp(self, other: Self) -> Ordering {
          cmp(self, other)
        }
      }

      impl <$($generics)*> From<$Raw> for $T {
        #[inline]
        fn from(raw: $Raw) -> Self {
          cast(raw)
        }
      }

      impl <$($generics)*> Debug for $T where $Raw: Debug {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Debug>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Display for $T where $Raw: Display {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Display>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Binary for $T where $Raw: Binary {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Binary>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Octal for $T where $Raw: Octal {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Octal>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> LowerHex for $T where $Raw: LowerHex {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as LowerHex>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> UpperHex for $T where $Raw: UpperHex {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as UpperHex>::fmt(&cast(*self), f)
        }
      }

      impl <Rhs: Numeric, $($generics)*> Add<Rhs> for $T where $Raw: Add, <$Raw as Add>::Output: Numeric {
        type Output = Self;

        #[inline]
        fn add(self, rhs: Rhs) -> Self {
          cast(cast::<_, $Raw>(self) + cast::<_, $Raw>(rhs))
        }
      }
    };
  }

  adt_nums! {
    u24 3 4 u32 u40 5 8 u64 u48 6 8 u64 u56 7 8 u64
    u72 9 16 u128 u80 10 16 u128 u88 11 16 u128 u96 12 16 u128 u104 13 16 u128 u112 14 16 u128 u120 15 16 u128
  }

  impl_num! {
    u8 1 u16 2 u24 3 u32 4 u40 5 u48 6 u56 7 u64 8
    u72 9 u80 10 u88 11 u96 12 u104 13 u112 14 u120 15 u128 16
  }

  impl_adt! { (T: Native) Le<T>[T] (<T as Numeric>::Bytes) }
  impl_adt! { (T: Native) Be<T>[T] (<T as Numeric>::Bytes) }
}

// -------------------------------------------------------------------------------------------------
// ptr

pub mod ptr {
  use core::mem;

  use crate::num::Numeric;
  use crate::Default;

  /// Provides the pointer metadata type of any pointed-to type.
  ///
  /// # Pointer metadata
  ///
  /// Raw pointer types and reference types in Rust can be thought of as made of two parts:
  /// a data pointer that contains the memory address of the value, and some metadata.
  ///
  /// For statically-sized types (that implement the `Sized` traits)
  /// as well as for `extern` types,
  /// pointers are said to be “thin”: metadata is zero-sized and its type is `()`.
  ///
  /// Pointers to [dynamically-sized types][dst] are said to be “wide” or “fat”,
  /// they have non-zero-sized metadata:
  ///
  /// * For structs whose last field is a DST, metadata is the metadata for the last field
  /// * For slice types like `[T]`, metadata is the length in items as `usize`
  ///
  /// In the future, the Rust language may gain new kinds of types
  /// that have different pointer metadata.
  ///
  /// [dst]: https://doc.rust-lang.org/nomicon/exotic-sizes.html#dynamically-sized-types-dsts
  ///
  ///
  /// # The `Pointee` trait
  ///
  /// The point of this trait is its `Metadata` associated type,
  /// which is `()` or `usize` as described above.
  ///
  ///
  /// # Usage
  ///
  /// Raw pointers can be decomposed into the data pointer and metadata components.
  ///
  /// Alternatively, metadata alone can be extracted with the [`metadata`] function.
  /// A reference can be passed to [`metadata`] and implicitly coerced.
  ///
  /// A (possibly-wide) pointer can be put back together from its data pointer and metadata
  /// with [`from_raw_parts`] or [`from_raw_parts_mut`].
  pub unsafe trait Pointee {
    /// The type for metadata in pointers and references to `Self`.
    type Metadata: Numeric;
  }

  /// Extract the metadata component of a pointer.
  #[inline]
  pub const fn metadata<T: Pointee + ?Sized>(ptr: *const T) -> T::Metadata {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline]
  pub const fn from_raw_parts<T: Pointee + ?Sized>(
    address: *const (), metadata: T::Metadata,
  ) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline]
  pub const fn from_raw_parts_mut<T: Pointee + ?Sized>(
    address: *mut (), metadata: T::Metadata,
  ) -> *mut T {
    unsafe { PtrRepr::with_components(address, metadata).mut_ptr }
  }

  /// Extract the address component of a non-dangling pointer.
  #[inline]
  pub const fn addr_of<T: ?Sized>(ptr: *const T) -> usize {
    // Compute `ptr` raw pointer `ptr`.
    let ptr = ptr.cast::<()>();
    // SAFETY: This no-op trick ensure that `ptr` has a provenance:
    // This will result in a compile-error if `ptr` is a dangling pointer (has no provenance).
    #[cfg(not(miri))] // Disable for Miri as it throw out-of-bounds pointer arithmetic error.
    let _ = unsafe { core::ptr::addr_of!((*(ptr as *const (u8, ()))).1) };
    // SAFETY: This is just an inlined `p.addr()` (which is not a `const fn` so we cannot call it).
    // - This is safe at runtime as `ptr` point to a valid non-dangling reference, hence the pointer
    //   has a provenance.
    // - During const eval, it is unsound to perform this transmutation if the pointer has no
    //   provenance; the above trick ensure just that.
    #[allow(clippy::transmutes_expressible_as_ptr_casts)]
    unsafe {
      mem::transmute(ptr)
    }
  }

  #[repr(C)]
  union PtrRepr<T: Pointee + ?Sized> {
    const_ptr: *const T,
    mut_ptr: *mut T,
    components: PtrComponents<T::Metadata>,
  }

  #[repr(C)]
  #[derive(Copy, Clone)]
  struct PtrComponents<T> {
    address: *const (),
    metadata: T,
  }

  // impl
  const _: () = {
    impl<T: Pointee + ?Sized> PtrRepr<T> {
      #[inline]
      const fn empty() -> Self {
        Self::empty_with(core::ptr::null())
      }

      #[inline]
      const fn empty_with(address: *const ()) -> Self {
        Self::with_components(address, T::Metadata::DEFAULT)
      }

      #[inline]
      const fn with_components(address: *const (), metadata: T::Metadata) -> Self {
        Self { components: PtrComponents { address, metadata } }
      }

      #[inline]
      const fn set_const(mut self, const_ptr: *const T) -> Self {
        self.const_ptr = const_ptr;
        self
      }
    }

    unsafe impl<T> Pointee for [T] {
      type Metadata = usize;
    }

    unsafe impl<T, const N: usize> Pointee for [T; N] {
      type Metadata = ();
    }
  };
}

// -------------------------------------------------------------------------------------------------
// layout

pub mod layout {
  use crate::num::LiteralUint;
  use crate::{
    endian::Endianness,
    layout, num,
    num::{Native, NativeOf, Numeric},
    ptr, Default, Literal, Type,
  };
  use core::marker::PhantomData;
  use core::num::NonZero;

  #[derive(Copy, Clone)]
  pub struct ByteInterval {
    start: u8,
    end: u8,
  }

  // impl `ByteInterval`
  const _: () = {
    use core::fmt;

    impl ByteInterval {
      pub const ANY: Self = Self { start: 0, end: 0xff };
      pub const EMPTY: Self = Self { start: 0xff, end: 0 };

      pub(super) const fn new(start: u8, end: u8) -> Self {
        assert!(start <= end, "end lower that start");
        Self { start, end }
      }

      pub(super) const fn eq(val: u8) -> Self {
        Self::new(val, val)
      }

      pub(crate) const fn len(&self) -> usize {
        if self.start > self.end {
          return 0;
        };
        1usize + (self.end - self.start) as usize
      }

      pub(crate) const fn contains(&self, byte: u8) -> bool {
        self.start <= byte && byte <= self.end
      }

      pub(crate) const fn intersect(&self, o: &Self) -> usize {
        macro_rules! l {
          ($v:expr) => {
            if $v <= 127 {
              1u128 << $v
            } else {
              0u128
            }
          };
        }
        macro_rules! h {
          ($v:expr) => {
            if $v <= 127 {
              0u128
            } else {
              1u128 << ($v - 128)
            }
          };
        }

        // `(a, b)` and `(c, d)` are the byte intervals.
        let ((mut a, b), (mut c, d)) = ((self.start, self.end), (o.start, o.end));

        // Compute low and high bits for an u256 bitset covering the `self` byte intervals.
        let mut e = (l!(b), h!(b));
        while a < b {
          e.0 |= l!(a);
          e.1 |= h!(a);
          a += 1;
        }

        // Compute low and high bits for an u256 bitset covering the `o` byte intervals.
        let mut f = (l!(d), h!(d));
        while c < d {
          f.0 |= l!(c);
          f.1 |= h!(c);
          c += 1;
        }

        // Return the number of intersecting values.
        (e.0 & f.0).count_ones() as usize + (e.1 & f.1).count_ones() as usize
      }
    }

    impl fmt::Debug for ByteInterval {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.len() == 0 {
          return write!(f, "∅");
        }
        if self.start == self.end {
          return write!(f, "{:#02x}", self.start);
        }
        if self.start != 0 {
          write!(f, "{:#02x}", self.start)?;
        }
        write!(f, "..")?;
        if self.end != 0xff {
          write!(f, "={:#02x}", self.end)?;
        }
        Ok(())
      }
    }
  };

  #[derive(Debug, Copy, Clone)]
  pub enum BitsPattern {
    Uninit,
    Init(&'static [ByteInterval]),
    Any(&'static [BitsPattern]),
  }

  // impl `BitsPattern`
  const _: () = {
    impl BitsPattern {
      const fn reachable(&self) -> bool {
        match self {
          BitsPattern::Uninit => true,
          BitsPattern::Init(x) => {
            let mut i = 0;
            while i < x.len() && x[i].len() > 0 {
              i += 1
            }
            i == x.len()
          }
          BitsPattern::Any(_) => true,
        }
      }

      #[cfg(test)]
      pub fn matches<T: Copy>(&self, memory: T) -> bool {
        use BitsPattern::*;
        match self {
          Uninit => true,
          Any(patterns) => patterns.iter().any(|p| p.matches(memory)),
          Init(intervals) => {
            let memory = unsafe {
              core::slice::from_raw_parts(core::ptr::from_ref(&memory).cast::<u8>(), size_of::<T>())
            };
            intervals.iter().zip(memory).all(|(i, m)| i.contains(*m))
          }
        }
      }
    }
  };

  pub enum Operator<T> {
    True,
    Equ(T),
    RangeFrom(T),
    RangeTo(T),
    Range(T, T),
    Or,
  }

  pub trait Satisfy<T> {
    type Lhs: Satisfy<T>;
    type Rhs: Satisfy<T>;
    const OP: Operator<T>;
  }

  #[derive(Copy, Clone)]
  pub struct Equ<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct RangeFrom<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct RangeTo<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct Range<T, Start, End>(PhantomData<(T, Start, End)>);

  #[derive(Copy, Clone)]
  pub struct Or<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

  #[inline]
  pub const fn is_true<O: Copy, T: Satisfy<O>>() -> bool {
    matches!(T::OP, Operator::True)
  }

  #[inline]
  pub const fn eval<O: Native, T: Satisfy<O>>(val: O) -> bool {
    match T::OP {
      Operator::True => true,
      Operator::RangeFrom(start) => num::is_ge(val, start),
      Operator::RangeTo(end) => num::is_le(val, end),
      Operator::Range(start, end) => num::is_ge(val, start) && num::is_le(val, end),
      Operator::Equ(rhs) => num::is_eq(val, rhs),
      Operator::Or => eval::<O, T::Lhs>(val) || eval::<O, T::Rhs>(val),
    }
  }

  // impl `Satisfy`
  const _: () = {
    impl<O> Satisfy<O> for () {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<O> = Operator::True;
    }

    impl<T, Val: Literal<T>> Satisfy<T> for Equ<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::Equ(Val::LITERAL);
    }

    impl<T, Val: Literal<T>> Satisfy<T> for RangeFrom<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::RangeFrom(Val::LITERAL);
    }

    impl<T, Val: Literal<T>> Satisfy<T> for RangeTo<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::RangeTo(Val::LITERAL);
    }

    impl<T: Native, Start: Literal<T>, End: Literal<T>> Satisfy<T> for Range<T, Start, End> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = {
        assert!(num::is_le(Start::LITERAL, End::LITERAL));
        Operator::Range(Start::LITERAL, End::LITERAL)
      };
    }

    impl<O, Lhs: Satisfy<O>, Rhs: Satisfy<O>> Satisfy<O> for Or<Lhs, Rhs> {
      type Lhs = Lhs;
      type Rhs = Rhs;
      const OP: Operator<O> = Operator::Or;
    }
  };

  pub trait Raw: 'static + Copy + Default {
    type Repr: Numeric;
    type Satisfy: Satisfy<NativeOf<Self::Repr>>;
    const RAW_DEFAULT: Self;
  }

  #[inline]
  pub const fn size_of_term<T: Raw>() -> usize {
    const { size_of::<T::Repr>() }
  }

  #[inline]
  pub const unsafe fn satisfy_term<F: Raw>(ptr: *const u8) -> bool {
    if const { is_true::<NativeOf<F::Repr>, F::Satisfy>() } {
      return true;
    }
    let ptr = ptr.cast::<F::Repr>();
    let val = num::cast(unsafe { ptr.read_unaligned() });
    eval::<NativeOf<F::Repr>, F::Satisfy>(val)
  }

  // impl `Raw`
  const _: () = {
    unsafe impl<T: Raw> ptr::Pointee for T {
      type Metadata = ();
    }

    impl<T: Numeric> Raw for T {
      type Repr = T;
      type Satisfy = ();
      const RAW_DEFAULT: Self = T::ZERO;
    }

    impl<T: Numeric, const VALUE: u128> Raw for LiteralUint<T, VALUE> {
      type Repr = T;
      type Satisfy = Equ<T::Native, LiteralUint<T::Native, VALUE>>;
      const RAW_DEFAULT: Self = Self::new();
    }

    impl Raw for bool {
      type Repr = u8;
      type Satisfy = RangeTo<u8, LiteralUint<u8, 1>>;
      const RAW_DEFAULT: Self = false;
    }

    impl Raw for char {
      type Repr = u32;
      type Satisfy = Or<
        RangeTo<u32, LiteralUint<u32, { 0xD800 - 1 }>>,
        Range<u32, LiteralUint<u32, { 0xDFFF + 1 }>, LiteralUint<u32, { 0x110000 - 1 }>>,
      >;
      const RAW_DEFAULT: Self = '\0';
    }

    macro_rules! impl_non_zero {
      { $($T:ident)+ } => {$(
        impl Raw for NonZero<$T> {
          type Repr = $T;
          type Satisfy = RangeFrom<$T, LiteralUint<$T, 1>>;
          const RAW_DEFAULT: Self = match NonZero::new(1) {
            Some(val) => val,
            _ => unreachable!()
          };
        }

        impl Raw for Option<NonZero<$T>> {
          type Repr = $T;
          type Satisfy = ();
          const RAW_DEFAULT: Self = None;
        }
      )+};
    }

    impl_non_zero! { u8 u16 u32 u64 u128 }
  };

  #[repr(transparent)]
  pub struct Sat<T, F>(T, PhantomData<F>);

  // impl `Sat`
  const _: () = {
    impl<T: Numeric, F: Satisfy<T::Native>> Copy for Sat<T, F> {}

    impl<T: Numeric, F: Satisfy<T::Native>> Clone for Sat<T, F> {
      fn clone(&self) -> Self {
        *self
      }
    }

    impl<T: Numeric, F: Satisfy<T::Native> + 'static> Raw for Sat<T, F> {
      type Repr = T;
      type Satisfy = F;
      const RAW_DEFAULT: Self = Sat(<T as Default>::DEFAULT, PhantomData);
    }
  };

  pub enum LayoutKind {
    Raw,
    And(usize, usize),
    Repeat(usize, usize),
    Wildcard(usize),
    #[cfg(test)]
    Uninit,
  }

  pub trait Layout {
    type Lhs: Layout;
    type Rhs: Layout;
    type Raw: Raw;
    const KIND: LayoutKind;
  }

  #[inline]
  pub const unsafe fn satisfy<T: Layout>(ptr: *const u8, metadata: usize) -> bool {
    use LayoutKind::*;
    match const { T::KIND } {
      Raw => satisfy_term::<T::Raw>(ptr),
      And(l, r) => {
        satisfy::<T::Lhs>(ptr.wrapping_add(l), metadata)
          && satisfy::<T::Rhs>(ptr.wrapping_add(r), metadata)
      }
      Repeat(_, 0) | Wildcard(0) => true,
      Repeat(_, s) | Wildcard(s) => {
        let (mut i, n) = (0, if let Repeat(n, _) = const { T::KIND } { n } else { metadata });
        while i < n {
          if !satisfy::<T::Lhs>(ptr.wrapping_add(i * s), 0) {
            return false;
          }
          i += 1;
        }
        true
      }
      #[cfg(test)]
      Uninit => true,
    }
  }

  #[inline]
  pub const fn wildcard_size<T: Layout>() -> usize {
    use LayoutKind::*;
    const {
      match T::KIND {
        Raw | Repeat(_, _) => 0,
        And(_, _) => wildcard_size::<T::Lhs>() + wildcard_size::<T::Rhs>(),
        Wildcard(s) => s,
        #[cfg(test)]
        Uninit => 0,
      }
    }
  }

  #[inline]
  pub const unsafe fn initialize_to_default<T: Layout>(ptr: *mut u8) {
    use LayoutKind::*;
    match const { T::KIND } {
      Raw => *ptr.cast() = T::Raw::RAW_DEFAULT,
      And(l, r) => {
        initialize_to_default::<T::Lhs>(ptr.wrapping_add(l));
        initialize_to_default::<T::Rhs>(ptr.wrapping_add(r));
      }
      Repeat(n, s) => {
        let mut i = 0;
        while i < n {
          initialize_to_default::<T::Lhs>(ptr.wrapping_add(i * s));
          i += 1;
        }
      }
      Wildcard(_) => {}
      #[cfg(test)]
      Uninit => {}
    }
  }

  /// Compile an inclusive range from `start` to `end`, where `start` and `end` are numbers
  /// encoded in big-endian. `start` bound must be greater or equals than `end` bound, this will
  /// not be checked within this function.
  ///
  /// # Parameters
  /// - `start`: Start bound encoded in big-endian.
  /// - `end`: End bound encoded in big-endian.
  /// - `lsb`: Whatever the returned sub-ranges should be in little-endian, big-endian otherwise.
  ///
  /// # Returns
  /// All the possible byte sub-ranges of the input range.
  /// Empty ranges are set to `[ByteInterval::EMPTY; N]`.
  const fn compile_range<const N: usize, const M: usize>(
    start: &[u8; N], end: &[u8; N], lsb: bool,
  ) -> [[ByteInterval; N]; M] {
    const fn offset<const N: usize>(off: usize, lsb: bool) -> usize {
      if lsb {
        N - off - 1
      } else {
        off
      }
    }

    const fn range<const N: usize>(off: usize, fixed: &[u8; N], lsb: bool) -> [ByteInterval; N] {
      let mut sub = [ByteInterval::ANY; N];
      // Fill empty sub-range with fixed bytes.
      let mut idx = 0;
      while idx < off {
        sub[offset::<N>(idx, lsb)] = ByteInterval { start: fixed[idx], end: fixed[idx] };
        idx += 1;
      }
      sub
    }

    const fn all<const N: usize>(mut idx: usize, val: u8, bytes: &[u8; N]) -> bool {
      while idx < N && bytes[idx] == val {
        idx += 1;
      }
      idx == N
    }

    let mut res = [[ByteInterval::EMPTY; N]; M];
    let mut res_count = 0;

    // Compute start offset.
    let mut off = 0;
    while off < N && start[off] == end[off] {
      off += 1;
    }

    // All bytes are equals.
    if off + 1 >= N {
      res[0] = range::<N>(N, start, lsb);
      if off < N {
        res[0][offset::<N>(N - 1, lsb)].end = end[N - 1];
      }
      return res;
    }

    let middle_end = if all::<N>(off, 0xff, end) { 0xff } else { end[off] - 1 };
    let middle_start = if all::<N>(off, 0x00, start) {
      0x00
    } else {
      // First sub-range:
      let mut sub = range::<N>(N, start, lsb);
      sub[offset::<N>(N - 1, lsb)].end = 0xff;
      res[res_count] = sub;
      res_count += 1;

      // Start bound middle ranges:
      let mut idx = off + 1;
      while idx < N - 1 {
        sub = range::<N>(idx, start, lsb);
        if let Some(s) = start[idx].checked_add(1) {
          sub[offset::<N>(idx, lsb)].start = s;
          res[res_count] = sub;
          res_count += 1;
        };
        idx += 1;
      }

      start[off] + 1
    };

    // Middle range:
    let mut sub = range::<N>(off, start, lsb);
    sub[offset::<N>(off, lsb)].start = middle_start;
    sub[offset::<N>(off, lsb)].end = middle_end;
    res[res_count] = sub;
    res_count += 1;

    if middle_end != 0xff {
      // End bound middle ranges:
      let mut idx = off + 1;
      while idx < N - 1 {
        sub = range::<N>(idx, end, lsb);
        if let Some(s) = end[idx].checked_sub(1) {
          sub[offset::<N>(idx, lsb)].end = s;
          res[res_count] = sub;
          res_count += 1;
        };
        idx += 1;
      }

      // Last sub-range:
      sub = range::<N>(N, end, lsb);
      sub[offset::<N>(N - 1, lsb)].start = 0x00;
      res[res_count] = sub;
    }

    res
  }

  /// Compile `T` term type into `BitsPattern`
  #[inline]
  const fn compile<T: Raw, const N: usize, const M: usize>() -> BitsPattern {
    const fn compile_impl<
      O: Native,
      T: Satisfy<O>,
      E: Endianness,
      const N: usize,
      const M: usize,
    >() -> BitsPattern {
      /// Compile both `lhs` and `rhs` sides of the expression.
      /// NOTE: Function indirection is required as it avoid a "cycle detected" compilation error.
      const fn operands<O: Native, T: Satisfy<O>, E: Endianness, const N: usize, const M: usize>(
      ) -> &'static [BitsPattern] {
        &const { [compile_impl::<O, T::Lhs, E, N, M>(), compile_impl::<O, T::Rhs, E, N, M>()] }
      }

      use BitsPattern::*;
      use Operator::*;

      match T::OP {
        True => Init(&[ByteInterval::ANY; N]),
        Or => Any(operands::<O, T, E, N, M>()),
        Equ(_) => Init(
          &const {
            let val = if let Equ(x) = T::OP { x } else { num::cast(0u8) };
            let bytes = match E::LITERAL.is_little() {
              true => num::to_le_bytes(val),
              _ => num::to_be_bytes(val),
            };
            // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
            let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
            let mut intervals = [ByteInterval::ANY; N];
            let mut i = 0;
            while i < N {
              intervals[i] = ByteInterval::eq(bytes[i]);
              i += 1;
            }
            intervals
          },
        ),
        RangeFrom(_) | RangeTo(_) | Range(_, _) => {
          let patterns = &const {
            let intervals = &const {
              use core::mem::transmute;
              use num::{cast, to_be_bytes, Maximum, Minimum};
              let (s, e) = match T::OP {
                RangeFrom(s) => (s, <O as Maximum>::MAX),
                RangeTo(e) => (<O as Minimum>::MIN, e),
                Range(s, e) => (s, e),
                _ => (cast(()), cast(())),
              };
              let (s, e) = (to_be_bytes(s), to_be_bytes(e));
              // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
              let (s, e): (&[u8; N], &[u8; N]) = unsafe { (transmute(&s), transmute(&e)) };
              compile_range::<N, M>(s, e, E::LITERAL.is_little())
            };
            let mut patterns = [Uninit; M];
            let mut i = 0;
            while i < M && Init(&intervals[i]).reachable() {
              patterns[i] = Init(&intervals[i]);
              i += 1;
            }
            patterns
          };
          let mut i = 0;
          while i < M && !matches!(patterns[i], Uninit) {
            i += 1
          }
          Any(patterns.split_at(i).0)
        }
      }
    }

    const { compile_impl::<NativeOf<T::Repr>, T::Satisfy, <T::Repr as Numeric>::Endian, N, M>() }
  }

  pub(crate) const fn offset<T: Layout>(off: usize) -> &'static BitsPattern {
    use LayoutKind::*;
    match T::KIND {
      #[cfg(test)]
      Uninit => &BitsPattern::Uninit,
      Raw if off == 0 => {
        &const {
          macro_rules! compile_dispatch {
            ($($size:literal)+) => {
              match size_of_term::<T::Raw>() {
                0 => BitsPattern::Init(&[]),
                $($size => compile::<T::Raw, $size, { 2 * $size - 1 }>(),)+
                _ => unreachable!(),
              }
            };
          }
          compile_dispatch!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16)
        }
      }
      Raw => &BitsPattern::Uninit,
      Repeat(n, s) if off < n * s => offset::<T::Lhs>(off % s),
      Repeat(_, _) | Wildcard(0) => &BitsPattern::Uninit,
      Wildcard(s) => offset::<T::Lhs>(off % s),
      And(l, r) => {
        let l = if off >= l { offset::<T::Lhs>(off - l) } else { &BitsPattern::Uninit };
        if matches!(l, &BitsPattern::Uninit) && off >= r {
          offset::<T::Rhs>(off - r)
        } else {
          l
        }
      }
    }
  }

  #[cfg(test)]
  pub struct Uninit;

  #[cfg(test)]
  impl Layout for Uninit {
    type Lhs = ();
    type Rhs = ();
    type Raw = ();
    const KIND: LayoutKind = LayoutKind::Uninit;
  }

  // impl `Layout`
  const _: () = {
    impl<T: Raw> Layout for T {
      type Lhs = T;
      type Rhs = T;
      type Raw = T;
      const KIND: LayoutKind = LayoutKind::Raw;
    }

    impl<_0Off: Literal<usize>, _0: Layout> Layout for ((_0Off, _0),) {
      type Lhs = _0;
      type Rhs = ();
      type Raw = ();
      const KIND: LayoutKind = LayoutKind::And(_0Off::LITERAL, 0);
    }

    impl<_0Off: Literal<usize>, _0: Layout, _1Off: Literal<usize>, _1: Layout> Layout
      for ((_0Off, _0), (_1Off, _1))
    {
      type Lhs = _0;
      type Rhs = _1;
      type Raw = ();
      const KIND: LayoutKind = LayoutKind::And(_0Off::LITERAL, _1Off::LITERAL);
    }

    macro_rules! impl_and {
      // Match at least one pair
      { ($TOff:ident $T:ident) ($ROff:ident $R:ident) } => {};
      { ($TOff:ident $T:ident) $( ($ROff:ident $R:ident) )* } => {
        impl_and! { $( ($ROff $R) )* }
        impl<$TOff: Literal<usize>, $T: Layout, $($ROff: Literal<usize>, $R: Layout ),*>
          Layout for (($TOff, $T), $( ($ROff, $R) ),*)
        {
          type Lhs = $T;
          type Rhs = ($( ($ROff, $R) ),*);
          type Raw = ();
          const KIND: LayoutKind = LayoutKind::And($TOff::LITERAL, 0);
        }
      };
    }

    impl_and! {
      (_0Off _0) (_1Off _1) (_2Off _2) (_3Off _3)
      (_4Off _4) (_5Off _5) (_6Off _6) (_7Off _7)
      (_8Off _8) (_9Off _9) (_10Off _10) (_11Off _11)
      (_12Off _12) (_13Off _13) (_14Off _14) (_15Off _15)
      (_16Off _16) (_17Off _17) (_18Off _18) (_19Off _19)
      (_20Off _20) (_21Off _21) (_22Off _22) (_23Off _23)
      (_24Off _24) (_25Off _25) (_26Off _26) (_27Off _27)
      (_28Off _28) (_29Off _29) (_30Off _30) (_31Off _31)
    }
  };

  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub(crate) enum Intersection {
    Disjoint, // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
    Overlap,  // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
    Subset,   // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)
  }

  pub(crate) const fn intersect<A: Layout, B: Layout>(end: usize) -> Intersection {
    const ALL: u8 = 0b11;
    const SOME: u8 = 0b01;

    const fn step<A: Layout, B: Layout>(
      mut off: usize, mut a: BitsPattern, mut b: BitsPattern, end: &mut usize,
    ) -> u8 {
      use BitsPattern::*;
      let mut result = ALL;
      loop {
        match (a, b) {
          // terminal conditions:
          (Uninit, Uninit) => {
            break {
              *end = off + 1;
              result
            }
          }
          (Init([]), Init([])) => {
            break {
              *end = off;
              result
            }
          }
          // overlapping patterns:
          (Init([]), _) => a = *offset::<A>(off),
          (_, Init([])) => b = *offset::<B>(off),
          // uninitialized against initialized:
          (Init([_, na @ ..]), Uninit) => {
            off += 1;
            a = Init(na);
            b = *offset::<B>(off);
          }
          (Uninit, Init([_, rb @ ..])) => {
            off += 1;
            a = *offset::<A>(off);
            b = Init(rb);
          }
          (Any(a), _) => {
            break result & {
              let mut result = step::<A, B>(off, a[0], b, end);
              let mut i = 1;
              while i < a.len() {
                result = match (result, step::<A, B>(off, a[i], b, end)) {
                  (ALL, ALL) => ALL,
                  (0, 0) => 0,
                  _ => SOME,
                };
                i += 1;
              }
              result
            }
          }
          (_, Any(b)) => {
            break result & {
              let mut result = step::<A, B>(off, a, b[0], end);
              let mut i = 1;
              while i < b.len() {
                result |= step::<A, B>(off, a, b[i], end);
                i += 1;
              }
              result
            }
          }
          // both initialized:
          (Init([la, ra @ ..]), Init([lb, rb @ ..])) => match (la.len(), la.intersect(lb)) {
            (_, 0) => break 0,
            (l, r) => {
              result &= if l == r { ALL } else { SOME };
              off += 1;
              a = Init(ra);
              b = Init(rb);
            }
          },
        }
      }
    }

    let (mut off, mut result) = (0, ALL);
    while result != 0 && off < end {
      result &= step::<A, B>(off, *offset::<A>(off), *offset::<B>(off), &mut off);
    }

    match result {
      SOME => Intersection::Overlap,
      ALL => Intersection::Subset,
      _ => Intersection::Disjoint,
    }
  }
}

// -------------------------------------------------------------------------------------------------
// sizing

pub mod sizing {
  use crate::Literal;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Sized;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Unsized;

  #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
  pub enum MaybeUnsized {
    Sized,
    Unsized,
  }

  pub trait Sizing: Literal<MaybeUnsized> + sealed::Sizing {}

  mod sealed {
    pub trait Sizing {}
  }

  // impl
  const _: () = {
    use crate::Default;

    impl sealed::Sizing for Sized {}
    impl sealed::Sizing for Unsized {}

    impl Sizing for Sized {}
    impl Sizing for Unsized {}

    impl Literal<MaybeUnsized> for Sized {
      const LITERAL: MaybeUnsized = MaybeUnsized::Sized;
    }

    impl Literal<MaybeUnsized> for Unsized {
      const LITERAL: MaybeUnsized = MaybeUnsized::Unsized;
    }

    impl Default for MaybeUnsized {
      const DEFAULT: Self = Sized::LITERAL;
    }

    impl MaybeUnsized {
      #[inline]
      pub const fn is_sized(&self) -> bool {
        matches!(self, MaybeUnsized::Sized)
      }

      #[inline]
      pub const fn is_unsized(&self) -> bool {
        matches!(self, MaybeUnsized::Unsized)
      }
    }
  };
}

// -------------------------------------------------------------------------------------------------
// `offset_of`

/// A pointer to an aligned allocation of size 2^20.
/// Used to compute the offset of a field within a type using [`offset_of`].
#[doc(hidden)]
pub const BIG_ALLOCATION: *const () = {
  #[repr(C, align(1048576))]
  struct Big([u8; 1 << 20]);

  const REF: &Big = &Big([0; 1 << 20]);
  let ptr: *const Big = REF;
  ptr.cast()
};

/// Expands to the offset in bytes of a field from the beginning of the given type.
///
/// `offset_of!` produces code which is valid in a `const` context.
#[doc(hidden)]
#[macro_export]
macro_rules! offset_of {
  ($Container:ty, $($fields:tt).+) => {{
    use $crate::Default;
    let ptr = $crate::ptr::from_raw_parts::<$Container>(
      $crate::BIG_ALLOCATION,
      <$Container as $crate::ptr::Pointee>::Metadata::DEFAULT
    );
    // SAFETY:
    // `(*ptr)` pointer deference syntax is required for `addr_of!` to work
    // but the content of deference is never actually read.
    #[allow(unused_unsafe)]
    let field = unsafe { ::core::ptr::addr_of!((*ptr).$($fields).+) };
    // SAFETY:
    // - Both `ptr` and `field` are derived from the same allocated object.
    // - `addr_of!` ensure result provenance, hence `field` is in bounds of that
    //   allocated object.
    // - The distance, in bytes, between `ptr` and `field` is required to be
    //   a multiple of the size of `u8`, which is trivially true because
    //   `u8`'s size is 1.
    // - The distance, in bytes, cannot overflow `isize`. This is guaranteed
    //   because no allocated object can have a size larger than can fit in
    //   `isize`. [1]
    // - The distance being in-bounds cannot rely on wrapping around the
    //   address space. This is guaranteed because the same is guaranteed of
    //   allocated objects. [1]
    //
    // [1] TODO(https://github.com/rust-lang/rust/pull/116675):
    //     Once these are guaranteed in the Reference, cite it.
    #[allow(unused_unsafe)]
    let offset = unsafe { field.cast::<u8>().offset_from(ptr.cast::<u8>()) };
    // Guaranteed not to be lossy: `field` comes after `ptr`, so the offset
    // from `ptr` to `field` is guaranteed to be positive.
    assert!(offset >= 0);
    offset as usize
  }};
}

pub trait MinimallySized<S: sizing::Sizing> {
  const MINSIZE: usize;
}

// impl `MinimallySized`
const _: () = {
  impl<T> MinimallySized<sizing::Sized> for T {
    const MINSIZE: usize = size_of::<T>();
  }

  impl<T> MinimallySized<sizing::Unsized> for [T] {
    const MINSIZE: usize = 0;
  }
};

pub trait Type: MinimallySized<Self::Sizing> {
  type Sizing: sizing::Sizing;
  type Layout: layout::Layout;
}

#[inline]
pub const fn min_size_of<T: Type + ?Sized>() -> usize {
  <T as MinimallySized<T::Sizing>>::MINSIZE
}

#[inline]
pub const fn is_sized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_sized()
}

#[inline]
pub const fn is_unsized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_unsized()
}

#[inline]
const unsafe fn validate<T: Type + ?Sized>(ptr: *const (), metadata: usize) -> bool {
  layout::satisfy::<T::Layout>(ptr.cast(), metadata)
}

const fn default<T: Type + Copy>() -> T {
  const {
    let mut uninit = MaybeUninit::<T>::uninit();
    unsafe {
      layout::initialize_to_default::<T::Layout>(from_mut(&mut uninit).cast());
      transmute_unchecked(uninit)
    }
  }
}

#[cfg(test)]
#[derive(Debug, Copy, Clone)]
pub struct Uninit(u8);

#[cfg(test)]
impl Type for Uninit {
  type Sizing = sizing::Sized;
  type Layout = layout::Uninit;
}

#[cfg(test)]
unsafe impl ptr::Pointee for Uninit {
  type Metadata = ();
}

// impl `Type`
const _: () = {
  impl<T: Type + Copy> Default for T {
    const DEFAULT: Self = default::<T>();
  }

  impl<T: layout::Raw> Type for T {
    type Sizing = sizing::Sized;
    type Layout = T;
  }

  pub struct ArrayLayout<T, const N: usize>(PhantomData<T>);

  impl<T: Type, const N: usize> layout::Layout for ArrayLayout<T, N> {
    type Lhs = T::Layout;
    type Rhs = ();
    type Raw = ();
    const KIND: layout::LayoutKind = layout::LayoutKind::Repeat(N, size_of::<T>());
  }

  impl<T: Type, const N: usize> Type for [T; N] {
    type Sizing = sizing::Sized;
    type Layout = ArrayLayout<T, N>;
  }

  pub struct SliceLayout<T>(PhantomData<T>);

  impl<T: Type> layout::Layout for SliceLayout<T> {
    type Lhs = T::Layout;
    type Rhs = ();
    type Raw = ();
    const KIND: layout::LayoutKind = layout::LayoutKind::Wildcard(size_of::<T>());
  }

  impl<T: Type> Type for [T] {
    type Sizing = sizing::Unsized;
    type Layout = SliceLayout<T>;
  }
};

mod internal {
  use crate::layout::{intersect, wildcard_size, Intersection};
  use crate::num::{cast, Numeric};
  use crate::{layout, min_size_of, Default, Type};

  /// Unwrap a `Result` and return the error if it is `Err`.
  #[rustfmt::skip]
  macro_rules! bail {
    ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => return Err(e) });
    ($e:expr, $x:ident => $map:expr $(,)?) => (match $e { Ok(r) => r, Err($x) => return Err($map) });
  }

  /// Check a condition and return an error if it is `false`.
  #[rustfmt::skip]
  macro_rules! check { ($c:expr, $e:expr $(,)?) => { if (!$c) { return Err($e); } } }

  /// Internal type layout representation.
  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub struct Layout {
    min_size: usize,
    dst_slice_item_size: usize,
  }

  impl Layout {
    #[inline]
    const fn size_of_with<M: Numeric>(&self, metadata: M) -> usize {
      self.min_size + cast::<_, usize>(metadata) * self.dst_slice_item_size
    }
  }

  #[inline]
  pub const fn layout_of<T: ?Sized + Type>() -> Layout {
    const {
      let min_size = min_size_of::<T>();
      let dst_slice_item_size = wildcard_size::<T::Layout>();
      Layout { min_size, dst_slice_item_size }
    }
  }

  #[inline]
  pub const fn transmute_metadata<M: Numeric>(a: &Layout, b: &Layout, metadata: M) -> M {
    match b.dst_slice_item_size {
      0 => M::DEFAULT,
      n => cast((a.size_of_with(metadata) - b.min_size) / n),
    }
  }

  #[inline]
  pub const fn transmute_metadata_checked<M: Numeric>(
    a: &Layout, b: &Layout, metadata: M, checks: &Option<MetadataChecks>,
  ) -> Result<M, isize> {
    if let Some(checks) = checks {
      bail!(checks.validate(cast(metadata)), n => n * a.dst_slice_item_size as isize);
    }
    Ok(transmute_metadata(a, b, metadata))
  }

  /// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub(super) enum MetadataChecks {
    Is(usize),                             // Is equals to `n`
    IsAtLeast(usize),                      // Is at least `n`
    IsMultipleOf(usize),                   // Is a multiple of `m`.
    IsAtLeastThenMultipleOf(usize, usize), // Is at least `n` and a multiple of `m`.
    IsEmpty,                               // Implies no data, i.e. should be zero.
  }

  impl MetadataChecks {
    /// Validates `metadata` against constraints.
    /// In case of an error `Err(e)`, a positive `e` indicates an overflow of `metadata` by `e`,
    /// while a negative value signifies an underflow by `-e`.
    #[inline]
    pub(super) const fn validate(self, metadata: usize) -> Result<(), isize> {
      use MetadataChecks::*;
      match self {
        Is(n) if metadata == n => Ok(()),
        IsAtLeast(n) if metadata >= n => Ok(()),
        Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
        IsMultipleOf(m) if metadata % m == 0 => Ok(()),
        IsMultipleOf(m) => Err(m as isize - (metadata % m) as isize),
        IsAtLeastThenMultipleOf(n, m) if metadata >= n && (metadata - n) % m == 0 => Ok(()),
        IsAtLeastThenMultipleOf(n, _) if metadata < n => Err(n as isize - metadata as isize),
        IsAtLeastThenMultipleOf(_, m) => Err(m as isize - (metadata % m) as isize),
        IsEmpty if metadata == 0 => Ok(()),
        IsEmpty => Err(-(metadata as isize)),
      }
    }
  }

  /// Try to resolve `xa[ya] => xb[yb]`, where:
  /// - `a` and `b` are source and target respectively,
  /// - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
  /// - `ya` and `yb` are the DST slice item sizes of `a` and `b` respectively.
  pub const fn maybe_size_mismatch(
    a: &Layout, b: &Layout,
  ) -> Result<Option<MetadataChecks>, &'static str> {
    use MetadataChecks::*;
    // Where:
    // - `n` is the minimum number of the source DST slice items needed to align `a` and `b`,
    // - `m` is the source DST slice items multiplier.
    // Compute if source and target are DSTs, i.e. had a DST slice with a non-zero item size.
    let (src_is_dst, dst_is_dst) = (a.dst_slice_item_size != 0, b.dst_slice_item_size != 0);
    // Compute the minimum source DST slice items needed to align source and target.
    // 1. Source is smaller than target:
    //  - Check for source to be a DST.
    //  - Compute the missing bytes to fit the target.
    //  - In case the target isn't a DST, check if the missing bytes are a multiple of
    //    the source DST slice item size.
    //  - The minimum source DST slice items is the missing bytes ceil divided by the source DST
    //    slice item size.
    let mut n = if a.min_size < b.min_size {
      // Check for source to be a DST.
      check!(src_is_dst, "source too small to fit target");
      // Compute the missing bytes to fit the target.
      let missing = b.min_size - a.min_size;
      // In case the target isn't a DST:
      if !dst_is_dst {
        // Check if the missing bytes are a multiple of the source DST slice item size.
        check!(
          missing % a.dst_slice_item_size == 0,
          "missing bytes to fit the target is not a multiple of the source DST trailing slice \
           item size"
        );
      }
      // The minimum source DST slice items is the missing bytes ceil divided by the source DST
      // slice item size.
      (missing + a.dst_slice_item_size - 1) / a.dst_slice_item_size
    }
    // 2. Source is larger or equal to target:
    //  - The minimum source DST slice items is 0.
    else {
      // In case the target is a DST:
      // - Compute the remaining bytes to fit in the target.
      // - In case the source isn't a DST, check if the remaining bytes is a multiple
      //   of the target DST slice item size.
      if dst_is_dst {
        // Compute the remaining bytes to fit in the target.
        let remaining = a.min_size - b.min_size;
        // In case the source isn't a DST:
        if !src_is_dst {
          // Check if the remaining bytes is a multiple of the target DST slice item size.
          check!(
            remaining % b.dst_slice_item_size == 0,
            "remaining bytes to fit in the target is not a multiple of the target DST trailing \
             slice item size",
          );
        }
      }
      // The minimum source DST slice items is 0.
      0
    };
    // Compute the possible size mismatch between source and target.
    // 1. Source isn't a DST:
    let maybe_size_mismatch = if !src_is_dst {
      // No size mismatch.
      None
    }
    // 2. Source is a DST and target isn't a DST:
    else if !dst_is_dst {
      // In case the source is smaller than the target, the source metadata must be the minimum
      // source DST slice items count.
      // Otherwise, the source metadata must be empty.
      Some(if a.min_size < b.min_size { Is(n) } else { IsEmpty })
    }
    // 3. Source and target are DSTs:
    else {
      /// Greatest common divisor and least common multiple calculations to determine the alignment
      /// and size compatibility.
      const fn gcd(a: usize, b: usize) -> usize {
        match b {
          0 => a,
          _ => gcd(b, a % b),
        }
      }
      // Compute the GCD of the source and target DST trailing slice item sizes.
      let gcd = gcd(a.dst_slice_item_size, b.dst_slice_item_size);
      // Compute the absolute difference between the source and target DST minimum sizes.
      let absolute_miss =
        if a.min_size < b.min_size { b.min_size - a.min_size } else { a.min_size - b.min_size };
      // Make sure the source can catch up on the target.
      check!(absolute_miss % gcd == 0, "DST source cannot catch up on DST target");
      // Compute the minimum number of source DST slice items needed to align source and target.
      // We need to find `n` in a way that `xa + n*ya - yb` is a multiple of `yb`. Although this
      // could be resolved using complex equations, for the sake of readability, we can simplify it
      // by using the following loop:
      // - `n * ya` is the total size in bytes of the source DST slice for `n` source items.
      // - `xa + n * ya` is the total size in bytes of the source for `n` source items.
      // - `(xa + n * ya - xb)` is the total size of the target DST slice for `n` source items.
      // If the latter is a multiple of `yb`, then we found the minimum number of source DST slice
      // items needed to align source and target.
      while (a.min_size + n * a.dst_slice_item_size - b.min_size) % b.dst_slice_item_size != 0 {
        n += 1; // Increment the minimum number of source DST slice items by 1.
      }
      // Compute the metadata checks for the source DST slice.
      match (n, b.dst_slice_item_size / gcd) {
        // No size mismatch.
        (0, 1) => None,
        // Source metadata must be a multiple of the GCD.
        (0, m) => Some(IsMultipleOf(m)),
        // Source metadata must be at least `n` and a multiple of `m`.
        (n, 1) => Some(IsAtLeast(n)),
        // Source metadata must be at least `n` and a multiple of `m`.
        (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
      }
    };
    Ok(maybe_size_mismatch)
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub(super) struct Transmutation {
    pub(super) maybe_size_mismatch: Option<MetadataChecks>,
    pub(super) source_to_target_intersection: Intersection,
    pub(super) target_to_source_intersection: Intersection,
  }

  /// Try to resolve `Src => Dst` transmutation.
  pub(super) const fn transmutation<Src: ?Sized + Type, Dst: ?Sized + Type>(
  ) -> Result<Transmutation, &'static str> {
    const {
      let (a, b) = (&layout_of::<Src>(), &layout_of::<Dst>());
      // Compute the possible size mismatch from `Src` and `Dst` layouts.
      match maybe_size_mismatch(a, b) {
        Err(e) => Err(e),
        Ok(maybe_size_mismatch) => {
          use MetadataChecks::*;
          // Compute the minimum size in bytes need to fully intersect `Src` and `Dst`.
          let bound = match maybe_size_mismatch {
            None if a.dst_slice_item_size != 0 => a.min_size + a.dst_slice_item_size,
            None => b.min_size,
            Some(IsEmpty) => a.min_size,
            Some(IsMultipleOf(m)) => a.min_size + (m * a.dst_slice_item_size),
            Some(Is(n)) => a.min_size + (n * a.dst_slice_item_size),
            Some(IsAtLeast(n)) => a.min_size + (n * a.dst_slice_item_size) + a.dst_slice_item_size,
            Some(IsAtLeastThenMultipleOf(n, m)) => a.min_size + ((n + m) * a.dst_slice_item_size),
          };
          // Finalize by intersecting `Src` and `Dst` in both direction.
          let source_to_target_intersection = intersect::<Src::Layout, Dst::Layout>(bound);
          if let Intersection::Disjoint = source_to_target_intersection {
            Err("none of the bit patterns in source are valid in target")
          } else {
            Ok(Transmutation {
              maybe_size_mismatch,
              source_to_target_intersection,
              target_to_source_intersection: intersect::<Dst::Layout, Src::Layout>(bound),
            })
          }
        }
      }
    }
  }

  #[inline]
  pub(super) const fn convert_metadata<Src: ?Sized + Type, Dst: ?Sized + Type>(
    metadata: usize,
  ) -> usize {
    let (a, b) = const { (&layout_of::<Src>(), &layout_of::<Dst>()) };
    match b.dst_slice_item_size {
      0 => 0,
      n => ((a.min_size + metadata * a.dst_slice_item_size) - b.min_size) / n,
    }
  }

  #[cfg(test)]
  impl Transmutation {
    #[inline]
    const fn new(
      maybe_size_mismatch: Option<MetadataChecks>, source_to_target_intersection: Intersection,
      target_to_source_intersection: Intersection,
    ) -> Self {
      Self { maybe_size_mismatch, source_to_target_intersection, target_to_source_intersection }
    }

    #[inline]
    pub const fn unreachable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Disjoint, Intersection::Disjoint)
    }

    #[inline]
    pub const fn fallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Overlap)
    }

    #[inline]
    pub const fn fallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Subset)
    }

    #[inline]
    pub const fn infallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Subset, Intersection::Overlap)
    }

    #[cfg(test)]
    #[inline]
    pub const fn infallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Subset, Intersection::Subset)
    }
  }
}

// TODO:
// - derive
// - Pack!() pack!()
// - Lit!() lit!()
// - tests
// - functions
// - default
// - ...

#[test]
fn main() {
  use core::num::NonZero;
  use num::LiteralUint;
  use std::ptr::from_ref;

  #[derive(Debug, Copy, Clone)]
  struct Yolo {
    a: bool,
    b: u8,
    c: NonZero<u16>,
  }

  unsafe impl ptr::Pointee for Yolo {
    type Metadata = ();
  }

  impl Type for Yolo {
    type Sizing = <NonZero<u16> as Type>::Sizing;
    type Layout = (field!(a: bool), field!(b: u8), field!(c: NonZero<u16>));
  }

  impl MinimallySized<sizing::Unsized> for Yolo {
    const MINSIZE: usize = if is_unsized::<Self>() {
      offset_of!(Self, c) + min_size_of::<NonZero<u16>>()
    } else {
      panic!("use of `MINSIZE` with an invalid `Unsized` sizing")
    };
  }

  unsafe {
    println!("{}", layout::eval::<u16, layout::RangeFrom<u16, LiteralUint<u16, 0xabcd>>>(0xabcd));
    println!(
      "{}",
      layout::eval::<u16, layout::RangeFrom<u16, LiteralUint<u16, 0xabcd>>>(0xabcd - 1)
    );
    println!("{}", layout::eval::<u16, layout::RangeTo<u16, LiteralUint<u16, 0xabcd>>>(0xabcd));
    println!("{}", layout::eval::<u16, layout::RangeTo<u16, LiteralUint<u16, 0xabcd>>>(0xabcd + 1));

    println!("{}", validate::<[bool; 3]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 0));
    println!("{}", validate::<[bool; 3]>(from_ref(&[1u8, 0u8, 2u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>; 3]>(from_ref(&[1u8, 1u8, 1u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>; 3]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>]>(from_ref(&[1u8, 1u8, 1u8]).cast(), 3));
    println!("{}", validate::<[NonZero<u8>]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 3));
    println!(
      "{}",
      validate::<Yolo>(from_ref(&Yolo { a: false, b: 0, c: NonZero::new(1).unwrap() }).cast(), 3)
    );

    println!(
      "{:?}",
      layout::intersect::<<Yolo as Type>::Layout, <Yolo as Type>::Layout>(size_of::<Yolo>())
    );

    println!("{:?}", internal::transmutation::<u8, u8>());
    println!("{:?}", internal::transmutation::<bool, bool>());
    println!("{:?}", internal::transmutation::<u8, bool>());
    println!("{:?}", internal::transmutation::<bool, u8>());

    println!("{:?}", default::<Yolo>());
  }
}
