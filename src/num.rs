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
