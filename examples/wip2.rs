#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]
/// # mutant
///
///

// -------------------------------------------------------------------------------------------------
// marker

/// Marker traits and types.
pub mod marker {

}

pub trait Get<T> {
  const VALUE: T;
}

pub trait Lit {
  type Type;
  const LITERAL: Self::Type;
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

pub trait Bytes: Copy + Zero + Ones + 'static {
  const LEN: usize;
}

const _: () = {
  impl Default for bool {
    const DEFAULT: Self = false;
  }

  impl Default for char {
    const DEFAULT: Self = '\0';
  }

  impl Zero for bool {
    const ZERO: Self = false;
  }

  impl Zero for char {
    const ZERO: Self = '\0';
  }

  impl<T: Default, const N: usize> Default for [T; N] {
    const DEFAULT: Self = [T::DEFAULT; N];
  }

  impl<T: Zero, const N: usize> Zero for [T; N] {
    const ZERO: Self = [T::ZERO; N];
  }

  impl<T: Ones, const N: usize> Ones for [T; N] {
    const ONES: Self = [T::ONES; N];
  }

  impl<const N: usize> Bytes for [u8; N] {
    const LEN: usize = N;
  }
};

// -------------------------------------------------------------------------------------------------
// num

pub mod num {
  use super::{ptr, Bytes, Default, Get, Lit, Ones, Zero};
  use core::cmp::Ordering;
  use core::fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex};
  use core::mem::size_of;
  use core::ops::Add;

  // -----------------------------------------------------------------------------------------------
  // endian

  pub mod endian {
    use crate::Get;

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

    pub trait Endianness: Get<Endian> + sealed::Endianness {}

    mod sealed {
      pub trait Endianness {}
    }

    const _: () = {
      impl sealed::Endianness for Le {}
      impl sealed::Endianness for Be {}

      impl Endianness for Le {}
      impl Endianness for Be {}

      impl Get<Endian> for Le {
        const VALUE: Endian = Endian::Le;
      }

      impl Get<Endian> for Be {
        const VALUE: Endian = Endian::Be;
      }

      impl crate::Default for Endian {
        const DEFAULT: Self = Ne::VALUE;
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
    use crate::Get;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Signed;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Unsigned;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub enum Sign {
      Signed,
      Unsigned,
    }

    pub trait Signedness: Get<Sign> + sealed::Signedness {}

    mod sealed {
      pub trait Signedness {}
    }

    const _: () = {
      impl sealed::Signedness for Signed {}
      impl sealed::Signedness for Unsigned {}

      impl Signedness for Signed {}
      impl Signedness for Unsigned {}

      impl Get<Sign> for Signed {
        const VALUE: Sign = Sign::Signed;
      }

      impl Get<Sign> for Unsigned {
        const VALUE: Sign = Sign::Unsigned;
      }

      impl crate::Default for Sign {
        const DEFAULT: Self = Signed::VALUE;
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

  // -----------------------------------------------------------------------------------------------
  // Literal

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Literal<T: Integer, const VALUE: u128>(T);

  const _: () = {
    impl<T: Integer, const VALUE: u128> Literal<T, VALUE> {
      #[inline(always)]
      const fn new() -> Self {
        Self(Self::LITERAL)
      }

      #[inline(always)]
      const fn into_inner(self) -> T {
        Self::LITERAL
      }
    }

    impl<T: Integer, const VALUE: u128> Lit for Literal<T, VALUE> {
      type Type = T;
      const LITERAL: T = match cast_checked(VALUE) {
        Some(x) => x,
        _ => panic!("literal `VALUE` is too large for type `T`."),
      };
    }

    impl<T: Integer, const VALUE: u128> Default for Literal<T, VALUE> {
      const DEFAULT: Self = Self::new();
    }

    impl<T: Integer, const VALUE: u128> Minimum for Literal<T, VALUE> {
      const MIN: Self = Self::new();
    }

    impl<T: Integer, const VALUE: u128> Maximum for Literal<T, VALUE> {
      const MAX: Self = Self::new();
    }
  };

  // -----------------------------------------------------------------------------------------------
  // ...

  use endian::Endianness;
  pub use types::*;

  pub struct NumberSpec {
    pub width: u8,
    pub endian: endian::Endian,
    pub sign: sign::Sign,
  }

  pub unsafe trait Integer:
    Zero + Ones + Default + Minimum + Maximum + Copy + 'static
  {
    type Bytes: Bytes;
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
  pub struct Dft<T: Integer>(T);

  impl<T: Integer> Lit for Dft<T> {
    type Type = T;
    const LITERAL: Self::Type = T::DEFAULT;
  }

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Le<T: Integer<Endian = endian::Ne>>(T);

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Be<T: Integer<Endian = endian::Ne>>(T);

  #[inline]
  pub const fn swap_bytes<T: Integer + Default>(n: T) -> T {
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
  pub const fn from_bytes<T: Integer>(bytes: T::Bytes) -> T {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(bytes) }
  }

  #[inline]
  pub const fn to_bytes<T: Integer>(n: T) -> T::Bytes {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(n) }
  }

  #[inline]
  pub const fn from_ne_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_le_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_be_bytes<T: Integer>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_ne_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_native() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_le_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_little() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_be_bytes<T: Integer>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_big() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_ne<T: Integer>(n: T) -> T {
    if const { !T::Endian::VALUE.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_be<T: Integer>(n: T) -> T {
    if const { !T::Endian::VALUE.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_le<T: Integer>(n: T) -> T {
    if const { !T::Endian::VALUE.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn cast<T: Integer, U: Integer>(n: T) -> U {
    union Cast<T: Copy, U: Copy> {
      src: T,
      dst: U,
    }

    let mut cast = Cast { dst: U::Bytes::ZERO };
    cast.src = to_le_bytes(n);
    from_le_bytes(unsafe { cast.dst })
  }

  #[inline]
  pub const fn cast_checked<T: Integer, U: Integer>(n: T) -> Option<U> {
    let new_n = cast(n);
    match is_eq(cast(new_n), n) {
      true => Some(new_n),
      _ => None,
    }
  }

  #[inline]
  pub const fn count_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).count_zeros()
  }

  #[inline]
  pub const fn count_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).count_ones()
  }

  #[inline]
  pub const fn leading_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).leading_zeros()
  }

  #[inline]
  pub const fn trailing_zeros<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_zeros()
  }

  #[inline]
  pub const fn leading_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).leading_ones()
  }

  #[inline]
  pub const fn trailing_ones<T: Integer>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_ones()
  }

  #[inline]
  pub const fn rotate_left<T: Integer>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_left(count))
  }

  #[inline]
  pub const fn rotate_right<T: Integer>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_right(count))
  }

  #[inline]
  pub const fn not<T: Integer>(n: T) -> T {
    cast(!cast::<T, u128>(n))
  }

  #[inline]
  pub const fn add<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) + cast::<T, u128>(b))
  }

  #[inline]
  pub const fn sub<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) - cast::<T, u128>(b))
  }

  #[inline]
  pub const fn mul<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) * cast::<T, u128>(b))
  }

  #[inline]
  pub const fn div<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) / cast::<T, u128>(b))
  }

  #[inline]
  pub const fn rem<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) % cast::<T, u128>(b))
  }

  #[inline]
  pub const fn shr<T: Integer>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) >> b)
  }

  #[inline]
  pub const fn shl<T: Integer>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) << b)
  }

  #[inline]
  pub const fn and<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) & cast::<T, u128>(b))
  }

  #[inline]
  pub const fn xor<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) ^ cast::<T, u128>(b))
  }

  #[inline]
  pub const fn or<T: Integer>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) | cast::<T, u128>(b))
  }

  #[inline]
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

  #[inline]
  pub const fn is_lt<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less)
  }

  #[inline]
  pub const fn is_le<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Equal)
  }

  #[inline]
  pub const fn is_gt<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater)
  }

  #[inline]
  pub const fn is_ge<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater | Ordering::Equal)
  }

  #[inline]
  pub const fn is_eq<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Equal)
  }

  #[inline]
  pub const fn is_ne<T: Integer>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Greater)
  }

  #[inline(always)]
  const unsafe fn transmute_unchecked<T: Copy, U: Copy>(val: T) -> U {
    union Transmute<T: Copy, U: Copy> {
      val: T,
      dst: U,
    }
    Transmute { val }.dst
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

  impl<T: Integer> Default for T {
    const DEFAULT: Self = Self::ZERO;
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

  impl<T: Integer> Get<NumberSpec> for T {
    const VALUE: NumberSpec = NumberSpec {
      width: T::Bytes::LEN as u8,
      endian: T::Endian::VALUE,
      sign: sign::Sign::Unsigned,
    };
  }

  impl<T: Integer> Get<T> for () {
    const VALUE: T = T::ZERO;
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
        pub const fn from<U: Integer>(n: U) -> Self {
          cast(n)
        }

        #[inline]
        pub const fn into<U: Integer>(self) -> U {
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

      impl <Rhs: Integer, $($generics)*> Add<Rhs> for $T where $Raw: Add, <$Raw as Add>::Output: Integer {
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

  impl_adt! { (T: Integer<Endian = endian::Ne>) Le<T>[T] (<T as Integer>::Bytes) }
  impl_adt! { (T: Integer<Endian = endian::Ne>) Be<T>[T] (<T as Integer>::Bytes) }
}

// -------------------------------------------------------------------------------------------------
// ptr

pub mod ptr {
  use core::mem;

  use super::num::Integer;
  use super::Default;

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

// -------------------------------------------------------------------------------------------------
// theory

pub mod theory {
  use core::marker::PhantomData;
  use num::endian::Endianness;
  use num::Native;
  use crate::bits::{Interval, Pattern};
  use super::{num, Lit};

  pub enum Operator<T> {
    True, Equ(T), Gte(T), Lte(T), And, Or
  }

  pub trait Formula<T> {
    type Lhs: Formula<T>;
    type Rhs: Formula<T>;
    const OP: Operator<T>;
  }

  #[derive(Copy, Clone)]
  pub struct True;

  impl<O> Formula<O> for True {
    type Lhs = True;
    type Rhs = True;
    const OP: Operator<O> = Operator::True;
  }

  #[derive(Copy, Clone)]
  pub struct Equ<Val>(PhantomData<Val>);

  impl<Val: Lit> Formula<Val::Type> for Equ<Val> {
    type Lhs = True;
    type Rhs = True;
    const OP: Operator<Val::Type> = Operator::Equ(Val::LITERAL);
  }

  #[derive(Copy, Clone)]
  pub struct Gte<Val>(PhantomData<Val>);

  impl<Val: Lit> Formula<Val::Type> for Gte<Val> {
    type Lhs = True;
    type Rhs = True;
    const OP: Operator<Val::Type> = Operator::Gte(Val::LITERAL);
  }

  #[derive(Copy, Clone)]
  pub struct Lte<Val>(PhantomData<Val>);

  impl<Val: Lit> Formula<Val::Type> for Lte<Val> {
    type Lhs = True;
    type Rhs = True;
    const OP: Operator<Val::Type> = Operator::Lte(Val::LITERAL);
  }

  #[derive(Copy, Clone)]
  pub struct And<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

  impl<O, Lhs: Formula<O>, Rhs: Formula<O>> Formula<O> for And<Lhs, Rhs> {
    type Lhs = Lhs;
    type Rhs = Rhs;
    const OP: Operator<O> = Operator::And;
  }

  #[derive(Copy, Clone)]
  pub struct Or<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

  impl<O, Lhs: Formula<O>, Rhs: Formula<O>> Formula<O> for Or<Lhs, Rhs> {
    type Lhs = Lhs;
    type Rhs = Rhs;
    const OP: Operator<O> = Operator::Or;
  }

  #[inline(always)]
  pub const fn is_true<O: Copy, T: Formula<O>>() -> bool {
    matches!(T::OP, Operator::True)
  }

  #[inline(always)]
  pub const fn eval<O: Native, T: Formula<O>>(val: O) -> bool {
    match T::OP {
      Operator::True => true,
      Operator::And => eval::<O, T::Lhs>(val) && eval::<O, T::Rhs>(val),
      Operator::Or => eval::<O, T::Lhs>(val) || eval::<O, T::Rhs>(val),
      Operator::Gte(rhs) => num::is_ge(val, rhs),
      Operator::Lte(rhs) => num::is_le(val, rhs),
      Operator::Equ(rhs) => num::is_eq(val, rhs),
    }
  }

  pub trait Theory {
    type Type: Native;
    type Expr: Formula<Self::Type>;
    const MASK: Option<Self::Type>;
  }

  #[derive(Copy, Clone)]
  pub struct Masked<M, E>(PhantomData<(M, E)>);

  impl<M: Lit<Type: Native>, E: Formula<M::Type>> Theory for Masked<M, E> {
    type Type = M::Type;
    type Expr = E;
    const MASK: Option<M::Type> = Some(M::LITERAL);
  }

  #[derive(Copy, Clone)]
  pub struct Unmasked<O, E>(PhantomData<(O, E)>);

  impl<O: Native, E: Formula<O>> Theory for Unmasked<O, E> {
    type Type = O;
    type Expr = E;
    const MASK: Option<O> = None;
  }

  #[inline(always)]
  pub const fn check<T: Theory>(val: T::Type) -> bool {
    let val = match T::MASK {
      None => val,
      Some(mask) => {
        let val = num::and(val, mask);
        num::rotate_left(val, num::leading_zeros(mask))
      },
    };
    eval::<T::Type, T::Expr>(val)
  }

  #[inline(always)]
  pub const unsafe fn check_ptr<T: Theory>(
    ptr: *const impl num::Integer<Bytes = <T::Type as num::Integer>::Bytes>
  ) -> bool {
    if const { is_true::<T::Type, T::Expr>() } {
      return true;
    }
    check::<T>(num::cast(*ptr))
  }

  const fn compile<O: Native, T: Formula<O>, E: Endianness, const N: usize>() -> Pattern {
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
      -> &'static [Pattern]
    {
      &const { [compile::<O, T::Lhs, E, N>(), compile::<O, T::Rhs, E, N>()] }
    }

    use Operator::*;
    use Pattern::*;

    match T::OP {
      True => Init(&[Interval::ANY; N]),
      And => All(operands::<O, T, E, N>()),
      Or => Any(operands::<O, T, E, N>()),
      Equ(_) => Init(&const {
        let bytes = match E::VALUE.is_little() {
          true => num::to_le_bytes(operand::<O, T>()),
          _ => num::to_be_bytes(operand::<O, T>()),
        };
        // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
        let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
        let mut intervals = [Interval::ANY; N];
        let mut i = 0;
        while i < N {
          intervals[i] = Interval::eq(0xff, bytes[i]);
          i += 1;
        }
        intervals
      }),
      Gte(_) | Lte(_) => Any(&const {
        let intervals = &const {
          let bytes = num::to_be_bytes(operand::<O, T>());
          // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
          let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
          let mut intervals = [[Interval::ANY; N]; N];
          let mut i = 0;
          while i < N {
            let mut j = 0;
            while j <= i {
              let k = if E::VALUE.is_little() { N - j - 1 } else { j };
              intervals[i][k] = match T::OP {
                // Greater-than-or-equals cases (`>=`).
                Gte(_) if j + 1 == N => Interval::new(0xff, bytes[j], 0xff),
                Gte(_) if j == i => match bytes[j].checked_add(1) {
                  Some(start) => Interval::new(0xff, start, 0xff),
                  _ => Interval::EMPTY,
                },
                // Lower-than-or-equals cases (`<=`).
                Lte(_) if j + 1 == N => Interval::new(0xff, 0x00, bytes[j]),
                Lte(_) if j == i => match bytes[j].checked_sub(1) {
                  Some(end) => Interval::new(0xff, 0x00, end),
                  _ => Interval::EMPTY,
                },
                // Common cases.
                _ if j < i => Interval::eq(0xff, bytes[j]),
                _ => Interval::ANY,
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

  pub trait Compile<B, O, E> {
    const OUTPUT: Pattern;
  }

  // The only trick found to abstract the compilation a bit.
  macro_rules! impl_compile {
    { $($n:literal)+ } => {$(
      impl<E: Endianness, O: Native<Bytes = [u8; $n]>, T: Formula<O>>
        Compile<[u8; $n], O, E> for T
          { const OUTPUT: Pattern = compile::<O, T, E, $n>(); }
    )+};
  }

  impl_compile! { 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 }

  #[cfg(test)]
  mod tests {
    use crate::*;
    use super::*;
    use num::*;

    fn test_expr<T: Native, E: Formula<T>>(inputs: &[(T, bool)])
    where
      E: Compile<T::Bytes, T, endian::Le>,
      E: Compile<T::Bytes, T, endian::Be>,
    {
      for &(input, expected) in inputs {
        assert_eq!(expected, eval::<T, E>(input));
        // little-endian test
        let compiled = <E as Compile<T::Bytes, T, endian::Le>>::OUTPUT;
        assert_eq!(expected, compiled.matches(to_le_bytes(input)));
        // big-endian test
        let compiled = <E as Compile<T::Bytes, T, endian::Be>>::OUTPUT;
        assert_eq!(expected, compiled.matches(to_be_bytes(input)));
      }
    }

    #[test]
    fn test_compile_gte() {
      test_expr::<u8, Gte<Literal<u8, 0x00>>>(&[
        (0x00, true),
        (0xab, true),
        (0xff, true),
      ]);

      test_expr::<u8, Gte<Literal<u8, 0xab>>>(&[
        (0x00, false),
        (0xaa, false),
        (0xab, true),
        (0xac, true),
        (0xff, true),
      ]);

      test_expr::<u8, Gte<Literal<u8, 0xff>>>(&[
        (0x00, false),
        (0xab, false),
        (0xfe, false),
        (0xff, true),
      ]);

      test_expr::<u16, Gte<Literal<u16, 0x0000>>>(&[
        (0x0000, true),
        (0x1234, true),
        (0xffff, true),
      ]);

      test_expr::<u16, Gte<Literal<u16, 0xa0b1>>>(&[
        (0x0000, false),
        (0xa0b0, false),
        (0xa0b1, true),
        (0xa0b2, true),
        (0xffff, true),
      ]);

      test_expr::<u16, Gte<Literal<u16, 0xffff>>>(&[
        (0x0000, false),
        (0xa0b1, false),
        (0xfffe, false),
        (0xffff, true),
      ]);

      test_expr::<u32, Gte<Literal<u32, 0x00000000>>>(&[
        (0x00000000, true),
        (0x12345678, true),
        (0xffffffff, true),
      ]);

      test_expr::<u32, Gte<Literal<u32, 0xa0b1c2d3>>>(&[
        (0x00000000, false),
        (0xa0b1c2d2, false),
        (0xa0b1c2d3, true),
        (0xa0b1c2d4, true),
        (0xffffffff, true),
      ]);

      test_expr::<u32, Gte<Literal<u32, 0xffffffff>>>(&[
        (0x00000000, false),
        (0xa0b1c2d3, false),
        (0xfffffffe, false),
        (0xffffffff, true),
      ]);

      test_expr::<u64, Gte<Literal<u64, 0x0000000000000000>>>(&[
        (0x0000000000000000, true),
        (0x123456789abcdef0, true),
        (0xffffffffffffffff, true),
      ]);

      test_expr::<u64, Gte<Literal<u64, 0xa0b1c2d3e4f50607>>>(&[
        (0x0000000000000000, false),
        (0xa0b1c2d3e4f50606, false),
        (0xa0b1c2d3e4f50607, true),
        (0xa0b1c2d3e4f50608, true),
        (0xffffffffffffffff, true),
      ]);

      test_expr::<u64, Gte<Literal<u64, 0xffffffffffffffff>>>(&[
        (0x0000000000000000, false),
        (0xa0b1c2d3e4f50607, false),
        (0xfffffffffffffffe, false),
        (0xffffffffffffffff, true),
      ]);
    }

    #[test]
    fn test_compile_lte() {
      test_expr::<u8, Lte<Literal<u8, 0x00>>>(&[
        (0x00, true),
        (0x01, false),
        (0xab, false),
        (0xff, false),
      ]);

      test_expr::<u8, Lte<Literal<u8, 0xab>>>(&[
        (0x00, true),
        (0xaa, true),
        (0xab, true),
        (0xac, false),
        (0xff, false),
      ]);

      test_expr::<u8, Lte<Literal<u8, 0xff>>>(&[
        (0x00, true),
        (0xab, true),
        (0xfe, true),
        (0xff, true),
      ]);

      test_expr::<u16, Lte<Literal<u16, 0x0000>>>(&[
        (0x0000, true),
        (0x0001, false),
        (0x1234, false),
        (0xffff, false),
      ]);

      test_expr::<u16, Lte<Literal<u16, 0xa0b1>>>(&[
        (0x0000, true),
        (0xa0b0, true),
        (0xa0b1, true),
        (0xa0b2, false),
        (0xffff, false),
      ]);

      test_expr::<u16, Lte<Literal<u16, 0xffff>>>(&[
        (0x0000, true),
        (0xa0b1, true),
        (0xfffe, true),
        (0xffff, true),
      ]);

      test_expr::<u32, Lte<Literal<u32, 0x00000000>>>(&[
        (0x00000000, true),
        (0x00000001, false),
        (0x12345678, false),
        (0xffffffff, false),
      ]);

      test_expr::<u32, Lte<Literal<u32, 0xa0b1c2d3>>>(&[
        (0x00000000, true),
        (0xa0b1c2d2, true),
        (0xa0b1c2d3, true),
        (0xa0b1c2d4, false),
        (0xffffffff, false),
      ]);

      test_expr::<u32, Lte<Literal<u32, 0xffffffff>>>(&[
        (0x00000000, true),
        (0xa0b1c2d3, true),
        (0xfffffffe, true),
        (0xffffffff, true),
      ]);

      test_expr::<u64, Lte<Literal<u64, 0x0000000000000000>>>(&[
        (0x0000000000000000, true),
        (0x0000000000000001, false),
        (0x123456789abcdef0, false),
        (0xffffffffffffffff, false),
      ]);

      test_expr::<u64, Lte<Literal<u64, 0xa0b1c2d3e4f50607>>>(&[
        (0x0000000000000000, true),
        (0xa0b1c2d3e4f50606, true),
        (0xa0b1c2d3e4f50607, true),
        (0xa0b1c2d3e4f50608, false),
        (0xffffffffffffffff, false),
      ]);

      test_expr::<u64, Lte<Literal<u64, 0xffffffffffffffff>>>(&[
        (0x0000000000000000, true),
        (0xa0b1c2d3e4f50607, true),
        (0xfffffffffffffffe, true),
        (0xffffffffffffffff, true),
      ]);
    }

    #[test]
    fn test_compile_equ() {
      test_expr::<u8, Equ<Literal<u8, 0x00>>>(&[
        (0x00, true),
        (0x01, false),
        (0xff, false),
      ]);

      test_expr::<u8, Equ<Literal<u8, 0xab>>>(&[
        (0x00, false),
        (0xaa, false),
        (0xab, true),
        (0xac, false),
        (0xff, false),
      ]);

      test_expr::<u8, Equ<Literal<u8, 0xff>>>(&[
        (0x00, false),
        (0xab, false),
        (0xff, true),
      ]);

      test_expr::<u16, Equ<Literal<u16, 0x0000>>>(&[
        (0x0000, true),
        (0x0001, false),
        (0xffff, false),
      ]);

      test_expr::<u16, Equ<Literal<u16, 0xa0b1>>>(&[
        (0x0000, false),
        (0xa0b0, false),
        (0xa0b1, true),
        (0xa0b2, false),
        (0xffff, false),
      ]);

      test_expr::<u16, Equ<Literal<u16, 0xffff>>>(&[
        (0x0000, false),
        (0xfffe, false),
        (0xffff, true),
      ]);

      test_expr::<u32, Equ<Literal<u32, 0x00000000>>>(&[
        (0x00000000, true),
        (0x00000001, false),
        (0xffffffff, false),
      ]);

      test_expr::<u32, Equ<Literal<u32, 0xa0b1c2d3>>>(&[
        (0x00000000, false),
        (0xa0b1c2d2, false),
        (0xa0b1c2d3, true),
        (0xa0b1c2d4, false),
        (0xffffffff, false),
      ]);

      test_expr::<u32, Equ<Literal<u32, 0xffffffff>>>(&[
        (0x00000000, false),
        (0xfffffffe, false),
        (0xffffffff, true),
      ]);

      test_expr::<u64, Equ<Literal<u64, 0x0000000000000000>>>(&[
        (0x0000000000000000, true),
        (0x0000000000000001, false),
        (0xffffffffffffffff, false),
      ]);

      test_expr::<u64, Equ<Literal<u64, 0xa0b1c2d3e4f50607>>>(&[
        (0x0000000000000000, false),
        (0xa0b1c2d3e4f50606, false),
        (0xa0b1c2d3e4f50607, true),
        (0xa0b1c2d3e4f50608, false),
        (0xffffffffffffffff, false),
      ]);

      test_expr::<u64, Equ<Literal<u64, 0xffffffffffffffff>>>(&[
        (0x0000000000000000, false),
        (0xfffffffffffffffe, false),
        (0xffffffffffffffff, true),
      ]);
    }

    #[test]
    fn test_compile_and() {
      // u8 tests
      // Test if value is between 0x10 and 0x20 (inclusive)
      test_expr::<u8, And<Gte<Literal<u8, 0x10>>, Lte<Literal<u8, 0x20>>>>(&[
        (0x0f, false), // Below lower bound
        (0x10, true),  // At lower bound
        (0x15, true),  // Between bounds
        (0x20, true),  // At upper bound
        (0x21, false), // Above upper bound
      ]);

      // Test if value equals 0xab and is greater than 0x50 (should only be true for 0xab)
      test_expr::<u8, And<Equ<Literal<u8, 0xab>>, Gte<Literal<u8, 0x50>>>>(&[
        (0x50, false),
        (0xaa, false),
        (0xab, true),
        (0xac, false),
        (0xff, false),
      ]);

      // u16 tests with complex sample values
      // Test if value is between 0x1234 and 0x5678 (inclusive)
      test_expr::<u16, And<Gte<Literal<u16, 0x1234>>, Lte<Literal<u16, 0x5678>>>>(&[
        (0x1233, false),
        (0x1234, true),
        (0x3456, true),
        (0x5678, true),
        (0x5679, false),
      ]);

      // Test if value equals 0xa0b1 or 0xc0d1
      test_expr::<u16, And<Equ<Literal<u16, 0xa0b1>>, Equ<Literal<u16, 0xc0d1>>>>(&[
        (0xa0b1, false),
        (0xc0d1, false),
        (0x0000, false),
        (0xffff, false),
      ]);

      // Since both Equ expressions cannot be true simultaneously with different literals, always false

      // u32 tests with complex sample values
      // Test if value is greater than or equal to 0x10000000 and less than or equal to 0x20000000
      test_expr::<u32, And<Gte<Literal<u32, 0x10000000>>, Lte<Literal<u32, 0x20000000>>>>(&[
        (0x0fffffff, false),
        (0x10000000, true),
        (0x18000000, true),
        (0x20000000, true),
        (0x20000001, false),
      ]);

      // Test if value is between 0xa0b1c2d3 and 0xd4e5f6a7 (inclusive)
      test_expr::<u32, And<Gte<Literal<u32, 0xa0b1c2d3>>, Lte<Literal<u32, 0xd4e5f6a7>>>>(&[
        (0xa0b1c2d2, false),
        (0xa0b1c2d3, true),
        (0xb2c3d4e5, true),
        (0xd4e5f6a7, true),
        (0xd4e5f6a8, false),
      ]);

      // u64 tests with complex sample values
      // Test if value is between 0x0000000012345678 and 0x000000009abcdef0 (inclusive)
      test_expr::<u64, And<Gte<Literal<u64, 0x0000000012345678>>, Lte<Literal<u64, 0x000000009abcdef0>>>>(&[
        (0x0000000012345677, false),
        (0x0000000012345678, true),
        (0x0000000056789abc, true),
        (0x000000009abcdef0, true),
        (0x000000009abcdef1, false),
      ]);

      // Test a nested And expression: value >= 0x1000_0000_0000_0000 && value <= 0x1fff_ffff_ffff_ffff
      test_expr::<u64, And<Gte<Literal<u64, 0x1000000000000000>>, Lte<Literal<u64, 0x1fffffffffffffff>>>>(&[
        (0x0fffffffffffffff, false),
        (0x1000000000000000, true),
        (0x1fffffffffffffff, true),
        (0x2000000000000000, false),
      ]);
    }

    #[test]
    fn test_complex_expr() {
      // u8 tests
      // Test if value is between 0x10 and 0x20 inclusive, or equals 0x30
      test_expr::<u8, Or<And<Gte<Literal<u8, 0x10>>, Lte<Literal<u8, 0x20>>>, Equ<Literal<u8, 0x30>>>>(&[
        (0x0f, false), // Below lower bound
        (0x10, true),  // At lower bound
        (0x15, true),  // Between bounds
        (0x20, true),  // At upper bound
        (0x21, false), // Above upper bound
        (0x30, true),  // Equal to 0x30
        (0x31, false), // Just above 0x30
      ]);

      // u16 tests
      // Test if value equals 0xa0b1 or is between 0x1234 and 0x5678 inclusive
      test_expr::<u16, Or<Equ<Literal<u16, 0xa0b1>>, And<Gte<Literal<u16, 0x1234>>, Lte<Literal<u16, 0x5678>>>>>(&[
        (0xa0b1, true),    // Equal to 0xa0b1
        (0x1233, false),   // Below range
        (0x1234, true),    // Start of range
        (0x3456, true),    // Within range
        (0x5678, true),    // End of range
        (0x5679, false),   // Above range
        (0xffff, false),   // Some other value
      ]);

      // u32 tests
      // Test if value is between 0x10000000 and 0x20000000 inclusive, or equals 0xa0b1c2d3
      test_expr::<u32, Or<And<Gte<Literal<u32, 0x10000000>>, Lte<Literal<u32, 0x20000000>>>, Equ<Literal<u32, 0xa0b1c2d3>>>>(&[
        (0x0fffffff, false),   // Below lower bound
        (0x10000000, true),    // At lower bound
        (0x18000000, true),    // Within range
        (0x20000000, true),    // At upper bound
        (0x20000001, false),   // Above upper bound
        (0xa0b1c2d3, true),    // Equal to 0xa0b1c2d3
        (0xffffffff, false),   // Some other value
      ]);

      // u64 tests
      // Test if value equals 0x123456789abcdef0 or is between 0xa0b1c2d3e4f50607 and 0xffffffffffffffff inclusive
      test_expr::<u64, Or<Equ<Literal<u64, 0x123456789abcdef0>>, And<Gte<Literal<u64, 0xa0b1c2d3e4f50607>>, Lte<Literal<u64, 0xffffffffffffffff>>>>>(&[
        (0x123456789abcdef0, true), // Equal to specific value
        (0xa0b1c2d3e4f50606, false), // Below range
        (0xa0b1c2d3e4f50607, true),  // Start of range
        (0xffffffffffffffff, true),  // End of range
        (0x0000000000000000, false), // Some other value
      ]);

      // Complex nested expressions
      // u8 test: value is less than or equal to 0x0f, or between 0x20 and 0x30 inclusive, or greater than or equal to 0x41
      test_expr::<u8, Or<Or<Lte<Literal<u8, 0x0f>>, And<Gte<Literal<u8, 0x20>>, Lte<Literal<u8, 0x30>>>>, Gte<Literal<u8, 0x41>>>>(&[
        (0x0f, true),  // Less than or equal to 0x0f
        (0x10, false), // Not in any range
        (0x1f, false),
        (0x20, true),  // Start of middle range
        (0x25, true),
        (0x30, true),  // End of middle range
        (0x31, false),
        (0x40, false), // Exactly 0x40
        (0x41, true),  // Greater than or equal to 0x41
        (0xff, true),  // Maximum u8 value
      ]);
    }
  }
}

// -------------------------------------------------------------------------------------------------
// pattern

pub mod bits {
  pub use Pattern::*;

  pub trait KnownBits {
    const LAYOUT: Layout;
  }

  #[derive(Copy, Clone)]
  pub struct Interval {
    mask: u8,
    start: u8,
    end: u8,
  }

  impl core::fmt::Debug for Interval {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
      if self.start != 0 { write!(f, "{:#02x}", self.start)?; }
      write!(f, "..")?;
      if self.end != 0xff { write!(f, "={:#02x}", self.end)?; }
      Ok(())
    }
  }

  #[derive(Debug, Copy, Clone)]
  pub enum Pattern {
    Overlap,
    Uninit,
    Init(&'static [Interval]),
    All(&'static [Pattern]),
    Any(&'static [Pattern]),
  }

  #[derive(Debug, Copy, Clone)]
  pub struct Layout {
    pub head: &'static [Pattern],
    pub tail: Option<&'static [Pattern]>,
  }

  const ANY: u8 = 0b01;
  const ALL: u8 = 0b11;

  impl Interval {
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

    const fn len(&self) -> usize {
      1usize + (self.end - self.start) as usize
    }

    const fn contains(&self, byte: u8) -> bool {
      let byte = match self.mask {
        0x00 => return false,
        0xff => byte,
        mask if byte != 0 && byte & mask == 0 => return false,
        mask => (byte & mask) >> mask.trailing_zeros(),
      };
      self.start <= byte && byte <= self.end
    }

    const fn intersects(&self, o: &Self) -> usize {
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

  impl Pattern {
    const fn intersects(&self, o: &Self, a: &Layout, b: &Layout, i: usize, m: usize) -> u8 {
      use Pattern::*;
      match (self, o) {
        (Uninit, _) | (_, Uninit) => todo!(),
        (Overlap, _) | (_, Overlap) => panic!("internal error"),
        (Init([]) | All([]) | Any([]), _) => a.load(i, m).intersects(o, a, b, i, m),
        (All([l]) | Any([l]), _) => l.intersects(o, a, b, i, m),
        (All([l, ln @ ..]), _) => l.intersects(o, a, b, i, m) & All(ln).intersects(o, a, b, i, m),
        (Any([l, ln @ ..]), _) => match (l.intersects(o, a, b, i, m), All(ln).intersects(o, a, b, i, m)) {
          (ALL, ALL) => ALL,
          (0, 0) => 0,
          _ => ANY,
        },
        (_, Init([]) | All([]) | Any([])) => self.intersects(b.load(i, m), a, b, i, m),
        (_, All([r]) | Any([r])) => self.intersects(r, a, b, i, m),
        (_, All([r, rn @ ..])) => self.intersects(r, a, b, i, m) & self.intersects(&All(rn), a, b, i, m),
        (_, Any([r, rn @ ..])) => match (self.intersects(r, a, b, i, m), self.intersects(&Any(rn), a, b, i, m)) {
          (ALL, _) | (_, ALL) => ALL,
          (0, 0) => 0,
          _ => ANY,
        },
        (Init([l, ln @ ..]), Init([r, rn @ ..])) => match (l.len(), l.intersects(r)) {
          (_, 0) => 0,
          (l, r) => (if l == r { ANY } else { ALL }) & Init(ln).intersects(&Init(rn), a, b, i + 1, m)
        }
      }
    }

    #[cfg(test)]
    pub fn matches<T: Copy>(&self, memory: T) -> bool {
      use Pattern::*;
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

  impl Layout {
    const fn new(sized: &'static [Pattern], dst: Option<&'static [Pattern]>) -> Self {
      Self { head: sized, tail: dst }
    }

    const fn load(&self, off: usize, max: usize) -> &'static Pattern {
      if off >= max {
        return &Uninit;
      }
      if off < self.head.len() {
        return &self.head[off];
      }
      if let Some(dst) = self.tail {
        return &dst[(off - self.head.len()) % dst.len()];
      }
      &Uninit
    }

    const fn intersects(&self, o: &Self, max: usize) -> u8 {
      self.load(0, max).intersects(o.load(0, max), self, o, 0, max)
    }
  }
}

// -------------------------------------------------------------------------------------------------
// Data

pub mod data {
  use core::num::{NonZeroU16, NonZeroU8};
  use super::{num, Lit, num::Literal};
  use super::theory;
  use super::Bytes;
  use super::Default;

  pub enum Shape {
    Slice,
    Array(usize),
    Struct,
    Scalar
  }

  pub trait Scalar: Default {
    type Repr: num::Integer;
    type Theory: theory::Theory<Type= <Self::Repr as num::Integer>::Native>;
  }

  pub trait Fields {
    type Type: Type + ?Sized;
    type Rest: Fields;
    const IDX: usize;
    const OFF: usize;
  }

  pub trait Type {
    type Fields: Fields;
    type Scalar: Scalar;
    type Bytes: Bytes;
    type Item: Type;
    const SHAPE: Shape;
  }

  const _: () = {
    impl Fields for () {
      type Type = ();
      type Rest = ();
      const IDX: usize = 0;
      const OFF: usize = 0;
    }

    // impl<T: Word> super::bits::KnownBits for T
    // where
    //   <<T as Word>::Check as typed::Check>::Expr: typed::Compile<
    //     <<T as Word>::Repr as num::Number>::Bytes,
    //     <<T as Word>::Repr as num::Number>::Native,
    //     <<T as Word>::Repr as num::Number>::Endian,
    //   >
    // {
    //   const LAYOUT: crate::bits::Layout = crate::bits::Layout {
    //     head: &[
    //       <<<T as Word>::Check as typed::Check>::Expr as typed::Compile<
    //         <T::Repr as num::Number>::Bytes,
    //         <T::Repr as num::Number>::Native,
    //         <T::Repr as num::Number>::Endian>
    //       >::OUTPUT
    //     ],
    //     tail: None
    //   };
    // }

    impl<T: Scalar> Type for T {
      type Scalar = T;
      type Item = ();
      type Fields = ();
      type Bytes = <T::Repr as num::Integer>::Bytes;
      const SHAPE: Shape = Shape::Scalar;
    }

    impl<T: num::Integer> Scalar for T {
      type Repr = T;
      type Theory = theory::Unmasked<T::Native, theory::True>;
    }

    impl<T: num::Integer, const VALUE: u128> Scalar for Literal<T, VALUE> {
      type Repr = T;
      type Theory = theory::Unmasked<T::Native, theory::Equ<Literal<T::Native, VALUE>>>;
    }

    impl Scalar for bool {
      type Repr = u8;
      type Theory = theory::Unmasked<u8, theory::Lte<Literal<u8, 1>>>;
    }

    impl Scalar for char {
      type Repr = u32;
      type Theory = theory::Unmasked<u32, theory::Or<
        theory::Gte<Literal<u32, 0x110000>>,
        theory::And<
          theory::Gte<Literal<u32, 0xD800>>,
          theory::Lte<Literal<u32, 0xDFFF>>,
        >
      >>;
    }

    impl Default for NonZeroU8 {
      const DEFAULT: Self = match NonZeroU8::new(1) { Some(x) => x, None => unreachable!() };
    }

    impl Scalar for NonZeroU8 {
      type Repr = u8;
      type Theory = theory::Unmasked<u8, theory::Gte<Literal<u8, 1>>>;
    }

    impl Default for NonZeroU16 {
      const DEFAULT: Self = match NonZeroU16::new(1) { Some(x) => x, None => unreachable!() };
    }

    impl Scalar for NonZeroU16 {
      type Repr = u16;
      type Theory = theory::Unmasked<u16, theory::Gte<Literal<u16, 1>>>;
    }
  };

  /// Compute the length of an array or slice type.
  const fn len<T: Type + ?Sized>(metadata: usize) -> usize {
    debug_assert!(matches!(T::SHAPE, Shape::Array(_) | Shape::Slice));
    if let Shape::Array(len) = T::SHAPE { len } else { metadata }
  }

  /// Check if the memory at `ptr` matches the memory pattern of `T`.
  pub const unsafe fn matches<T: Type + ?Sized>(ptr: *const u8, metadata: usize) -> bool {
    /// Check if the memory at `ptr` matches the field `T`.
    const unsafe fn matches_field<T: Fields>(ptr: *const u8, metadata: usize) -> bool {
      matches::<T::Type>(ptr.wrapping_add(T::OFF), metadata)
        && (T::IDX == 0 || matches_field::<T::Rest>(ptr, metadata))
    }

    match T::SHAPE {
      Shape::Scalar => theory::check_ptr::<<T::Scalar as Scalar>::Theory>(
        ptr.cast::<<T::Scalar as Scalar>::Repr>()
      ),
      Shape::Struct => matches_field::<T::Fields>(ptr, metadata),
      Shape::Array(_) | Shape::Slice => {
        let (mut i, len) = (0, len::<T>(metadata));
        while i < len && matches::<T::Item>(ptr.wrapping_add(i * size_of::<T::Item>()), 0) {
          i += 1;
        }
        i == len
      }
    }
  }
}

// TODO:
// - move out `bits` and `data`
// - !!! Correct renaming !!!
// - const LAYOUT
// - derive
// - ...

fn main() {
  use std::ptr::from_ref;
  use num::Literal;

  unsafe {
    // println!("{}", data::matches::<char>(from_ref(&0x110000).cast(), 0));
    // println!("{}", data::matches::<char>(from_ref(&(0x110000 - 1)).cast(), 0));
    // println!("{}", data::matches::<bool>(from_ref(&0x00).cast(), 0));
    // println!("{}", data::matches::<bool>(from_ref(&0x01).cast(), 0));
    // println!("{}", data::matches::<bool>(from_ref(&0x02).cast(), 0));
    // println!("{:?}", <<<u16 as data::Word>::Check as typed::Check>::Expr as typed::Compile<[u8; 2], num::endian::Ne>>::OUTPUT);
    // println!("{:?}", <<<u32 as data::Word>::Check as typed::Check>::Expr as typed::Compile<[u8; 4], num::endian::Ne>>::OUTPUT);
    println!("{:?}", <<<bool as data::Scalar>::Theory as theory::Theory>::Expr as theory::Compile<[u8; 1], u8, num::endian::Ne>>::OUTPUT);
    println!("{:?}", <<<Literal<u16, 0xabcd> as data::Scalar>::Theory as theory::Theory>::Expr as theory::Compile<[u8; 2], u16, num::endian::Le>>::OUTPUT);
    println!("{:?}", <<<Literal<u16, 0xabcd> as data::Scalar>::Theory as theory::Theory>::Expr as theory::Compile<[u8; 2], u16, num::endian::Be>>::OUTPUT);
    println!("{:?}", <theory::Gte<Literal<u16, 0xabcd>> as theory::Compile<[u8; 2], u16, num::endian::Le>>::OUTPUT);
    println!("{:?}", <theory::Gte<Literal<u16, 0xabcd>> as theory::Compile<[u8; 2], u16, num::endian::Be>>::OUTPUT);
    println!("{:?}", <theory::Lte<Literal<u16, 0xabcd>> as theory::Compile<[u8; 2], u16, num::endian::Le>>::OUTPUT);
    println!("{:?}", <theory::Lte<Literal<u16, 0xabcd>> as theory::Compile<[u8; 2], u16, num::endian::Be>>::OUTPUT);
    println!("{}", theory::eval::<u16, theory::Gte<Literal<u16, 0xabcd>>>(0xabcd));
    println!("{}", theory::eval::<u16, theory::Gte<Literal<u16, 0xabcd>>>(0xabcd - 1));
    println!("{}", theory::eval::<u16, theory::Lte<Literal<u16, 0xabcd>>>(0xabcd));
    println!("{}", theory::eval::<u16, theory::Lte<Literal<u16, 0xabcd>>>(0xabcd + 1));
  }
}
