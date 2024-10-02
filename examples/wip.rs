#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]
/// # mutant
///
///

// -------------------------------------------------------------------------------------------------
// marker

pub trait Lit<T> {
  const VALUE: T;
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

  impl Zero for bool {
    const ZERO: Self = false;
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
  use super::{ptr, typed, Bytes, Default, Lit, Ones, Term, Zero};
  use core::cmp::Ordering;
  use core::fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex};
  use core::mem::size_of;
  use core::ops::Add;

  // -----------------------------------------------------------------------------------------------
  // endian

  pub mod endian {
    use crate::Lit;

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

    pub trait Endianness: Lit<Endian> + sealed::Endianness {}

    mod sealed {
      pub trait Endianness {}
    }

    const _: () = {
      impl sealed::Endianness for Le {}
      impl sealed::Endianness for Be {}

      impl Endianness for Le {}
      impl Endianness for Be {}

      impl Lit<Endian> for Le {
        const VALUE: Endian = Endian::Le;
      }

      impl Lit<Endian> for Be {
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
    use crate::Lit;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Signed;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Unsigned;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub enum Sign {
      Signed,
      Unsigned,
    }

    pub trait Signedness: Lit<Sign> + sealed::Signedness {}

    mod sealed {
      pub trait Signedness {}
    }

    const _: () = {
      impl sealed::Signedness for Signed {}
      impl sealed::Signedness for Unsigned {}

      impl Signedness for Signed {}
      impl Signedness for Unsigned {}

      impl Lit<Sign> for Signed {
        const VALUE: Sign = Sign::Signed;
      }

      impl Lit<Sign> for Unsigned {
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

  use endian::Endianness;
  pub use types::*;

  pub struct NumberSpec {
    pub width: u8,
    pub endian: endian::Endian,
    pub sign: sign::Sign,
  }

  pub unsafe trait Number:
    Zero + Ones + Default + Minimum + Maximum + Copy + 'static
  {
    type Bytes: Bytes;
    type Endian: Endianness;
  }

  pub trait Minimum: Sized {
    const MIN: Self;
  }

  pub trait Maximum: Sized {
    const MAX: Self;
  }

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Le<T: Number<Endian = endian::Ne>>(T);

  #[repr(transparent)]
  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub struct Be<T: Number<Endian = endian::Ne>>(T);

  #[inline]
  pub const fn swap_bytes<T: Number + Default>(n: T) -> T {
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
  pub const fn from_bytes<T: Number>(bytes: T::Bytes) -> T {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(bytes) }
  }

  #[inline]
  pub const fn to_bytes<T: Number>(n: T) -> T::Bytes {
    const {
      assert!(size_of::<T>() >= size_of::<T::Bytes>());
    }
    unsafe { transmute_unchecked(n) }
  }

  #[inline]
  pub const fn from_ne_bytes<T: Number>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_le_bytes<T: Number>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn from_be_bytes<T: Number>(bytes: T::Bytes) -> T {
    let n = from_bytes(bytes);
    if const { !T::Endian::VALUE.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_ne_bytes<T: Number>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_native() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_le_bytes<T: Number>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_little() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_be_bytes<T: Number>(n: T) -> T::Bytes {
    if const { !T::Endian::VALUE.is_big() } {
      to_bytes(swap_bytes(n))
    } else {
      to_bytes(n)
    }
  }

  #[inline]
  pub const fn to_ne<T: Number>(n: T) -> T {
    if const { !T::Endian::VALUE.is_native() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_be<T: Number>(n: T) -> T {
    if const { !T::Endian::VALUE.is_big() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn to_le<T: Number>(n: T) -> T {
    if const { !T::Endian::VALUE.is_little() } {
      swap_bytes(n)
    } else {
      n
    }
  }

  #[inline]
  pub const fn cast<T: Number, U: Number>(n: T) -> U {
    union Cast<T: Copy, U: Copy> {
      src: T,
      dst: U,
    }

    let mut cast = Cast { dst: U::Bytes::ZERO };
    cast.src = to_le_bytes(n);
    from_le_bytes(unsafe { cast.dst })
  }

  #[inline]
  pub const fn count_zeros<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).count_zeros()
  }

  #[inline]
  pub const fn count_ones<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).count_ones()
  }

  #[inline]
  pub const fn leading_zeros<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).leading_zeros()
  }

  #[inline]
  pub const fn trailing_zeros<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_zeros()
  }

  #[inline]
  pub const fn leading_ones<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).leading_ones()
  }

  #[inline]
  pub const fn trailing_ones<T: Number>(n: T) -> u32 {
    cast::<T, u128>(n).trailing_ones()
  }

  #[inline]
  pub const fn rotate_left<T: Number>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_left(count))
  }

  #[inline]
  pub const fn rotate_right<T: Number>(n: T, count: u32) -> T {
    cast(cast::<T, u128>(n).rotate_right(count))
  }

  #[inline]
  pub const fn not<T: Number>(n: T) -> T {
    cast(!cast::<T, u128>(n))
  }

  #[inline]
  pub const fn add<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) + cast::<T, u128>(b))
  }

  #[inline]
  pub const fn sub<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) - cast::<T, u128>(b))
  }

  #[inline]
  pub const fn mul<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) * cast::<T, u128>(b))
  }

  #[inline]
  pub const fn div<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) / cast::<T, u128>(b))
  }

  #[inline]
  pub const fn rem<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) % cast::<T, u128>(b))
  }

  #[inline]
  pub const fn shr<T: Number>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) >> b)
  }

  #[inline]
  pub const fn shl<T: Number>(a: T, b: u32) -> T {
    cast(cast::<T, u128>(a) << b)
  }

  #[inline]
  pub const fn and<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) & cast::<T, u128>(b))
  }

  #[inline]
  pub const fn xor<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) ^ cast::<T, u128>(b))
  }

  #[inline]
  pub const fn or<T: Number>(a: T, b: T) -> T {
    cast(cast::<T, u128>(a) | cast::<T, u128>(b))
  }

  #[inline]
  pub const fn cmp<T: Number>(a: T, b: T) -> Ordering {
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
  pub const fn is_lt<T: Number>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less)
  }

  #[inline]
  pub const fn is_le<T: Number>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Less | Ordering::Equal)
  }

  #[inline]
  pub const fn is_gt<T: Number>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater)
  }

  #[inline]
  pub const fn is_ge<T: Number>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Greater | Ordering::Equal)
  }

  #[inline]
  pub const fn is_eq<T: Number>(a: T, b: T) -> bool {
    matches!(cmp(a, b), Ordering::Equal)
  }

  #[inline]
  pub const fn is_ne<T: Number>(a: T, b: T) -> bool {
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
  }

  impl<T: Number> Zero for T {
    const ZERO: Self = unsafe { core::mem::zeroed() };
  }

  impl<T: Number> Ones for T {
    const ONES: Self = not(Self::ZERO);
  }

  impl<T: Number> Default for T {
    const DEFAULT: Self = Self::ZERO;
  }

  impl<T: Number> Minimum for T {
    const MIN: Self = Self::ZERO;
  }

  impl<T: Number> Maximum for T {
    const MAX: Self = Self::ONES;
  }

  unsafe impl<T: Number> ptr::Pointee for T {
    type Metadata = ();
  }

  impl<T: Number> Term for T {
    type Backing = T;
    type Pattern = typed::Any<T>;
  }

  impl<T: Number> Lit<NumberSpec> for T {
    const VALUE: NumberSpec = NumberSpec {
      width: T::Bytes::LEN as u8,
      endian: T::Endian::VALUE,
      sign: sign::Sign::Unsigned,
    };
  }

  impl<T: Number> Lit<T> for () {
    const VALUE: T = T::ZERO;
  }

  unsafe impl Number for () {
    type Bytes = [u8; 0];
    type Endian = endian::Ne;
  }

  unsafe impl Number for usize {
    type Bytes = [u8; size_of::<Self>()];
    type Endian = endian::Ne;
  }

  unsafe impl<T: Number<Endian = endian::Ne>> Number for Le<T> {
    type Bytes = T::Bytes;
    type Endian = endian::Le;
  }

  unsafe impl<T: Number<Endian = endian::Ne>> Number for Be<T> {
    type Bytes = T::Bytes;
    type Endian = endian::Be;
  }

  macro_rules! adt_nums {
    { $($name:ident $size:literal $align:literal $raw:ident)+ } => {$(
      #[repr(align($align))]
      #[allow(non_camel_case_types)]
      #[derive(Copy, Clone, PartialEq, Eq, Hash)]
      pub struct $name([u8; $size]);

      impl_adt! { () $name[$raw] }
      impl_adt_from! { $name[$raw] }
    )+};
  }

  macro_rules! impl_num {
    { $($name:ident $size:literal)+ } => {$(
      unsafe impl Number for $name {
        type Bytes = [u8; $size];
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
    (($($generics:tt)*) $T:ty[$Raw:ty]) => {
      impl <$($generics)*> $T {
        #[inline]
        pub const fn from<U: Number>(n: U) -> Self {
          cast(n)
        }

        #[inline]
        pub const fn into<U: Number>(self) -> U {
          cast(self)
        }

        #[inline]
        pub const fn from_bytes(bytes: <Self as Number>::Bytes) -> Self {
          from_bytes(bytes)
        }

        #[inline]
        pub const fn to_bytes(self) -> <Self as Number>::Bytes {
          to_bytes(self)
        }

        #[inline]
        pub const fn swap_bytes(self) -> Self {
          swap_bytes(self)
        }

        #[inline]
        pub const fn from_ne_bytes(bytes: <Self as Number>::Bytes) -> Self {
          from_ne_bytes(bytes)
        }

        #[inline]
        pub const fn from_le_bytes(bytes: <Self as Number>::Bytes) -> Self {
          from_le_bytes(bytes)
        }

        #[inline]
        pub const fn from_be_bytes(bytes: <Self as Number>::Bytes) -> Self {
          from_be_bytes(bytes)
        }

        #[inline]
        pub const fn to_ne_bytes(self) -> <Self as Number>::Bytes {
          to_ne_bytes(self)
        }

        #[inline]
        pub const fn to_le_bytes(self) -> <Self as Number>::Bytes {
          to_le_bytes(self)
        }

        #[inline]
        pub const fn to_be_bytes(self) -> <Self as Number>::Bytes {
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

      impl <Rhs: Number, $($generics)*> Add<Rhs> for $T where $Raw: Add, <$Raw as Add>::Output: Number {
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

  impl_adt! { (T: Number<Endian = endian::Ne>) Le<T>[T] }

  impl_adt! { (T: Number<Endian = endian::Ne>) Be<T>[T] }
}

// -------------------------------------------------------------------------------------------------
// ptr

pub mod ptr {
  use core::mem;

  use super::num::Number;
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
    type Metadata: Number;
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
// Type

pub mod typed {
  use crate::{num, Lit};

  pub trait Pattern: Lit<super::BitsPattern> {
    type Lhs: Pattern;
    type Rhs: Pattern;
    type Int: num::Number;
    const TYPE: PatternType<Self::Int>;
  }

  #[derive(Copy, Clone, PartialEq, Eq, Hash)]
  pub enum PatternType<T: num::Number = ()> {
    Void,
    Any(usize),
    Within(super::ByteInterval),
    OneOf,
    And,
    Gte(T)
  }

  pub struct Void;
  pub struct Any<T, Rhs = Void>(T, Rhs);
  pub struct Within<const MASK: u8, const START: u8, const END: u8, Rhs = Void>(Rhs);
  pub struct OneOf<List>(List);
  pub struct And<List>(List);
  pub struct Gte<T, Val>(T, Val);

  impl Pattern for Void {
    type Lhs = Void;
    type Rhs = Void;
    type Int = ();
    const TYPE: PatternType<()> = PatternType::Void;
  }

  impl Lit<super::BitsPattern> for Void {
    const VALUE: super::BitsPattern = super::BitsPattern::Void;
  }

  impl<T: num::Number, Rhs: Pattern> Pattern for Any<T, Rhs> {
    type Lhs = Void;
    type Rhs = Rhs;
    type Int = T;
    const TYPE: PatternType<T> = PatternType::Any(size_of::<T>());
  }

  impl<T: num::Number, Rhs: Pattern> Lit<super::BitsPattern> for Any<T, Rhs> {
    const VALUE: super::BitsPattern = super::BitsPattern::Any(size_of::<T>(), &Rhs::VALUE);
  }

  impl<const MASK: u8, const START: u8, const END: u8, Rhs: Pattern> Pattern
    for Within<MASK, START, END, Rhs>
  {
    type Lhs = Void;
    type Rhs = Rhs;
    type Int = u8;
    const TYPE: PatternType<u8> = PatternType::Within(
      super::ByteInterval { mask: MASK, start: START, end: END }
    );
  }

  impl<const MASK: u8, const START: u8, const END: u8, Rhs: Pattern> Lit<super::BitsPattern>
    for Within<MASK, START, END, Rhs>
  {
    const VALUE: super::BitsPattern = super::BitsPattern::Within {
      interval: super::ByteInterval { mask: MASK, start: START, end: END },
      next: &Rhs::VALUE,
    };
  }

  impl<Val: Lit<u8>> Pattern for Gte<u8, Val> {
    type Lhs = Void;
    type Rhs = Void;
    type Int = u8;
    const TYPE: PatternType<u8> = PatternType::Gte(Val::VALUE);
  }

  impl<Val: Lit<u8>> Lit<super::BitsPattern> for Gte<u8, Val> {
    const VALUE: super::BitsPattern = super::BitsPattern::Within {
      interval: super::ByteInterval { mask: 0xff, start: Val::VALUE, end: 0xff },
      next: &super::BitsPattern::Void,
    };
  }

  impl<Val: Lit<u16>> Pattern for Gte<u16, Val> {
    type Lhs = Void;
    type Rhs = Void;
    type Int = u16;
    const TYPE: PatternType<u16> = PatternType::Gte(Val::VALUE);
  }

  /// for 0xabcd:
  ///   0xab + 1..0xff, 0x00..0xff
  /// | 0xab..0xab,     0xcd..0xff
  impl<Val: Lit<num::le16>> Lit<super::BitsPattern> for Gte<num::le16, Val> {
    const VALUE: super::BitsPattern = super::BitsPattern::OneOf(
      &super::BitsPattern::Within {
        interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[0] + 1, end: 0xff },
        next: &super::BitsPattern::Within {
          interval: super::ByteInterval { mask: 0xff, start: 0, end: 0xff },
          next: &super::BitsPattern::Void,
        },
      },
      &super::BitsPattern::Within {
        interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[0], end: Val::VALUE.to_bytes()[0] },
        next: &super::BitsPattern::Within {
          interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[1], end: 0xff },
          next: &super::BitsPattern::Void,
        },
      },
    );
  }

  /// for 0xabcd:
  ///   0x00..0xff, 0xab + 1..0xff
  /// | 0xcd..0xff, 0xab..0xab
  impl<Val: Lit<num::be16>> Lit<super::BitsPattern> for Gte<num::be16, Val> {
    const VALUE: super::BitsPattern = super::BitsPattern::OneOf(
      &super::BitsPattern::Within {
        interval: super::ByteInterval { mask: 0xff, start: 0, end: 0xff },
        next: &super::BitsPattern::Within {
          interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[0] + 1, end: 0xff },
          next: &super::BitsPattern::Void,
        },
      },
      &super::BitsPattern::Within {
        interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[1], end: 0xff },
        next: &super::BitsPattern::Within {
          interval: super::ByteInterval { mask: 0xff, start: Val::VALUE.to_bytes()[0], end: Val::VALUE.to_bytes()[0] },
          next: &super::BitsPattern::Void,
        },
      },
    );
  }

  // #[cfg(target_endian = "little")]
  // impl<Val: Lit<u16>> Lit<super::BitsPattern> for Gte<u16, Val> {
  //   const VALUE: super::BitsPattern = Gte::<num::le16, Val>::VALUE;
  // }

  macro_rules! impl_lists {
    { $List:ident $Head:ident $Tail:ident } => {
      impl<$Head: Pattern, $Tail: Pattern> Pattern for $List<($Head, $Tail)> {
        type Lhs = $Head;
        type Rhs = $Tail;
        type Int = ();
        const TYPE: PatternType<()> = PatternType::$List;
      }
      impl<$Head: Pattern, $Tail: Pattern> Lit<super::BitsPattern> for $List<($Head, $Tail)> {
        const VALUE: super::BitsPattern = super::BitsPattern::$List(&$Head::VALUE, &$Tail::VALUE);
      }
    };
    { $List:ident $Head:ident $($Tail:ident)+ } => {
      impl<$Head: Pattern, $($Tail: Pattern),+> Pattern for $List<($Head, $($Tail),+)> {
        type Lhs = $Head;
        type Rhs = $List<($($Tail),+)>;
        type Int = ();
        const TYPE: PatternType<()> = PatternType::$List;
      }
      impl<$Head: Pattern, $($Tail: Pattern),+> Lit<super::BitsPattern> for $List<($Head, $($Tail),+)> {
        const VALUE: super::BitsPattern =
          super::BitsPattern::$List(&$Head::VALUE, &$List::<($($Tail),+)>::VALUE);
      }
      impl_lists! { $List $($Tail)+ }
    };
  }

  impl_lists! { OneOf A B C D E F G H I J K L M N O P Q R S T U V W X Y Z }
  impl_lists! { And A B C D E F G H I J K L M N O P Q R S T U V W X Y Z }
}

pub mod marker {
  pub struct Sized;
  pub struct Unsized;
}

pub trait Term {
  type Backing: num::Number;
  type Pattern: typed::Pattern;
}

pub trait Field {
  type Type: Type + ?Sized;
  type Prev: SizedField;
  const IDX: usize;
  const OFF: usize;
}

pub trait SizedField: Field {
  type SizedType: Type;
}

pub trait MaybeUnsized {
  type Sizing;
}

pub trait Type<S = <Self as MaybeUnsized>::Sizing>: MaybeUnsized {
  type Term: Term;
  type Item: Type;
  type TailField: Field;
  const LAYOUT: Layout;
  const SIZED: &'static [BytePattern];
  const TAIL: Option<&'static [BytePattern]> = <Self::TailField as Field>::Type::TAIL;
}

const _: () = {
  use num::*;
  use core::num::{NonZeroU16, NonZeroU8, NonZeroU32, NonZeroU64, NonZeroU128};

  impl<T: Term> MaybeUnsized for T {
    type Sizing = marker::Sized;
  }

  impl Term for bool {
    type Backing = u8;
    type Pattern = typed::Within<0xff, 0, 0b1>;
  }

  impl Term for NonZeroU8 {
    type Backing = u8;
    type Pattern = typed::Within<0xff, 1, 0xff>;
  }

  impl Term for NonZeroU16 {
    type Backing = u16;
    type Pattern = typed::OneOf<(
      typed::Within<0xff, 1, 0xff, typed::Any<u8>>,
      typed::Any<u8, typed::Within<0xff, 1, 0xff>>,
    )>;
  }

  impl Term for NonZeroU32 {
    type Backing = u32;
    type Pattern = typed::OneOf<(
      typed::Within<0xff, 1, 0xff, typed::Any<u24>>,
      typed::Any<u8, typed::Within<0xff, 1, 0xff, typed::Any<u16>>>,
      typed::Any<u16, typed::Within<0xff, 1, 0xff, typed::Any<u8>>>,
      typed::Any<u24, typed::Within<0xff, 1, 0xff>>,
    )>;
  }

  impl Term for NonZeroU64 {
    type Backing = u64;
    type Pattern = typed::OneOf<(
      typed::Within<0xff, 1, 0xff, typed::Any<u56>>,
      typed::Any<u8, typed::Within<0xff, 1, 0xff, typed::Any<u48>>>,
      typed::Any<u16, typed::Within<0xff, 1, 0xff, typed::Any<u40>>>,
      typed::Any<u24, typed::Within<0xff, 1, 0xff, typed::Any<u32>>>,
      typed::Any<u32, typed::Within<0xff, 1, 0xff, typed::Any<u24>>>,
      typed::Any<u40, typed::Within<0xff, 1, 0xff, typed::Any<u16>>>,
      typed::Any<u48, typed::Within<0xff, 1, 0xff, typed::Any<u8>>>,
      typed::Any<u56, typed::Within<0xff, 1, 0xff>>,
    )>;
  }

  impl Term for NonZeroU128 {
    type Backing = u128;
    type Pattern = typed::OneOf<(
      typed::Within<0xff, 1, 0xff, typed::Any<u120>>,
      typed::Any<u8, typed::Within<0xff, 1, 0xff, typed::Any<u112>>>,
      typed::Any<u16, typed::Within<0xff, 1, 0xff, typed::Any<u104>>>,
      typed::Any<u24, typed::Within<0xff, 1, 0xff, typed::Any<u96>>>,
      typed::Any<u32, typed::Within<0xff, 1, 0xff, typed::Any<u88>>>,
      typed::Any<u40, typed::Within<0xff, 1, 0xff, typed::Any<u80>>>,
      typed::Any<u48, typed::Within<0xff, 1, 0xff, typed::Any<u72>>>,
      typed::Any<u56, typed::Within<0xff, 1, 0xff, typed::Any<u64>>>,
      typed::Any<u64, typed::Within<0xff, 1, 0xff, typed::Any<u56>>>,
      typed::Any<u72, typed::Within<0xff, 1, 0xff, typed::Any<u48>>>,
      typed::Any<u80, typed::Within<0xff, 1, 0xff, typed::Any<u40>>>,
      typed::Any<u88, typed::Within<0xff, 1, 0xff, typed::Any<u32>>>,
      typed::Any<u96, typed::Within<0xff, 1, 0xff, typed::Any<u24>>>,
      typed::Any<u104, typed::Within<0xff, 1, 0xff, typed::Any<u16>>>,
      typed::Any<u112, typed::Within<0xff, 1, 0xff, typed::Any<u8>>>,
      typed::Any<u120, typed::Within<0xff, 1, 0xff>>,
    )>;
  }

  impl Term for Option<NonZeroU8> {
    type Backing = u8;
    type Pattern = typed::Any<u8>;
  }

  impl Term for Option<NonZeroU16> {
    type Backing = u16;
    type Pattern = typed::Any<u16>;
  }

  impl Term for Option<NonZeroU32> {
    type Backing = u32;
    type Pattern = typed::Any<u32>;
  }

  impl Term for Option<NonZeroU64> {
    type Backing = u64;
    type Pattern = typed::Any<u64>;
  }

  impl Term for Option<NonZeroU128> {
    type Backing = u128;
    type Pattern = typed::Any<u128>;
  }

  impl Term for char {
    type Backing = u32;
    type Pattern = typed::OneOf<(
      // Range 1: U+0000 to U+D7FF
      typed::Within<0xFF, 0x00, 0x00, // Byte 0 is 0x00
        typed::Within<0xFF, 0x00, 0x00, // Byte 1 is 0x00
          typed::Within<0xFF, 0x00, 0xD7, // Byte 2 is 0x00 to 0xD7
            typed::Any<u8>>>>, // Byte 3 can be any value

      // Range 2: U+E000 to U+FFFF
      typed::Within<0xFF, 0x00, 0x00, // Byte 0 is 0x00
        typed::Within<0xFF, 0x00, 0x00, // Byte 1 is 0x00
          typed::Within<0xFF, 0xE0, 0xFF, // Byte 2 is 0xE0 to 0xFF
            typed::Any<u8>>>>, // Byte 3 can be any value

      // Range 3: U+10000 to U+10FFFF
      typed::Within<0xFF, 0x00, 0x00, // Byte 0 is 0x00
        typed::Within<0xFF, 0x01, 0x10, // Byte 1 is 0x01 to 0x10
          typed::Any<u8, // Byte 2 can be any value
            typed::Any<u8>>>> // Bytes 3 can be any value
    )>;
  }
};

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ByteInterval {
  mask: u8,
  start: u8,
  end: u8,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum BitsPattern {
  Void,
  Any(usize, &'static BitsPattern),
  Within { interval: ByteInterval, next: &'static BitsPattern },
  OneOf(&'static BitsPattern, &'static BitsPattern),
  And(&'static BitsPattern, &'static BitsPattern),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum BytePattern {
  Uninit,
  Initialized { offset: usize, pattern: &'static BitsPattern },
}

pub trait Data {
  const PATTERNS: &'static [BytePattern];
}

const ANY: &'static BitsPattern = &BitsPattern::Within {
  interval: ByteInterval { mask: 0xff, start: 0, end: 0xff },
  next: &BitsPattern::Void,
};

impl Data for bool {
  const PATTERNS: &'static [BytePattern] = &[BytePattern::Initialized {
    offset: 0,
    pattern: &BitsPattern::Within {
      interval: ByteInterval { mask: 0b1, start: 0, end: 0b1 },
      next: &BitsPattern::Void,
    },
  }];
}

impl Data for u8 {
  const PATTERNS: &'static [BytePattern] = &[BytePattern::Initialized { offset: 0, pattern: ANY }];
}

impl Data for u16 {
  const PATTERNS: &'static [BytePattern] = &[
    BytePattern::Initialized { offset: 0, pattern: ANY },
    BytePattern::Initialized { offset: 0, pattern: ANY },
  ];
}

struct Yolo {
  a: bool,
  b: u8,
  c: u16,
}

unsafe impl ptr::Pointee for Yolo {
  type Metadata = ();
}

impl Data for Yolo {
  const PATTERNS: &'static [BytePattern] = &{
    let fields = [
      (offset_of!(Yolo, a), size_of::<bool>(), <bool as Data>::PATTERNS),
      (offset_of!(Yolo, b), size_of::<u8>(), <u8 as Data>::PATTERNS),
      (offset_of!(Yolo, c), size_of::<u16>(), <u16 as Data>::PATTERNS),
    ];

    let mut patterns = [BytePattern::Uninit; size_of::<Yolo>()];
    let mut i = 0;
    while i < fields.len() {
      let (offset, size, field) = fields[i];
      let mut j = 0;
      while j < size {
        patterns[offset + j] = field[j];
        j += 1;
      }
      i += 1;
    }

    patterns
  };
}

pub enum Layout {
  Term,
  Array,
  Slice,
  Struct,
}

fn main() {}
