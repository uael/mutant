#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]

use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::num::NonZero;
use core::ptr::from_mut;

/// # mutant
///
///

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
  use core::ptr::from_ref;
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &*ptr::from_raw_parts(from_ref(val).cast(), T::LEN) }
}

pub const fn bytes_of_mut<T: ByteArray>(val: &mut T) -> &mut [u8] {
  use core::ptr::from_mut;
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &mut *ptr::from_raw_parts_mut(from_mut(val).cast(), T::LEN) }
}

#[inline(always)]
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
      #[inline(always)]
      pub const fn is_native(&self) -> bool {
        match self {
          Endian::Le => cfg!(target_endian = "little"),
          Endian::Be => cfg!(target_endian = "big"),
        }
      }

      #[inline(always)]
      pub const fn is_little(&self) -> bool {
        matches!(self, Endian::Le)
      }

      #[inline(always)]
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
      #[inline(always)]
      pub const fn is_signed(&self) -> bool {
        matches!(self, Sign::Signed)
      }

      #[inline(always)]
      pub const fn is_unsigned(&self) -> bool {
        matches!(self, Sign::Unsigned)
      }
    }
  };
}

// -------------------------------------------------------------------------------------------------
// num

pub mod num {
  use crate::{transmute_unchecked, ptr, Literal, ByteArray, Default, Ones, Zero, endian, endian::Endianness};
  use core::cmp::Ordering;
  use core::fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex};
  use core::ops::Add;

  pub use types::*;

  // -----------------------------------------------------------------------------------------------
  // UnsignedLiteral

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct LiteralUint<T: Integer, const VALUE: u128>(T);

  // impl `UnsignedLiteral`
  const _: () = {
    impl<T: Integer, const VALUE: u128> LiteralUint<T, VALUE> {
      #[inline(always)]
      const fn new() -> Self {
        Self(Self::LITERAL)
      }

      #[inline(always)]
      const fn into_inner(self) -> T {
        Self::LITERAL
      }
    }

    impl<T: Integer, const VALUE: u128> Literal<T> for LiteralUint<T, VALUE> {
      const LITERAL: T = match cast_checked(VALUE) {
        Some(x) => x,
        _ => panic!("literal `VALUE` is too large for type `T`."),
      };
    }

    impl<T: Integer, const VALUE: u128> Default for LiteralUint<T, VALUE> {
      const DEFAULT: Self = Self::new();
    }

    impl<T: Integer, const VALUE: u128> Minimum for LiteralUint<T, VALUE> {
      const MIN: Self = Self::new();
    }

    impl<T: Integer, const VALUE: u128> Maximum for LiteralUint<T, VALUE> {
      const MAX: Self = Self::new();
    }
  };

  // -----------------------------------------------------------------------------------------------
  // ...

  pub type NativeOf<T> = <T as Integer>::Native;

  pub unsafe trait Integer:
    Zero + Ones + Minimum + Maximum + Copy + 'static
  {
    type Bytes: ByteArray;
    type Native: Native<Bytes = Self::Bytes>;
    type Endian: Endianness;
  }

  pub trait Minimum: Sized {
    const MIN: Self;
  }

  pub trait Maximum: Sized {
    const MAX: Self;
  }

  pub trait Native: Integer {}

  impl<T: Integer<Endian = endian::Ne>> Native for T {}

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Le<T: Integer<Endian = endian::Ne>>(T);

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Be<T: Integer<Endian = endian::Ne>>(T);

  #[inline(always)]
  pub const fn swap_bytes<T: Integer + Default>(n: T) -> T {
    #[inline(always)]
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

  #[inline(always)]
  pub const fn from_bytes<T: Integer>(bytes: T::Bytes) -> T {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(bytes) }
  }

  #[inline(always)]
  pub const fn to_bytes<T: Integer>(n: T) -> T::Bytes {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(n) }
  }

  #[inline(always)]
  pub const fn from_ne_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn from_le_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn from_be_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::LITERAL.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn to_ne_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_native() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline(always)]
  pub const fn to_le_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_little() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline(always)]
  pub const fn to_be_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::LITERAL.is_big() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline(always)]
  pub const fn to_ne<T: Integer>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn to_be<T: Integer>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn to_le<T: Integer>(n: T) -> T {
    if const { !T::Endian::LITERAL.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline(always)]
  pub const fn cast<T: Integer, U: Integer>(n: T) -> U {
    union Cast<T: Copy, U: Copy> {
      src: T,
      dst: U,
    }

    let mut cast = Cast { dst: U::Bytes::ZERO };
    cast.src = to_le_bytes(n); // TODO: I'm sure about that ?
    from_le_bytes(unsafe { cast.dst })
  }

  #[inline(always)]
  pub const fn cast_checked<T: Integer, U: Integer>(n: T) -> Option<U> {
    let new_n = cast(n);
    match is_eq(cast(new_n), n) {
      true => Some(new_n),
      _ => None,
    }
  }

  #[inline(always)]
  pub const fn count_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).count_zeros()
  }

  #[inline(always)]
  pub const fn count_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).count_ones()
  }

  #[inline(always)]
  pub const fn leading_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).leading_zeros()
  }

  #[inline(always)]
  pub const fn trailing_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_zeros()
  }

  #[inline(always)]
  pub const fn leading_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).leading_ones()
  }

  #[inline(always)]
  pub const fn trailing_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_ones()
  }

  #[inline(always)]
  pub const fn rotate_left<T: Integer>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_left(count))
  }

  #[inline(always)]
  pub const fn rotate_right<T: Integer>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_right(count))
  }

  #[inline(always)]
  pub const fn not<T: Integer>(n: T) -> T {
    cast(!cast::<T, u128>(n))
  }

  #[inline(always)]
  pub const fn add<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) + cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn sub<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) - cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn mul<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) * cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn div<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) / cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn rem<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) % cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn shr<T: Integer>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) >> b)
  }

  #[inline(always)]
  pub const fn shl<T: Integer>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) << b)
  }

  #[inline(always)]
  pub const fn and<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) & cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn xor<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) ^ cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn or<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) | cast::<T, u128>(b))
  }

  #[inline(always)]
  pub const fn cmp<T: Integer>(a: T, b: T) -> Ordering {
    let (a, b): (u128, u128) = (cast(a), cast(b));
    if a < b {
      Ordering::Less
    } else if a > b {
      Ordering::Greater
    } else {
      Ordering::Equal
    }
  }

  #[inline(always)]
  pub const fn is_lt<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less)
  }

  #[inline(always)]
  pub const fn is_le<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Equal)
  }

  #[inline(always)]
  pub const fn is_gt<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater)
  }

  #[inline(always)]
  pub const fn is_ge<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater | Ordering::Equal)
  }

  #[inline(always)]
  pub const fn is_eq<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Equal)
  }

  #[inline(always)]
  pub const fn is_ne<T: Integer>(a: T, b: T) -> bool {
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

  impl<T: Integer> Zero for T {
    const ZERO: Self = unsafe { core::mem::zeroed() };
  }

  impl<T: Integer> Ones for T {
    const ONES: Self = not(Self::ZERO);
  }

  impl<T: Integer> Minimum for T {
    const MIN: Self = Self::ZERO;
  }

  impl<T: Integer> Maximum for T {
    const MAX: Self = Self::ONES;
  }

  unsafe impl<T: Integer> ptr::Pointee for T {
    type Metadata = ();
  }

  impl<T: Default> Literal<T> for () {
    const LITERAL: T = T::DEFAULT;
  }

  unsafe impl Integer for () {
    type Bytes = [u8; 0];
    type Native = Self;
    type Endian = endian::Ne;
  }

  unsafe impl Integer for usize {
    type Bytes = [u8; size_of::<Self>()];
    type Native = Self;
    type Endian = endian::Ne;
  }

  unsafe impl<T: Integer<Endian = endian::Ne>> Integer for Le<T> {
    type Bytes = T::Bytes;
    type Native = T;
    type Endian = endian::Le;
  }

  unsafe impl<T: Integer<Endian = endian::Ne>> Integer for Be<T> {
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
      unsafe impl Integer for $name {
        type Bytes = [u8; $size];
        type Native = Self;
        type Endian = endian::Ne;
      }
    )+};
  }

  macro_rules! impl_adt_from {
    { $T:ty[$Raw:ident] } => {
      impl From<$T> for $Raw {
        #[inline(always)]
        fn from(value: $T) -> Self {
          cast(value)
        }
      }
    };
  }

  macro_rules! impl_adt {
    (($($generics:tt)*) $T:ty[$Raw:ty] ($Bytes:ty)) => {
      impl <$($generics)*> $T {
        #[inline(always)]
        pub const fn from<U: Integer>(n: U) -> Self {
          cast(n)
        }

        #[inline(always)]
        pub const fn into<U: Integer>(self) -> U {
          cast(self)
        }

        #[inline(always)]
        pub const fn from_bytes(bytes: $Bytes) -> Self {
          from_bytes(bytes)
        }

        #[inline(always)]
        pub const fn to_bytes(self) -> $Bytes {
          to_bytes(self)
        }

        #[inline(always)]
        pub const fn swap_bytes(self) -> Self {
          swap_bytes(self)
        }

        #[inline(always)]
        pub const fn from_ne_bytes(bytes: $Bytes) -> Self {
          from_ne_bytes(bytes)
        }

        #[inline(always)]
        pub const fn from_le_bytes(bytes: $Bytes) -> Self {
          from_le_bytes(bytes)
        }

        #[inline(always)]
        pub const fn from_be_bytes(bytes: $Bytes) -> Self {
          from_be_bytes(bytes)
        }

        #[inline(always)]
        pub const fn to_ne_bytes(self) -> $Bytes {
          to_ne_bytes(self)
        }

        #[inline(always)]
        pub const fn to_le_bytes(self) -> $Bytes {
          to_le_bytes(self)
        }

        #[inline(always)]
        pub const fn to_be_bytes(self) -> $Bytes {
          to_be_bytes(self)
        }

        #[inline(always)]
        pub const fn to_ne(self) -> Self {
          to_ne(self)
        }

        #[inline(always)]
        pub const fn to_le(self) -> Self {
          to_le(self)
        }

        #[inline(always)]
        pub const fn to_be(self) -> Self {
          to_be(self)
        }

        #[inline(always)]
        pub const fn count_zeros(self) -> u32 {
          count_zeros(self)
        }

        #[inline(always)]
        pub const fn count_ones(self) -> u32 {
          count_ones(self)
        }

        #[inline(always)]
        pub const fn leading_zeros(self) -> u32 {
          leading_zeros(self)
        }

        #[inline(always)]
        pub const fn trailing_zeros(self) -> u32 {
          trailing_zeros(self)
        }

        #[inline(always)]
        pub const fn leading_ones(self) -> u32 {
          leading_ones(self)
        }

        #[inline(always)]
        pub const fn trailing_ones(self) -> u32 {
          trailing_ones(self)
        }

        #[inline(always)]
        pub const fn rotate_left(self, count: u32) -> Self {
          rotate_left(self, count)
        }

        #[inline(always)]
        pub const fn rotate_right(self, count: u32) -> Self {
          rotate_right(self, count)
        }

        #[inline(always)]
        pub const fn cmp(self, other: Self) -> Ordering {
          cmp(self, other)
        }
      }

      impl <$($generics)*> From<$Raw> for $T {
        #[inline(always)]
        fn from(raw: $Raw) -> Self {
          cast(raw)
        }
      }

      impl <$($generics)*> Debug for $T where $Raw: Debug {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Debug>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Display for $T where $Raw: Display {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Display>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Binary for $T where $Raw: Binary {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Binary>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> Octal for $T where $Raw: Octal {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Octal>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> LowerHex for $T where $Raw: LowerHex {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as LowerHex>::fmt(&cast(*self), f)
        }
      }

      impl <$($generics)*> UpperHex for $T where $Raw: UpperHex {
        #[inline(always)]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as UpperHex>::fmt(&cast(*self), f)
        }
      }

      impl <Rhs: Integer, $($generics)*> Add<Rhs> for $T where $Raw: Add, <$Raw as Add>::Output: Integer {
        type Output = Self;

        #[inline(always)]
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

  impl_adt! { (T: Integer<Endian = endian::Ne>) Le<T>[T] (<T as Integer>::Bytes) }
  impl_adt! { (T: Integer<Endian = endian::Ne>) Be<T>[T] (<T as Integer>::Bytes) }
}

// -------------------------------------------------------------------------------------------------
// ptr

pub mod ptr {
  use core::mem;

  use crate::num::Integer;
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
    type Metadata: Integer;
  }

  /// Extract the metadata component of a pointer.
  #[inline(always)]
  pub const fn metadata<T: Pointee + ?Sized>(ptr: *const T) -> T::Metadata {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline(always)]
  pub const fn from_raw_parts<T: Pointee + ?Sized>(
    address: *const (), metadata: T::Metadata,
  ) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline(always)]
  pub const fn from_raw_parts_mut<T: Pointee + ?Sized>(
    address: *mut (), metadata: T::Metadata,
  ) -> *mut T {
    unsafe { PtrRepr::with_components(address, metadata).mut_ptr }
  }

  /// Extract the address component of a non-dangling pointer.
  #[inline(always)]
  pub(crate) const fn addr_of<T: ?Sized>(ptr: *const T) -> usize {
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
      #[inline(always)]
      const fn empty() -> Self {
        Self::empty_with(core::ptr::null())
      }

      #[inline(always)]
      const fn empty_with(address: *const ()) -> Self {
        Self::with_components(address, T::Metadata::DEFAULT)
      }

      #[inline(always)]
      const fn with_components(address: *const (), metadata: T::Metadata) -> Self {
        Self { components: PtrComponents { address, metadata } }
      }

      #[inline(always)]
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
  use core::marker::PhantomData;
  use crate::{num, num::{NativeOf, Native, Integer}, endian::Endianness, Default, Literal};

  #[derive(Copy, Clone)]
  pub struct ByteInterval {
    mask: u8,
    start: u8,
    end: u8,
  }

  // impl `ByteInterval`
  const _: () = {
    use core::fmt;

    impl ByteInterval {
      pub const ANY: Self = Self { mask: 0xff, start: 0, end: 0xff };
      pub const EMPTY: Self = Self { mask: 0, start: 0, end: 0 };

      pub(super) const fn new(mask: u8, start: u8, end: u8) -> Self {
        assert!(start <= end, "end lower that start");
        assert!(mask.count_zeros() == mask.trailing_zeros() + mask.leading_zeros(), "not contiguous");
        Self { mask, start, end }
      }

      pub(super) const fn eq(mask: u8, val: u8) -> Self {
        Self::new(mask, val, val)
      }

      pub(crate) const fn len(&self) -> usize {
        1usize + (self.end - self.start) as usize
      }

      pub(crate) const fn contains(&self, byte: u8) -> bool {
        let byte = match self.mask {
          0x00 => return false,
          0xff => byte,
          mask if byte != 0 && byte & mask == 0 => return false,
          mask => (byte & mask) >> mask.trailing_zeros(),
        };
        self.start <= byte && byte <= self.end
      }

      pub(crate) const fn intersects(&self, o: &Self) -> usize {
        // If the masks don't intersect, the patterns don't intersect.
        if self.mask & o.mask == 0 {
          return 0;
        }

        macro_rules! l { ($v:expr) => (if $v <= 127 { 1u128 << $v } else { 0u128 }); }
        macro_rules! h { ($v:expr) => (if $v <= 127 { 0u128 } else { 1u128 << ($v - 128) }); }

        // `(a, b)` and `(c, d)` are the byte intervals, offset by `x` and `y` respectively.
        let (x, y) = (self.mask.trailing_zeros(), o.mask.trailing_zeros());
        let ((mut a, b), (mut c, d)) = ((self.start, self.end), (o.start, o.end));

        // Compute low and high bits for an u256 bitset covering the `self` byte intervals.
        let mut e = (l!(b << x), h!(b << x));
        while a < b { e.0 |= l!(a << x); e.1 |= h!(a << x); a += 1; }

        // Compute low and high bits for an u256 bitset covering the `o` byte intervals.
        let mut f = (l!(d << y), h!(d << y));
        while c < d { f.0 |= l!(c << y); f.1 |= h!(c << y); c += 1; }

        // Return the number of intersecting values.
        (e.0 & f.0).count_ones() as usize + (e.1 & f.1).count_ones() as usize
      }
    }

    impl core::fmt::Debug for ByteInterval {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start != 0 { write!(f, "{:#02x}", self.start)?; }
        write!(f, "..")?;
        if self.end != 0xff { write!(f, "={:#02x}", self.end)?; }
        Ok(())
      }
    }
  };

  #[derive(Debug, Copy, Clone)]
  pub enum BitsPattern {
    Uninit,
    Init(&'static [ByteInterval]),
    All(&'static [BitsPattern]),
    Any(&'static [BitsPattern]),
  }

  // impl `BitsPattern`
  const _: () = {
    impl BitsPattern {
      #[cfg(test)]
      pub fn matches<T: Copy>(&self, memory: T) -> bool {
        use BitsPattern::*;
        match self {
          Uninit => true,
          Overlap => unreachable!(),
          Any(patterns) => patterns.iter().any(|p| p.matches(memory)),
          All(patterns) => patterns.iter().all(|p| p.matches(memory)),
          Init(intervals) => {
            let memory = unsafe {
              core::slice::from_raw_parts(
                core::ptr::from_ref(&memory).cast::<u8>(),
                size_of::<T>()
              )
            };
            intervals.iter().zip(memory).all(|(i, m)| i.contains(*m))
          }
        }
      }
    }
  };

  pub enum Operator<T> {
    True, Equ(T), Gte(T), Lte(T), And, Or
  }

  pub trait Formula<T> {
    type Lhs: Formula<T>;
    type Rhs: Formula<T>;
    const OP: Operator<T>;
  }

  #[derive(Copy, Clone)]
  pub struct Equ<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct Gte<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct Lte<T, Val>(PhantomData<(T, Val)>);

  #[derive(Copy, Clone)]
  pub struct And<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

  #[derive(Copy, Clone)]
  pub struct Or<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

  #[inline(always)]
  pub const fn is_true<O: Copy, T: Formula<O>>() -> bool {
    matches!(T::OP, Operator::True)
  }

  #[inline(always)]
  pub const fn eval<O: Native, T: Formula<O>>(val: O) -> bool {
    match T::OP {
      Operator::True => true,
      Operator::Gte(rhs) => num::is_ge(val, rhs),
      Operator::Lte(rhs) => num::is_le(val, rhs),
      Operator::Equ(rhs) => num::is_eq(val, rhs),
      Operator::And => eval::<O, T::Lhs>(val) && eval::<O, T::Rhs>(val),
      Operator::Or => eval::<O, T::Lhs>(val) || eval::<O, T::Rhs>(val),
    }
  }

  // impl `Boolean`
  const _: () = {
    impl<O> Formula<O> for () {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<O> = Operator::True;
    }

    impl<T, Val: Literal<T>> Formula<T> for Equ<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::Equ(Val::LITERAL);
    }

    impl<T, Val: Literal<T>> Formula<T> for Gte<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::Gte(Val::LITERAL);
    }

    impl<T, Val: Literal<T>> Formula<T> for Lte<T, Val> {
      type Lhs = ();
      type Rhs = ();
      const OP: Operator<T> = Operator::Lte(Val::LITERAL);
    }

    impl<O, Lhs: Formula<O>, Rhs: Formula<O>> Formula<O> for And<Lhs, Rhs> {
      type Lhs = Lhs;
      type Rhs = Rhs;
      const OP: Operator<O> = Operator::And;
    }

    impl<O, Lhs: Formula<O>, Rhs: Formula<O>> Formula<O> for Or<Lhs, Rhs> {
      type Lhs = Lhs;
      type Rhs = Rhs;
      const OP: Operator<O> = Operator::Or;
    }
  };

  pub trait Term {
    type Input: Integer;
    type Satisfy: Formula<NativeOf<Self::Input>>;
    const BITS: Option<NativeOf<Self::Input>>;
    const DEFAULT: Self::Input;
  }

  #[inline(always)]
  pub const fn size_of_term<T: Term>() -> usize {
    const { size_of::<T::Input>() }
  }

  #[inline(always)]
  pub const unsafe fn satisfy_term<F: Term>(ptr: *const u8) -> bool {
    if const { is_true::<NativeOf<F::Input>, F::Satisfy>() } {
      return true;
    }
    let ptr = ptr.cast::<F::Input>();
    let val = num::cast(unsafe { ptr.read_unaligned() });
    eval::<NativeOf<F::Input>, F::Satisfy>(
      match F::BITS {
        None => val,
        Some(mask) => num::rotate_left(num::and(val, mask), num::leading_zeros(mask)),
      }
    )
  }

  // impl `Term`
  const _: () = {
    impl Term for () {
      type Input = ();
      type Satisfy = ();
      const BITS: Option<()> = None;
      const DEFAULT: Self::Input = ();
    }
  };

  pub enum LayoutKind {
    Term, And(usize, usize), Repeat(usize, usize), Wildcard(usize)
  }

  pub trait Layout {
    type Lhs: Layout;
    type Rhs: Layout;
    type Term: Term;
    const KIND: LayoutKind;
  }

  #[inline(always)]
  pub const unsafe fn satisfy<T: Layout>(ptr: *const u8, metadata: usize) -> bool {
    use LayoutKind::*;
    match const { T::KIND } {
      Term => satisfy_term::<T::Term>(ptr),
      And(l, r) =>
        satisfy::<T::Lhs>(ptr.wrapping_add(l), metadata)
          && satisfy::<T::Rhs>(ptr.wrapping_add(r), metadata),
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
    }
  }

  #[inline(always)]
  pub const fn wildcard_size<T: Layout>() -> usize {
    use LayoutKind::*;
    const {
      match T::KIND {
        Term | Repeat(_, _) => 0,
        And(_, _) => wildcard_size::<T::Lhs>() + wildcard_size::<T::Rhs>(),
        Wildcard(s) => s,
      }
    }
  }

  #[inline(always)]
  pub const unsafe fn initialize_to_default<T: Layout>(ptr: *mut u8) {
    use LayoutKind::*;
    match const { T::KIND } {
      Term => *ptr.cast() = T::Term::DEFAULT,
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
      },
      Wildcard(_) => {},
    }
  }

  /// Compile `T` term type into `BitsPattern`
  #[inline(always)]
  const fn compile<T: Term, const N: usize>() -> BitsPattern {
    const fn compile_impl<O: Native, T: Formula<O>, E: Endianness, const N: usize>()
      -> BitsPattern
    {
      /// Returns the constant operand of operator expressions.
      const fn operand<O: Native, T: Formula<O>>() -> O {
        match T::OP {
          Equ(val) | Gte(val) | Lte(val) => val,
          _ => O::DEFAULT,
        }
      }

      /// Compile both `lhs` and `rhs` sides of the expression.
      /// NOTE: Function indirection is required as it avoid a "cycle detected" compilation error.
      const fn operands<O: Native, T: Formula<O>, E: Endianness, const N: usize>()
        -> &'static [BitsPattern]
      {
        &const { [compile_impl::<O, T::Lhs, E, N>(), compile_impl::<O, T::Rhs, E, N>()] }
      }

      use Operator::*;
      use BitsPattern::*;

      match T::OP {
        True => Init(&[ByteInterval::ANY; N]),
        And => All(operands::<O, T, E, N>()),
        Or => Any(operands::<O, T, E, N>()),
        Equ(_) => Init(&const {
          let bytes = match E::LITERAL.is_little() {
            true => num::to_le_bytes(operand::<O, T>()),
            _ => num::to_be_bytes(operand::<O, T>()),
          };
          // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
          let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
          let mut intervals = [ByteInterval::ANY; N];
          let mut i = 0;
          while i < N {
            intervals[i] = ByteInterval::eq(0xff, bytes[i]);
            i += 1;
          }
          intervals
        }),
        Gte(_) | Lte(_) => Any(&const {
          let intervals = &const {
            let bytes = num::to_be_bytes(operand::<O, T>());
            // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
            let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
            let mut intervals = [[ByteInterval::ANY; N]; N];
            let mut i = 0;
            while i < N {
              let mut j = 0;
              while j <= i {
                let k = if E::LITERAL.is_little() { N - j - 1 } else { j };
                intervals[i][k] = match T::OP {
                  // Greater-than-or-equals cases (`>=`).
                  Gte(_) if j + 1 == N => ByteInterval::new(0xff, bytes[j], 0xff),
                  Gte(_) if j == i => match bytes[j].checked_add(1) {
                    Some(start) => ByteInterval::new(0xff, start, 0xff),
                    _ => ByteInterval::EMPTY,
                  },
                  // Lower-than-or-equals cases (`<=`).
                  Lte(_) if j + 1 == N => ByteInterval::new(0xff, 0x00, bytes[j]),
                  Lte(_) if j == i => match bytes[j].checked_sub(1) {
                    Some(end) => ByteInterval::new(0xff, 0x00, end),
                    _ => ByteInterval::EMPTY,
                  },
                  // Common cases.
                  _ if j < i => ByteInterval::eq(0xff, bytes[j]),
                  _ => ByteInterval::ANY,
                };
                j += 1;
              }
              i += 1;
            }
            intervals
          };
          let mut patterns = [Uninit; N];
          let mut i = 0;
          while i < N {
            patterns[i] = Init(&intervals[i]);
            i += 1;
          }
          patterns
        }),
      }
    }

    const { compile_impl::<NativeOf<T::Input>, T::Satisfy, <T::Input as Integer>::Endian, N>() }
  }

  #[inline(always)]
  const fn offset<T: Layout>(off: usize, max: usize) -> &'static BitsPattern {


    #[inline(always)]
    const fn unbounded<T: Layout>(off: usize) -> &'static BitsPattern {
      use LayoutKind::*;
      match T::KIND {
        Term if off == 0 => &const {
          macro_rules! compile_dispatch {
            ($($size:literal)+) => {
              match size_of_term::<T::Term>() {
                $($size => compile::<T::Term, $size>(),)+
                _ => unreachable!(),
              }
            };
          }
          compile_dispatch!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16)
        },
        Term => &BitsPattern::Uninit,
        Repeat(n, s) if off == 0 || off < n * s => unbounded::<T::Lhs>(off % s),
        Repeat(_, _) => &BitsPattern::Uninit,
        Wildcard(s) => unbounded::<T::Lhs>(off % s),
        And(l, r) => {
          let l = if off >= l {
            unbounded::<T::Lhs>(off - l)
          } else {
            &BitsPattern::Uninit
          };
          if matches!(l, &BitsPattern::Uninit) && off >= r {
            unbounded::<T::Rhs>(off - r)
          } else {
            &BitsPattern::Uninit
          }
        }
      }
    }

    if off >= max {
      return &BitsPattern::Uninit;
    }
    unbounded::<T>(off)
  }

  // impl `Layout`
  const _: () = {
    impl<T: Term> Layout for T {
      type Lhs = T;
      type Rhs = T;
      type Term = T;
      const KIND: LayoutKind = LayoutKind::Term;
    }

    impl<L: Layout, LOff: Literal<usize>, R: Layout, ROff: Literal<usize>> Layout for
      And<(L, LOff), (R, ROff)>
    {
      type Lhs = L;
      type Rhs = R;
      type Term = ();
      const KIND: LayoutKind = LayoutKind::And(LOff::LITERAL, ROff::LITERAL);
    }
  };

  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub(crate) enum Intersection {
    Disjoint, // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
    Overlap,  // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
    Subset,   // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)
  }

  pub(crate) const fn intersects<A: Layout, B: Layout>(bound: usize) -> Intersection {
    struct Intersects<A, B> {
      a: BitsPattern,
      b: BitsPattern,
      bound: usize,
      offset: usize,
      _types: PhantomData<(A, B)>,
    }

    const ANY: u8 = 0b01;
    const ALL: u8 = 0b11;

    impl<A: Layout, B: Layout> Intersects<A, B> {
      const fn with_a(&mut self, a: BitsPattern) -> u8 {
        self.a = a;
        self.compute()
      }

      const fn with_b(&mut self, b: BitsPattern) -> u8 {
        self.b = b;
        self.compute()
      }

      const fn offset_a(&mut self) -> u8 {
        self.with_a(*offset::<A>(self.offset, self.bound))
      }

      const fn offset_b(&mut self) -> u8 {
        self.with_b(*offset::<B>(self.offset, self.bound))
      }

      const fn next(&mut self, a: &'static [ByteInterval], b: &'static [ByteInterval]) -> u8 {
        self.offset += 1;
        self.a = BitsPattern::Init(a);
        self.b = BitsPattern::Init(b);
        self.compute()
      }

      const fn compute(&mut self) -> u8 {
        use BitsPattern::*;
        match (self.a, self.b) {
          _ if self.offset >= self.bound => ALL,
          (Init([]) | All([]) | Any([]), _) => self.offset_a(),
          (All([a]) | Any([a]), _) => self.with_a(*a),
          (All([la, ra @ ..]), _) => self.with_a(*la) & self.with_a(All(ra)),
          (Any([la, ra @ ..]), _) => match (self.with_a(*la), self.with_a(Any(ra))) {
            (ALL, ALL) => ALL,
            (0, 0) => 0,
            _ => ANY,
          },
          (_, Init([]) | All([]) | Any([])) => self.offset_b(),
          (_, All([b]) | Any([b])) => self.with_b(*b),
          (_, All([lb, rb @ ..])) => self.with_b(*lb) & self.with_b(All(rb)),
          (_, Any([lb, rb @ ..])) => match (self.with_b(*lb), self.with_b(Any(rb))) {
            (ALL, _) | (_, ALL) => ALL,
            (0, 0) => 0,
            _ => ANY,
          },
          (Uninit, Uninit) => self.next(&[], &[]),
          (Init([_, ra @ ..]), Uninit) => self.next(ra, &[]),
          (Uninit, Init([_, rb @ ..])) => self.next(&[], rb),
          (Init([la, ra @ ..]), Init([lb, rb @ ..])) => match (la.len(), la.intersects(lb)) {
            (_, 0) => 0,
            (l, r) => (if l == r { ALL } else { ANY }) & self.next(ra, rb)
          }
        }
      }
    }

    let (a, b) = (*offset::<A>(0, bound), *offset::<B>(0, bound));
    match (Intersects::<A, B> { a, b, bound, offset: 0, _types: PhantomData }.compute()) {
      ANY => Intersection::Overlap,
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
      #[inline(always)]
      pub const fn is_sized(&self) -> bool {
        matches!(self, MaybeUnsized::Sized)
      }

      #[inline(always)]
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

pub trait Scalar: Copy + 'static {
  type Repr: num::Integer;
  type Satisfy: layout::Formula<num::NativeOf<Self::Repr>>;
  const RAWDEFAULT: Self;
}

// impl `Scalar`.
const _: () = {
  impl<T: num::Integer> Scalar for T {
    type Repr = T;
    type Satisfy = ();
    const RAWDEFAULT: Self = T::ZERO;
  }

  impl Scalar for bool {
    type Repr = u8;
    type Satisfy = layout::Lte<u8, num::LiteralUint<u8, 1>>;
    const RAWDEFAULT: Self = false;
  }

  impl Scalar for char {
    type Repr = u32;
    type Satisfy = layout::Or<
      layout::Gte<u32, num::LiteralUint<u32, 0x110000>>,
      layout::And<
        layout::Gte<u32, num::LiteralUint<u32, 0xD800>>,
        layout::Lte<u32, num::LiteralUint<u32, 0xDFFF>>,
      >
    >;
    const RAWDEFAULT: Self = '\0';
  }

  impl Scalar for NonZero<u8> {
    type Repr = u8;
    type Satisfy = layout::Gte<u8, num::LiteralUint<u8, 1>>;
    const RAWDEFAULT: Self = match NonZero::new(1) {
      Some(val) => val,
      _ => unreachable!()
    };
  }

  impl Scalar for NonZero<u16> {
    type Repr = u16;
    type Satisfy = layout::Gte<u16, num::LiteralUint<u16, 1>>;
    const RAWDEFAULT: Self = match NonZero::new(1) {
      Some(val) => val,
      _ => unreachable!()
    };
  }
};

pub trait Type: MinimallySized<Self::Sizing> {
  type Sizing: sizing::Sizing;
  type Layout: layout::Layout;
}

#[inline(always)]
pub const fn min_size_of<T: Type + ?Sized>() -> usize {
  <T as MinimallySized<T::Sizing>>::MINSIZE
}

#[inline(always)]
pub const fn is_sized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_sized()
}

#[inline(always)]
pub const fn is_unsized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_unsized()
}

#[inline(always)]
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

// impl `Type`
const _: () = {
  impl<T: Type + Copy> Default for T {
    const DEFAULT: Self = default::<T>();
  }

  pub struct ScalarLayout<T>(PhantomData<T>);

  impl<T: Scalar> layout::Term for ScalarLayout<T> {
    type Input = T::Repr;
    type Satisfy = T::Satisfy;
    const BITS: Option<num::NativeOf<Self::Input>> = None;
    const DEFAULT: Self::Input = unsafe { transmute_unchecked(T::RAWDEFAULT) };
  }

  impl<T: Scalar> Type for T {
    type Sizing = sizing::Sized;
    type Layout = ScalarLayout<T>;
  }

  pub struct ArrayLayout<T, const N: usize>(PhantomData<T>);

  impl<T: Type, const N: usize> layout::Layout for ArrayLayout<T, N> {
    type Lhs = T::Layout;
    type Rhs = ();
    type Term = ();
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
    type Term = ();
    const KIND: layout::LayoutKind = layout::LayoutKind::Wildcard(size_of::<T>());
  }

  impl<T: Type> Type for [T] {
    type Sizing = sizing::Unsized;
    type Layout = SliceLayout<T>;
  }
};

mod internal {
  use crate::layout::{intersects, wildcard_size, Intersection};
  use crate::{min_size_of, Type, Default};
  use crate::num::{cast, Integer};

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
    #[inline(always)]
    const fn size_of_with<M: Integer>(&self, metadata: M) -> usize {
      self.min_size + cast::<_, usize>(metadata) * self.dst_slice_item_size
    }
  }

  #[inline(always)]
  pub const fn layout_of<T: ?Sized + Type>() -> Layout {
    const {
      let min_size = min_size_of::<T>();
      let dst_slice_item_size = wildcard_size::<T::Layout>();
      Layout { min_size, dst_slice_item_size }
    }
  }

  #[inline(always)]
  pub const fn transmute_metadata<M: Integer>(a: &Layout, b: &Layout, metadata: M) -> M {
    match b.dst_slice_item_size {
      0 => M::DEFAULT,
      n => cast((a.size_of_with(metadata) - b.min_size) / n),
    }
  }

  #[inline(always)]
  pub const fn transmute_metadata_checked<M: Integer>(
    a: &Layout, b: &Layout, metadata: M, checks: &Option<MetadataChecks>
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
    #[inline(always)]
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
  pub const fn maybe_size_mismatch(a: &Layout, b: &Layout)
    -> Result<Option<MetadataChecks>, &'static str>
  {
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
  pub(super) const fn transmutation<Src: ?Sized + Type, Dst: ?Sized + Type>()
    -> Result<Transmutation, &'static str>
  {
    const {
      let (a, b) = (&layout_of::<Src>(), &layout_of::<Dst>());
      // Compute the possible size mismatch from `Src` and `Dst` layouts.
      match maybe_size_mismatch(a, b) {
        Err(e) => Err(e),
        Ok(maybe_size_mismatch) => {
          use MetadataChecks::*;
          // Compute the minimum size in bytes need to fully intersects `Src` and `Dst`.
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
          Ok(Transmutation {
            maybe_size_mismatch,
            source_to_target_intersection: intersects::<Src::Layout, Dst::Layout>(bound),
            target_to_source_intersection: intersects::<Dst::Layout, Src::Layout>(bound),
          })
        }
      }
    }
  }

  #[inline(always)]
  pub(super) const fn convert_metadata<Src: ?Sized + Type, Dst: ?Sized + Type>(metadata: usize)
    -> usize
  {
    let (a, b) = const { (&layout_of::<Src>(), &layout_of::<Dst>()) };
    match b.dst_slice_item_size {
      0 => 0,
      n => ((a.min_size + metadata * a.dst_slice_item_size) - b.min_size) / n,
    }
  }

  #[cfg(test)]
  impl Transmutation {
    #[inline(always)]
    const fn new(
      maybe_size_mismatch: Option<MetadataChecks>,
      source_to_target_intersection: Intersection,
      target_to_source_intersection: Intersection,
    ) -> Self {
      Self { maybe_size_mismatch, source_to_target_intersection, target_to_source_intersection }
    }

    #[inline(always)]
    pub const fn unreachable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Disjoint, Intersection::Disjoint)
    }

    #[inline(always)]
    pub const fn fallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Overlap)
    }

    #[inline(always)]
    pub const fn fallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Subset)
    }

    #[inline(always)]
    pub const fn infallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, Intersection::Subset, Intersection::Overlap)
    }

    #[cfg(test)]
    #[inline(always)]
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

fn main() {
  use std::ptr::from_ref;
  use num::LiteralUint;
  use core::num::NonZero;

  #[derive(Debug, Copy, Clone)]
  struct Yolo {
    a: bool,
    b: u8,
    c: NonZero<u16>,
  }

  unsafe impl ptr::Pointee for Yolo {
    type Metadata = ();
  }

  macro_rules! field_layout {
    ($field:ident: $ty:ty) => {
      (<$ty as Type>::Layout, LiteralUint<usize, { offset_of!(Self, $field) as u128 }>)
    };
  }

  impl Type for Yolo {
    type Sizing = <NonZero<u16> as Type>::Sizing;
    type Layout = layout::And<
      field_layout!(a: bool),
      (layout::And<
        field_layout!(b: u8),
        field_layout!(c: NonZero<u16>),
      >, LiteralUint<usize, 0>)
    >;
  }

  impl MinimallySized<sizing::Unsized> for Yolo {
    const MINSIZE: usize = if is_unsized::<Self>() {
      offset_of!(Self, c) + min_size_of::<NonZero<u16>>()
    } else {
      panic!("use of `MINSIZE` with an invalid `Unsized` sizing")
    };
  }

  unsafe {
    println!("{}", layout::eval::<u16, layout::Gte<u16, LiteralUint<u16, 0xabcd>>>(0xabcd));
    println!("{}", layout::eval::<u16, layout::Gte<u16, LiteralUint<u16, 0xabcd>>>(0xabcd - 1));
    println!("{}", layout::eval::<u16, layout::Lte<u16, LiteralUint<u16, 0xabcd>>>(0xabcd));
    println!("{}", layout::eval::<u16, layout::Lte<u16, LiteralUint<u16, 0xabcd>>>(0xabcd + 1));

    println!("{}", validate::<[bool; 3]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 0));
    println!("{}", validate::<[bool; 3]>(from_ref(&[1u8, 0u8, 2u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>; 3]>(from_ref(&[1u8, 1u8, 1u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>; 3]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 0));
    println!("{}", validate::<[NonZero<u8>]>(from_ref(&[1u8, 1u8, 1u8]).cast(), 3));
    println!("{}", validate::<[NonZero<u8>]>(from_ref(&[1u8, 0u8, 1u8]).cast(), 3));
    println!("{}", validate::<Yolo>(from_ref(&Yolo {
      a: false,
      b: 0,
      c: NonZero::new(1).unwrap(),
    }).cast(), 3));


    println!("{:?}", layout::intersects::<<Yolo as Type>::Layout, <Yolo as Type>::Layout>(size_of::<Yolo>()));

    println!("{:?}", internal::transmutation::<u8, u8>());
    println!("{:?}", internal::transmutation::<bool, bool>());
    println!("{:?}", internal::transmutation::<u8, bool>());
    println!("{:?}", internal::transmutation::<bool, u8>());

    println!("{:?}", default::<Yolo>());
  }
}
