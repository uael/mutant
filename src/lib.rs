//! ***Efficient, safe, transmutation. Even for dynamically-sized types.***
//!
//! # Example
//!
//! Here's a simple example demonstrating how to use mutant:
//!
//! ```rust
//! use mutant::{Data, num::u16le};
//!
//! #[derive(Data, Eq, PartialEq, Debug)]
//! #[repr(C)]
//! struct Loot<T: ?Sized> {
//!   position: (u8, u8),
//!   player: u16le,
//!   items: T,
//! }
//!
//! #[derive(Copy, Clone, Data, Eq, PartialEq, Debug)]
//! #[repr(C)]
//! struct Item {
//!   archetype: u8,
//!   legendary: bool,
//! }
//!
//! // Create a `Loot` instance with some items.
//! let mut loot = Loot {
//!   position: (21, 12),
//!   player: 0xcafe.into(),
//!   items: [
//!     Item { archetype: 0x42, legendary: true },
//!     Item { archetype: 0x21, legendary: false },
//!   ]
//! };
//!
//! #[derive(Copy, Clone, Data, Eq, PartialEq, Debug)]
//! #[repr(C)]
//! struct Position { x: u8, y: u8 }
//!
//! // Transmute the `position` field to a `Position` struct.
//! assert_eq!(&Position { x: 21, y: 12 }, mutant::transmute_ref(&loot.position));
//! assert_eq!(Position { x: 21, y: 12 }, mutant::transmute(loot.position)); // Position is `Copy`
//!
//! // Modify the `position` field in place.
//! let mut position: &mut Position = mutant::transmute_mut(&mut loot.position);
//! position.x *= 2;
//!
//! // Transmute the entire `Loot` struct to bytes.
//! let loot: &Loot<[Item]> = &loot;
//! let loot_bytes: &[u8] = mutant::transmute_ref(loot);
//! assert_eq!(
//!   &[21 * 2, 12, 0xfe, 0xca, 0x42, 1, 0x21, 0],
//!   loot_bytes
//! );
//!
//! // This fail to compile as it requires runtime checks.
//! // let loot2: &Loot<[Item]> = mutant::transmute_ref(loot_bytes);
//!
//! // Transmute the bytes back to a `Loot` struct.
//! let loot2: Result<&Loot<[Item]>, _> = mutant::try_transmute_ref(loot_bytes);
//! assert_eq!(Ok(loot), loot2);
//! ```
//!
//! Transmute is checked at compile time:
//! ```rust,compile_fail
//! let _: &mut u8 = mutant::transmute_mut(&mut true);
//! ```
//!
//! # Technical terms
//!
//! ### DST
//! According to the [Rust reference]:
//! > *Most types have a fixed size that is known at compile time and implement the trait [`Sized`].
//! > A type with a size that is known only at run-time is called a dynamically sized type (DST)
//! > or, informally, an unsized type. [Slices] and [trait objects] are two examples of DSTs.*
//!
//! Where `mutant` support [Slices], structs with a [tail slice](#tail-slice), and
//! [maybe DST](#maybe-dst) struct types, but not [trait objects]. Enums can't be DST as they
//! aren't allowed to contain any DST, i.e. enums are always [`Sized`] where struct may not.
//!
//! [Rust reference]: https://doc.rust-lang.org/reference/dynamically-sized-types.html
//! [Slices]: https://doc.rust-lang.org/reference/types/slice.html
//! [trait objects]: https://doc.rust-lang.org/reference/types/trait-object.html
//!
//! #### Tail slice
//! The following `TailSliceDst` struct is a DST because the size of `data` is only known at
//! runtime, hence not [`Sized`]:
//! ```rust
//! struct TailSliceDst {
//!   info: u32,
//!   data: [u8], // `tail slice` of type `[u8]`.
//! }
//! ```
//!
//! **But** according to the [Rust reference]:
//! > *Structs can actually store a single DST directly as their last field, but this makes them a
//! > DST as well.*
//!
//! The following `TailDst` struct is also a DST, **but** with a [tail](#tail) DST instead:
//! ```rust
//! struct TailSliceDst {
//!   info: u32,
//!   data: [u8], // `tail slice` of type `[u8]`.
//! }
//!
//! struct TailDst {
//!   version: u32,
//!   payload: TailSliceDst, // `tail` DST of type `TailSliceDst`.
//! }
//! ```
//!
//! In `mutant`, the [tail slice](#tail-slice) term **always** refer to the **last slice** of a
//! [DST](#dst):
//! - The [tail slice](#tail-slice) type of `TailSliceDst` is `[u8]`.
//! - The [tail slice](#tail-slice) type of `TailDst` is **also** `[u8]`.
//!
//! #### Maybe DST
//! The relaxed [`?Sized`](Sized) bound allow for types that are maybe [DST](#dst):
//! ```rust
//! struct MaybeDst<T: ?Sized> {
//!   info: u32,
//!   data: T, // `tail` of type `T`.
//! }
//!
//! type Dst = MaybeDst<[u8]>; // Is a `DST`.
//! type NotDst = MaybeDst<u32>; // Not a `DST`.
//! ```
//!
//! ### Tail
//! In `mutant`, the [tail](#tail) term **always** refer to **last field** of a struct, **either for
//! [DST](#dst) and [`Sized`]**.
//! As per the examples above in the [tail slice](#tail-slice) section:
//! - The [tail](#tail) of `TailSliceDst` is slice of type `[u8]`.
//! - The [tail](#tail) of `TailDst` is a [DST](#dst) of type `TailSliceDst`.
//!
#![cfg_attr(not(test), no_std)]

// mod bytes;
mod bytes;
#[cfg(test)]
mod tests;

use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;

// re-export `mutant_derive` derive macros.
pub use mutant_derive::Data;
pub use mutant_derive::DebugPacked;
pub use mutant_derive::PartialEqPacked;

// Utility macros ----------------------------------------------------------------------------------

/// Unwrap a `Result` and return the error if it is `Err`.
#[rustfmt::skip]
macro_rules! bail {
  ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => return Err(e) });
  ($e:expr, $x:ident => $map:expr $(,)?) => (match $e { Ok(r) => r, Err($x) => return Err($map) });
}

/// Check a condition and return an error if it is `false`.
#[rustfmt::skip]
macro_rules! check { ($c:expr, $e:expr $(,)?) => { if (!$c) { return Err($e); } } }

/// Unwrap an `Option` and panic if it is `None`.
#[rustfmt::skip]
macro_rules! unwrap {
  ($e:expr, $none:expr $(,)?) => (match $e { Some(r) => r, None => $none });
  ($e:expr $(,)?) => (unwrap!($e, unreachable!()));
}

/// Unwrap a `Result` and panic with the error if it is `Err`.
#[rustfmt::skip]
macro_rules! unwrap_err { ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => panic!("{}", e) }) }

/// Same as iter.fold() but for const fns.
#[rustfmt::skip]
macro_rules! fold {
  ($iter:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $iter.len(), $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    fold!($iter, $iter.len(), $init, |$i, $acc, $it| $x)
  }};
  ($iter:expr, $len:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $len, $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $len:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    let (mut $i, mut $acc) = (0, $init);
    loop {
      if $i >= $len { break $acc; }
      let $it = &$iter[$i];
      ($acc, $i) = ($x, $i + 1);
    }
  }};
}

/// Same as iter.any() but for const fns.
#[rustfmt::skip]
macro_rules! any {
  ($iter:expr, |$it:tt| $x:expr) => (
    any!($iter, $iter.len(), |__i, $it| $x)
  );
  ($iter:expr, |$i:tt, $it:tt| $x:expr) => {{
    any!($iter, $iter.len(), |$i, $it| $x)
  }};
  ($iter:expr, $len:expr, |$it:tt| $x:expr) => (
    any!($iter, $len, |__i, $it| $x)
  );
  ($iter:expr, $len:expr, |$i:tt, $it:tt| $x:expr) => {{
    let mut $i = 0;
    loop {
      if $i >= $len { break false; }
      let $it = &$iter[$i];
      if $x { break true; }
    }
  }};
}

#[rustfmt::skip]
macro_rules! each {
  ($start:literal..$end:expr, |$it:tt| $x:expr) => {{
    let mut $it = $start; while $it < $end { $x; $it += 1; }; $it
  }};
  ($start:literal..=$end:expr, |$it:tt| $x:expr) => {{
    let mut $it = $start; while $it <= $end { $x; $it += 1; }; $it
  }};
  (..$end:expr, |$it:tt| $x:expr) => {{
    let mut $it = 0; while $it < $end { $x; $it += 1; }; $it
  }};
  (..=$end:expr, |$it:tt| $x:expr) => {{
    let mut $it = 0; while $it <= $end { $x; $it += 1; }; $it
  }};
  ($iter:expr, |$it:tt| $x:expr) => {{
    let mut __i = 0; while __i < $iter.len() { let $it = &$iter[__i]; $x; __i += 1; }; __i
  }};
}

macro_rules! array {
  ($init:expr, $iter:expr, |$it:tt| $x:expr) => {{
    let mut __array = $init;
    let mut __i = 0;
    while __i < $iter.len() {
      let $it = &$iter[__i];
      __array[__i] = $x;
      __i += 1;
    }
    __array
  }};
}

pub(crate) use bail;
pub(crate) use each;
pub(crate) use fold;
pub(crate) use unwrap;

// Internal public API -----------------------------------------------------------------------------
// Even if part of the public API, the following items are not meant to be used directly by the
// user. They are used internally by the derive macro and other parts of the library and are subject
// to change without notice.
// -------------------------------------------------------------------------------------------------

/// Represents the layout of a type in memory.
///
/// The [`Layout`] enum is used to categorize the memory representation of a type, which is
/// essential for safe and correct transmutation. It describes how data is laid out in memory,
/// providing the necessary context for transmutation operations.
#[doc(hidden)]
#[rustfmt::skip]
#[derive(Copy, Clone)]
pub enum Layout<T: ?Sized + Data> {
  Slice, Array, Struct, Integer { spec: IntegerSpec<<T::Integer as Integer>::Repr>, },
}

#[doc(hidden)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone)]
pub enum SizeField { Size }

#[doc(hidden)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BytePattern<'a> {
  Uninit,
  All(&'a [BytePattern<'a>]), Any(&'a [BytePattern<'a>]),
  Equ(&'a [u8]), Gte(&'a [u8]), Cat(&'a [(u8, u8)])
}

#[doc(hidden)]
#[derive(Debug, Copy, Clone)]
pub struct IntegerSpec<T> {
  default: [u8; 16],
  endianness: internal::Endianness,
  byte_pattern: BytePattern<'static>,
  size_field: Option<SizeField>,
  _type: PhantomData<T>,
}

/// This is used as a compile-time field list for struct layout.
#[doc(hidden)]
pub unsafe trait Field {
  type Type: ?Sized + Data;
  type PrevField: Field;
  const OFFSET: usize;
  const INDEX: usize;
}

#[doc(hidden)]
#[repr(transparent)]
pub struct DataField<C: ?Sized + Data, T: ?Sized + Data>(PhantomData<C>, T);

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
    let ptr = $crate::ptr::from_raw_parts::<$Container>($crate::BIG_ALLOCATION, 0);
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

macro_rules! decl_pack {
  ($($name:ident ($($t:ident)* =$tt:ident))+) => {$(
    #[doc(hidden)]
    #[repr(C, packed)]
    #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone)]
    #[data(crate)]
    pub struct $name<$($t,)* $tt: Data + ?Sized>($(pub $t,)* pub ManuallyDrop<$tt>);

    impl<$($t: Copy + Eq,)* $tt: Data + Eq + ?Sized> Eq for $name<$($t,)* $tt> {}
  )+};
}

decl_pack! {
  Pack1 (=A)
  Pack2 (A =B)
  Pack3 (A B =C)
  Pack4 (A B C =D)
  Pack5 (A B C D =E)
  Pack6 (A B C D E =F)
  Pack7 (A B C D E F =G)
  Pack8 (A B C D E F G =H)
  Pack9 (A B C D E F G H =I)
}

/// Assert at compile-time that a type is unaligned (i.e. his alignment is equals to 1).
#[inline]
#[doc(hidden)]
#[rustfmt::skip]
pub const fn const_assert_unaligned<T: ?Sized>() {
  struct Assert<T: ?Sized>(*const T);
  impl<T: ?Sized> Assert<T> { const ASSERT_UNALIGNED: () = assert!(align_of::<T>() == 1); }
  #[allow(clippy::let_unit_value)]
    let _ = Assert::<T>::ASSERT_UNALIGNED;
}

/// Assert at compile-time that a type is unaligned (i.e. his alignment is equals to 1).
#[inline]
#[doc(hidden)]
#[rustfmt::skip]
pub const fn const_assert_unaligned_raw<T: ?Sized>(_: *const T) {
  const_assert_unaligned::<T>();
}

/// Identity function used by the [`layout`] macro.
#[inline]
#[doc(hidden)]
pub const fn raw<T: Integer>(raw: T::Repr) -> T::Repr {
  assert!(mem::size_of::<T>() == mem::size_of::<T::Repr>());
  raw
}

/// Constants used to declare bits pattern for [`InitLayout`].
#[doc(hidden)]
pub mod bits {
  pub const UNINIT: u8 = 0;
  pub const ANY: u8 = 0b1;
  pub const ALLOW: u8 = 0b10;
  pub const DISALLOW: u8 = 0b100;
}

#[doc(hidden)]
pub mod ptr {
  use core::mem;

  /// Convert a reference to a raw pointer.
  ///
  /// This is equivalent to `r as *const T`, but is a bit safer since it will never silently change
  /// type or mutability, in particular if the code is refactored.
  #[inline]
  #[must_use]
  pub const fn from_ref<T: ?Sized>(r: &T) -> *const T {
    r
  }

  /// Convert a mutable reference to a raw pointer.
  ///
  /// This is equivalent to `r as *mut T`, but is a bit safer since it will never silently change
  /// type or mutability, in particular if the code is refactored.
  #[inline]
  #[must_use]
  pub fn from_mut<T: ?Sized>(r: &mut T) -> *mut T {
    r
  }

  /// Extract the metadata component of a pointer.
  #[inline]
  pub const fn metadata<T: ?Sized>(ptr: *const T) -> usize {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline]
  pub const fn from_raw_parts<T: ?Sized>(address: *const (), metadata: usize) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline]
  pub const fn from_raw_parts_mut<T: ?Sized>(address: *mut (), metadata: usize) -> *mut T {
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
  union PtrRepr<T: ?Sized> {
    const_ptr: *const T,
    mut_ptr: *mut T,
    components: PtrComponents,
  }

  #[repr(C)]
  #[derive(Copy, Clone)]
  struct PtrComponents {
    address: *const (),
    metadata: usize,
  }

  impl<T: ?Sized> PtrRepr<T> {
    #[inline]
    const fn empty() -> Self {
      Self::empty_with(core::ptr::null())
    }

    #[inline]
    const fn empty_with(address: *const ()) -> Self {
      Self::with_components(address, 0)
    }

    #[inline]
    const fn with_components(address: *const (), metadata: usize) -> Self {
      Self { components: PtrComponents { address, metadata } }
    }

    #[inline]
    const fn set_const(mut self, const_ptr: *const T) -> Self {
      self.const_ptr = const_ptr;
      self
    }
  }
}

// Public API --------------------------------------------------------------------------------------

/// Represents errors that can arise within `mutant` operations.
///
/// This type enumerates all possible error conditions in `mutant`,
/// providing detailed information for debugging and error handling.
/// It is primarily used in the context of type transmutation, memory layout
/// manipulation, and alignment assertions.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Error {
  /// Indicates a mismatch in alignment requirements between the source and target types.
  ///
  /// This error is raised when attempting to transmute between two types where the alignment
  /// of the target type is not compatible with the alignment of the source type.
  /// Alignment mismatches can lead to undefined behavior, so it's crucial to ensure alignment
  /// compatibility during transmutation.
  AlignmentMismatch,
  /// Indicates a mismatch in size between the source and target types.
  ///
  /// This error occurs when the size of the source type does not match the size of the
  /// target type. The inner value (`isize`) represents the size difference.
  SizeMismatch(isize),
  /// Source bits pattern is invalid for target.
  InvalidBitPattern,
}

/// A wrapper to un-align types that originally require alignment (align > 1).
///
/// `Unalign` is used to safely encapsulate types within structures while
/// disregarding their original alignment requirements. This is particularly
/// useful when working with types that inherently have an alignment greater
/// than 1, ensuring that they can be placed in memory without alignment constraints.
///
/// # Examples
///
/// ```
/// use mutant::Unalign;
///
/// // Check the alignment of a regular u32
/// assert_eq!(4, mutant::align_of::<u32>());
///
/// // Check the alignment of an u32 wrapped with `Unalign`
/// assert_eq!(1, mutant::align_of::<Unalign<u32>>());
///
/// let byte: Unalign<u8> = 123.into();
/// assert_eq!(123, byte.into_inner()); // `u8` is `Copy`
/// assert_eq!(123, *byte); // `u8` is already unaligned, `byte` can be  `deref`.
/// ```
///
/// ```compile_fail
/// let uint: Unalign<u32> = 123.into();
/// assert_eq!(123, uint.into_inner()); // `u32` is `Copy`
/// assert_eq!(123, *uint); // this does not compile as it may result in an unaligned read.
/// ```
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default, Data)]
#[data(crate)]
#[repr(C, packed)]
pub struct Unalign<T>(ManuallyDrop<T>);

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Size<T: Integer> {
  inner: T,
}

/// Marker trait for plain integer types (e.g. integers or enum discriminants).
///
/// # Safety
/// This marker trait isn't meant to be implemented by the user. If you do so, you're on your own.
pub unsafe trait Integer: Copy + 'static {
  /// Representation of `Self` that can accept any bit pattern.
  /// The size of `Self::Repr` is equals to the size of `Self`.
  type Repr: Integer<Repr = Self::Repr>;

  /// # Safety
  /// Implementor is responsible for `SPEC` to represents only valid values for `Self`.
  #[doc(hidden)]
  const SPEC: IntegerSpec<Self::Repr>;
}

/// Marker trait for "data" types. "data" types are types that can be safely transmuted between each
/// other. They contain no references, pointers or drops types, but they can contain uninitialized
/// bytes.
///
/// # Safety
/// This marker trait isn't meant to be implemented by the user. If you do so, you're on your own.
pub unsafe trait Data: 'static {
  /// Item type of `Self`, as of:
  /// - `Self` is a slice `[T]`; set to `T`.
  /// - `Self` is an array `[T; N]`; set to `T`.
  /// - Otherwise (`Self` is an integer or a struct); set to [`()`] instead.
  type Item: Copy + Data;

  /// Item type of the [tail slice](#tail-slice) of `Self`, as of:
  /// - `Self` is a [DST](#dst); set to the item type of the [tail slice](#tail-slice) of `Self`
  /// - otherwise (`Self` is [`Sized`]); set to [`()`] instead.
  type TailSliceItem: Copy + Data;

  /// [`Sized`] representation of `Self`, equals to `Self` for [`Sized`] types.
  #[doc(hidden)]
  type Sized: Copy + Data;

  /// Fields of `Self`, only in case of [`Layout::Struct`] layout.
  #[doc(hidden)]
  type TailField: Field;

  /// [`Integer`] representation of `Self`, only in case of [`Layout::Integer`] layout.
  #[doc(hidden)]
  type Integer: Integer;

  type Container: ?Sized + Data;

  /// Layout of `Self`.
  #[doc(hidden)]
  const LAYOUT: Layout<Self>;
}

/// Pack up to 9 types into a single packed type (i.e. without padding bytes).
#[macro_export]
#[allow(non_snake_case)]
macro_rules! Pack {
  ($($t:ty),+ $(,)?) => { $crate::Pack!(@ $($t),+, ($($t),+)) };
  (@ $a:ty, ($($t:ty),+)) => { $crate::Pack1<$($t),+> };
  (@ $a:ty, $b:ty, ($($t:ty),+)) => { $crate::Pack2<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, ($($t:ty),+)) => { $crate::Pack3<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, ($($t:ty),+)) => { $crate::Pack4<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, $e:ty, ($($t:ty),+)) => { $crate::Pack5<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty, ($($t:ty),+)) => { $crate::Pack6<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty, $g:ty, ($($t:ty),+)) => { $crate::Pack7<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty, $g:ty, $h:ty, ($($t:ty),+)) => { $crate::Pack8<$($t),+> };
  (@ $a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty, $g:ty, $h:ty, $i:ty, ($($t:ty),+)) => { $crate::Pack9<$($t),+> };
}

/// Pack up to 9 values into a single packed value (i.e. without padding bytes).
#[macro_export]
macro_rules! pack {
  ($a:expr $(,)?) => {
    $crate::Pack1(::core::mem::ManuallyDrop::new($a))
  };
  ($a:expr, $b:expr $(,)?) => {
    $crate::Pack2($a, ::core::mem::ManuallyDrop::new($b))
  };
  ($a:expr, $b:expr, $c:expr $(,)?) => {
    $crate::Pack3($a, $b, ::core::mem::ManuallyDrop::new($c))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {
    $crate::Pack4($a, $b, $c, ::core::mem::ManuallyDrop::new($d))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {
    $crate::Pack5($a, $b, $c, $d, ::core::mem::ManuallyDrop::new($e))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr $(,)?) => {
    $crate::Pack6($a, $b, $c, $d, $e, ::core::mem::ManuallyDrop::new($f))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr $(,)?) => {
    $crate::Pack7($a, $b, $c, $d, $e, $f, ::core::mem::ManuallyDrop::new($g))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr, $h:expr $(,)?) => {
    $crate::Pack8($a, $b, $c, $d, $e, $f, $g, ::core::mem::ManuallyDrop::new($h))
  };
  ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr, $h:expr, $i:expr $(,)?) => {
    $crate::Pack9($a, $b, $c, $d, $e, $f, $g, $h, ::core::mem::ManuallyDrop::new($i))
  };
}

/// Safely reference unaligned data.
///
/// # Examples
/// ```
/// use mutant::unaligned_ref;
///
/// #[repr(C, packed)]
/// struct Unaligned {
///   a: u8,
///   b: u16,
///   c: u8,
/// }
///
/// let unaligned = &Unaligned { a: 1, b: 2, c: 3 };
/// // `unaligned.a` alignment is 1, hence safe to reference:
/// assert_eq!(&1, unaligned_ref!(unaligned.a));
/// // But `unaligned.b` alignment is 2, hence not safe to reference directly:
/// // let b = unaligned_ref!(unaligned.b); // this does not compile.
/// // Instead, use `unaligned_ref!` with `in` syntax to safely reference `b`:
/// let b;
/// assert_eq!(&2, unaligned_ref!(unaligned.b in b));
/// ```
#[macro_export]
macro_rules! unaligned_ref {
  ($base:expr, $($fields:tt).+) => {{
    let base: &_ = $base;
    let addr = core::ptr::addr_of!(base.$($fields).+);
    $crate::const_assert_unaligned_raw(addr);
    // SAFETY:
    // - `addr` came from valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned_raw` ensure field un-alignment.
    unsafe { &*addr }
  }};
  ($base:expr, $($fields:tt).+ in $sized:ident) => {{
    let base: &_ = $base;
    let mut addr = core::ptr::addr_of!(base.$($fields).+);
    if $crate::is_sized_val_raw(addr) && $crate::align_of_val_raw(addr) > 1 {
      #[inline] // A function for typing inference.
      const fn downcast<T: ?Sized + $crate::Data>(ptr: *const T) -> *const T::Sized { ptr.cast() }

      // `T` is `Sized` hence `T::Sized` is equals to `T`:
      // 1. Load `T::Sized` to stack.
      // SAFETY: `addr` came from valid reference hence not a dangling pointer.
      $sized = unsafe { ::core::ptr::read_unaligned(downcast(addr)) };
      // 2. Make `addr` point to `sized` instead.
      addr = $crate::ptr::from_raw_parts($crate::ptr::from_ref(&$sized).cast(), 0);
    } else {
      // TODO: make this a compile-time check.
      // `T` is `!Sized`: assert that `T` has no alignment requirement.
      assert!($crate::align_of_val_raw(addr) == 1);
    }
    // SAFETY:
    // - `addr` came from valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
  ($base:tt.$field:tt.$($fields:tt).+ $(in $sized:ident)?) => {{
    let base = $crate::unaligned_ref!($base.$field);
    $crate::unaligned_ref!(base.$($fields).+ $(in $sized)?)
  }};
  ($base:tt.$field:tt $(in $sized:ident)?) => (
    $crate::unaligned_ref!($base, $field $(in $sized)?)
  );
}

/// Safely mutably reference unaligned data.
#[macro_export]
macro_rules! unaligned_mut {
  ($base:expr, $($fields:tt).+) => {{
    let base: &mut _ = $base;
    let addr = core::ptr::addr_of_mut!(base.$($fields).+);
    $crate::const_assert_unaligned_raw(addr);
    // SAFETY:
    // - `addr` came from valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned_raw` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_mut!($base.$field);
    $crate::unaligned_mut!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => ($crate::unaligned_mut!($base, $field));
}

// TODO: refactor to a proc-macro
#[macro_export]
macro_rules! unpack {
  ($bytes:ident { $(| $($ty:ty)|+ => |$var:tt| $ex:stmt,)+ _ => $els:stmt $(,)? }) => {{
    let mut __bytes = $bytes;
    let e = $(
      $(
        if let Ok(($var, _bytes)) = $crate::unpack::<$ty>(__bytes) {
          __bytes = _bytes;
          $ex
        } else
      )+
    )+ { $els };
    (e, __bytes)
  }};
}

/// Same as [`align_of`](mem::align_of), returns the minimum alignment of a type in bytes.
/// With the difference that it also accept unsized types with relaxed [`?Sized`](Sized) bound.
///
/// # Examples
/// ```
/// assert_eq!(4, mutant::align_of::<i32>());
/// assert_eq!(4, mutant::align_of::<[i32]>()); // Also accept `?Sized` types.
/// ```
#[inline]
pub const fn align_of<T: ?Sized>() -> usize {
  #[repr(C)]
  struct AlignOf<T: ?Sized>(u8, T);

  // `AlignOf` is `repr(C)`, and its layout is guaranteed [1] to begin
  // with the single-byte layout for `AlignOf::0`, followed by the padding
  // needed to align `AlignOf::1`, then the layout for `AlignOf::1`, and
  // finally any trailing padding bytes needed to correctly-align the entire
  // struct.
  //
  // This macro computes the alignment of `$ty` by counting the number of
  // bytes preceding `AlignOf::1`. For instance, if the alignment of `$ty`
  // is `1`, then no padding is required align `AlignOf::1` and it will be
  // located immediately after `AlignOf::0` at offset 1. If the alignment of
  // `$ty` is 2, then a single padding byte is required before
  // `AlignOf::1`, and `AlignOf::1` will be located at offset 2.
  //
  // [1]: https://doc.rust-lang.org/nomicon/other-reprs.html#reprc
  offset_of!(AlignOf<T>, 1)
}

/// Returns the minimum alignment of a value in bytes. Also accept unsized types with
/// relaxed [`?Sized`](Sized) bound.
#[inline]
pub const fn align_of_val<T: ?Sized>(_: &T) -> usize {
  align_of::<T>()
}

/// Returns the minimum alignment of a value address in bytes. Also accept unsized types with
/// relaxed [`?Sized`](Sized) bound.
#[inline]
pub const fn align_of_val_raw<T: ?Sized>(_: *const T) -> usize {
  align_of::<T>()
}

#[inline]
pub fn as_mut_slice<T: ?Sized + Data>(val: &mut T) -> &mut [T::Item] {
  let ptr: *mut T = val;
  let len = match T::LAYOUT {
    Layout::Slice | Layout::Array if mem::size_of::<T::Item>() == 0 => return &mut [],
    Layout::Slice => ptr::metadata(val),
    Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
    _ => return &mut [],
  };
  unsafe { &mut *ptr::from_raw_parts_mut(ptr.cast(), len) }
}

#[inline]
pub const fn as_slice<T: ?Sized + Data>(val: &T) -> &[T::Item] {
  let ptr: *const T = val;
  let len = match T::LAYOUT {
    Layout::Slice | Layout::Array if mem::size_of::<T::Item>() == 0 => return &[],
    Layout::Slice => ptr::metadata(val),
    Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
    _ => return &[],
  };
  unsafe { &*ptr::from_raw_parts(ptr.cast(), len) }
}

/// Returns whatever an address to `T` is correctly aligned.
#[inline]
pub const fn is_aligned_raw<T: ?Sized + Data>(val: *const T) -> bool {
  is_aligned_to_raw(val, align_of_val_raw(val))
}

/// Returns the default value for `T` (only support types up to `1 << 20` bytes).
#[inline]
pub const fn default<T: Copy + Data>() -> T {
  struct Comptime<T: Copy + Data>(PhantomData<T>);

  impl<T: Copy + Data> Comptime<T> {
    const DEFAULT: T = {
      assert!(
        min_size_of::<T>() <= 1 << 20,
        "default value is only supported for types up to `1 << 20` bytes"
      );

      let mut bytes = [0u8; 1 << 20];
      let mut i = 0;
      while i < min_size_of::<T>() {
        if let Some(byte) = internal::default_at::<T>(i) {
          bytes[i] = byte;
        }
        i += 1;
      }

      unsafe { transmute_unchecked(bytes) }
    };
  }

  Comptime::<T>::DEFAULT
}

/// Returns whatever a value of type `T` is aligned to `align`.
#[inline]
pub const fn is_aligned_to<T: ?Sized + Data>(val: &T, align: usize) -> bool {
  is_aligned_to_raw(val, align)
}

/// Returns whatever an address to `T` is aligned to `align`.
#[inline]
pub const fn is_aligned_to_raw<T: ?Sized + Data>(val: *const T, align: usize) -> bool {
  assert!(align.is_power_of_two(), "alignment must be power of 2");
  // `value` is aligned for `U` if `T` alignment is already big enough.
  align <= align_of::<T>()
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(val) & (align - 1) == 0
}

#[inline]
pub const fn is_sized_val<T: ?Sized + Data>(_: &T) -> bool {
  is_sized::<T>()
}

#[inline]
pub const fn is_sized_val_raw<T: ?Sized + Data>(_: *const T) -> bool {
  is_sized::<T>()
}

#[inline]
pub const fn is_sized<T: ?Sized + Data>() -> bool {
  match T::LAYOUT {
    Layout::Slice => false,
    Layout::Struct => is_sized::<<T::TailField as Field>::Type>(),
    _ => true,
  }
}

#[inline]
pub const fn matches<T: Integer>(raw: &T::Repr) -> bool {
  T::SPEC.matches_ref(raw)
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::Sized>()
}

#[inline]
pub const fn min_size_of_tail<T: ?Sized + Data>() -> usize {
  min_size_of::<<<T as Data>::TailField as Field>::Type>()
}

#[inline]
pub const fn min_size_of_tail_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of_tail_val_raw(val)
}

#[inline]
pub const fn min_size_of_tail_val_raw<T: ?Sized + Data>(_: *const T) -> usize {
  min_size_of_tail::<T>()
}

#[inline]
pub const fn min_size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of_val_raw(val)
}

#[inline]
pub const fn min_size_of_val_raw<T: ?Sized + Data>(_: *const T) -> usize {
  min_size_of::<T>()
}

#[inline]
pub const fn size_of_tail<T: ?Sized + Data>(val: &T) -> usize {
  size_of_tail_raw(val)
}

#[inline]
pub const fn size_of_tail_raw<T: ?Sized + Data>(ptr: *const T) -> usize {
  size_of_val_raw(ptr) - internal::tail_offset::<T>()
}

#[inline]
pub const fn size_of_tail_slice<T: ?Sized + Data>(val: &T) -> usize {
  size_of_tail_slice_raw(val)
}

#[inline]
pub const fn size_of_tail_slice_item<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::TailSliceItem>()
}

#[inline]
pub const fn size_of_tail_slice_raw<T: ?Sized + Data>(val: *const T) -> usize {
  ptr::metadata(val) * size_of_tail_slice_item::<T>()
}

#[inline]
pub const fn size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  size_of_val_raw(val)
}

#[inline]
pub const fn size_of_val_raw<T: ?Sized + Data>(val: *const T) -> usize {
  min_size_of::<T>() + size_of_tail_slice_raw(val)
}

#[inline]
pub const fn size_of_with<T: ?Sized + Data>(metadata: usize) -> usize {
  min_size_of::<T>() + metadata * size_of_tail_slice_item::<T>()
}

#[inline]
pub const fn split_at<T: ?Sized + Data>(val: &T, mid: usize) -> (&T, &[T::TailSliceItem]) {
  match try_split_at(val, mid) {
    Some(result) => result,
    _ => panic!("`try_split_at` failed"),
  }
}

#[inline]
pub const fn tail_slice_of<T: ?Sized + Data>(val: &T) -> &[T::TailSliceItem] {
  let ptr: *const T = val;
  let ptr = unsafe { ptr.cast::<T::Sized>().add(1).cast::<T::TailSliceItem>() };
  assert!(is_aligned_raw(ptr));
  unsafe { &*ptr::from_raw_parts(ptr.cast(), ptr::metadata(val)) }
}

/// Safe transmute from a value of type `T` to a value of type `U`. The safety of this transmutation
/// is guaranteed by static analysis at compile time, hence a call to [`transmute`] that compile is
/// safe & sound.
///
/// This function is `const` and can be used in a `const` context.
///
/// # Examples
/// ```
/// assert_eq!(0, mutant::transmute::<_, u8>(false));
/// assert_eq!(1, mutant::transmute::<_, u8>(true));
///
/// // With arrays:
/// assert_eq!([0, 1], mutant::transmute::<_, [u8; 2]>([false, true]));
/// assert_eq!(0, mutant::transmute::<_, u16>([false, false]));
/// ```
#[inline]
pub const fn transmute<T: Data + Copy, U: Data + Copy>(val: T) -> U {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
  );

  unsafe { transmute_unchecked(val) }
}

/// # Safety
/// TODO
#[inline]
pub const unsafe fn transmute_unchecked<T, U>(val: T) -> U {
  union Transmute<T, U> {
    val: ManuallyDrop<T>,
    dst: ManuallyDrop<U>,
  }
  ManuallyDrop::into_inner(Transmute { val: ManuallyDrop::new(val) }.dst)
}

/// Safe transmute from a reference of type `T` to a value of type `U`. The safety of this
/// transmutation is guaranteed by static analysis at compile time, hence a call to
/// [`transmute_copy`] that compile is safe & sound.
///
/// This function is `const` and can be used in a `const` context.
///
/// # Examples
/// ```
/// use mutant::Unalign;
///
/// assert_eq!(0, mutant::transmute_copy::<_, u8>(&false));
/// assert_eq!(1, mutant::transmute_copy::<_, u8>(&true));
///
/// // With arrays:
/// assert_eq!([0, 0], mutant::transmute_copy::<_, [u8; 2]>(&[false, false]));
/// assert_eq!(0, mutant::transmute_copy::<_, u16>(&[false, false]));
/// ```
#[inline]
pub const fn transmute_copy<T: Data + Copy, U: Data + Copy>(val: &T) -> U {
  transmute(*val)
}

/// Safe transmute from a mutable reference of type `T` to a mutable reference of type `U`. The
/// safety of this transmutation is guaranteed by static analysis at compile time, hence a call to
/// [`transmute_mut`] that compile is safe & sound.
///
/// This function is `const` and can be used in a `const` context.
///
/// # Examples
/// ```
/// use mutant::num::*;
///
/// let mut value = u16le::from_raw(0xcafe);
/// mutant::transmute_mut::<_, [u8; 2]>(&mut value)[0] = 0xef;
/// mutant::transmute_mut::<_, [u8; 2]>(&mut value)[1] = 0xbe;
/// assert_eq!(0xbeef, value.into_raw());
///
/// // With arrays:
/// assert_eq!(&mut [0, 0], mutant::transmute_mut::<_, [u8; 2]>(&mut 0u16));
/// ```
#[inline]
pub fn transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(val: &mut T) -> &mut U {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_ALIGNMENT_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
    internal::Transmutation::<T, U>::CHECK_SOUND_TARGET_MUTATION,
  );

  let metadata = internal::convert_metadata::<T, U>(ptr::metadata(val));
  let ptr = ptr::from_raw_parts_mut::<U>(ptr::from_mut(val).cast(), metadata);
  unsafe { &mut *ptr }
}

/// Safe transmute from a reference of type `T` to a value of type `U`. The safety of this
/// transmutation is guaranteed by static analysis at compile time, hence a call to
/// [`transmute_ref`] that compile is safe & sound.
///
/// This function is `const` and can be used in a `const` context.
///
/// # Examples
/// ```
/// use mutant::Unalign;
///
/// assert_eq!(&0, mutant::transmute_ref::<_, u8>(&false));
/// assert_eq!(&1, mutant::transmute_ref::<_, u8>(&true));
///
/// // With arrays:
/// assert_eq!(&[0, 1], mutant::transmute_ref::<_, [u8; 2]>(&[false, true]));
/// assert_eq!(&Unalign::new(0), mutant::transmute_ref::<_, Unalign<u16>>(&[false, false]));
/// ```
#[inline]
pub const fn transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(val: &T) -> &U {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_ALIGNMENT_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
  );

  let metadata = internal::convert_metadata::<T, U>(ptr::metadata(val));
  let ptr = ptr::from_raw_parts::<U>(ptr::from_ref(val).cast(), metadata);
  unsafe { &*ptr }
}

/// Safe transmute & truncate from a value of type `T` to a value of type `U`. The safety of this
/// transmutation is guaranteed by static analysis at compile time, hence a call to [`truncate`]
/// that compile is safe & sound.
///
/// This function is `const` and can be used in a `const` context.
///
/// # Examples
/// ```
/// assert_eq!(123u8, mutant::truncate(123u32));
///
/// // With arrays:
/// assert_eq!([0, 1], mutant::truncate::<_, [u8; 2]>([false, true, false]));
/// assert_eq!(0, mutant::truncate::<_, u16>([false, false, false]));
/// ```
#[inline]
pub const fn truncate<T: Data + Copy, U: Data + Copy>(val: T) -> U {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
  );

  unsafe { transmute_unchecked(val) }
}

#[inline]
pub const fn truncate_ref<T: ?Sized + Data, U: ?Sized + Data>(val: &T) -> &U {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_ALIGNMENT_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
  );

  let metadata = internal::convert_metadata::<T, U>(ptr::metadata(val));
  let ptr = ptr::from_raw_parts::<U>(ptr::from_ref(val).cast(), metadata);
  unsafe { &*ptr }
}

#[inline]
pub const fn try_split_at<T: ?Sized + Data>(
  val: &T, mid: usize,
) -> Option<(&T, &[T::TailSliceItem])> {
  if mid < min_size_of::<T>() || mid > size_of_val(val) {
    return None;
  }
  if mid == size_of_val(val) {
    return Some((val, &[]));
  }
  let tail = mid - min_size_of::<T>();
  if tail % mem::size_of::<T::TailSliceItem>() != 0 {
    return None;
  }
  let ptr: *const u8 = (val as *const T).cast();
  let val_ptr = ptr::from_raw_parts::<T>(ptr.cast(), tail / mem::size_of::<T::TailSliceItem>());
  let rest_ptr = ptr::from_raw_parts::<[T::TailSliceItem]>(
    unsafe { ptr.add(mid) }.cast(),
    ptr::metadata(val) - ptr::metadata(val_ptr),
  );
  Some(unsafe { (&*val_ptr, &*rest_ptr) })
}

#[inline]
pub fn try_transmute<T: Data + Copy, U: Data + Copy>(val: T) -> Option<U> {
  try_transmute_copy(&val)
}

#[inline]
pub const fn try_transmute_copy<T: Data + Copy, U: Data + Copy>(val: &T) -> Option<U> {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
  );

  let ptr = ptr::from_ref(val).cast::<U>();
  match internal::validate::<_, _, false>(ptr, ptr).is_ok() {
    true => Some(unsafe { *ptr }),
    _ => None,
  }
}

#[inline]
pub fn try_transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(val: &mut T) -> Result<&mut U, Error> {
  #[allow(clippy::let_unit_value)]
  let _ = (
    internal::Transmutation::<T, U>::CHECK,
    internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_SOUND_TARGET_MUTATION,
  );

  if !is_aligned_to::<T>(val, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match internal::try_convert_metadata::<T, U>(ptr::metadata(val)) {
    Ok(metadata) => metadata,
    Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
  };
  let ptr = ptr::from_raw_parts_mut::<U>(ptr::from_mut(val).cast(), metadata);
  bail!(internal::validate::<_, _, false>(ptr, ptr));
  Ok(unsafe { &mut *ptr })
}

#[inline]
pub const fn try_transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(val: &T) -> Result<&U, Error> {
  #[allow(clippy::let_unit_value)]
  let _ =
    (internal::Transmutation::<T, U>::CHECK, internal::Transmutation::<T, U>::CHECK_NO_TRUNCATE);

  if !is_aligned_to::<T>(val, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match internal::try_convert_metadata::<T, U>(ptr::metadata(val)) {
    Ok(metadata) => metadata,
    Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
  };
  let ptr = ptr::from_raw_parts::<U>(ptr::from_ref(val).cast(), metadata);
  bail!(internal::validate::<_, _, false>(ptr, ptr));
  Ok(unsafe { &*ptr })
}

#[inline]
pub const fn unpack<T: ?Sized + Data>(bytes: &[u8]) -> Result<(&T, &[u8]), Error> {
  use internal::Transmutation;

  #[allow(clippy::let_unit_value)]
  let _ = Transmutation::<[u8], T>::CHECK;
  const_assert_unaligned::<T>();

  // Check if the size of `bytes` is compatible with `T`.
  if let Some(check) = unwrap_err!(Transmutation::<[u8], T>::STATIC_ANALYZE).maybe_size_mismatch {
    bail!(check.validate_for_unpack(bytes.len()), e => Error::SizeMismatch(e));
  }
  // Compute the metadata of `T` from the length of `bytes`.
  let metadata = match size_of_tail_slice_item::<T>() {
    0 => 0,
    _ => (bytes.len() - min_size_of::<T>()) / size_of_tail_slice_item::<T>(),
  };
  let ptr = ptr::from_raw_parts::<T>(ptr::from_ref(bytes).cast(), metadata);
  let ptr = bail!(internal::validate::<_, _, true>(ptr, ptr));
  Ok((unsafe { &*ptr }, bytes.split_at(size_of_val_raw(ptr)).1))
}

/// Returns zeroed value for `T` (only support types up to `1 << 20` bytes).
pub const fn zeroed<T: Copy + Data>() -> T {
  // Used to evaluate an expression at compile time.
  // TODO: use `const_inline` when stabilized.
  struct CompileTime<T>(PhantomData<T>);

  impl<T: Copy + Data> CompileTime<T> {
    // A call to truncate on bytes that only accept 0 as bits pattern will
    // only compile if `T` is actually zero-able.
    const ZEROED: T = truncate([num::u8c::<0>::new(); 1 << 20]);
  }

  // As a big buffer is used, make sure to compute the value only
  // at compile time.
  CompileTime::<T>::ZEROED
}

pub mod num {
  use super::*;
  use core::fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex};

  #[inline]
  pub const fn to_bytes<T: Integer>(int: T) -> [u8; 16] {
    union IntoBytes<T: Copy> {
      int: T,
      bytes: [u8; 16],
    }

    let mut bytes = IntoBytes { bytes: [0; 16] };
    bytes.int = int;
    unsafe { bytes.bytes }
  }

  #[inline]
  pub const fn from_bytes<T: Integer<Repr = T>>(bytes: [u8; 16]) -> T {
    truncate(bytes)
  }

  #[inline]
  pub const fn swap_bytes<T: Integer>(int: T) -> T::Repr {
    let mut bytes = to_bytes(int);
    let mut i = 0;
    while i < min_size_of::<T>() / 2 {
      let tmp = bytes[i];
      bytes[i] = bytes[min_size_of::<T>() - i - 1];
      bytes[min_size_of::<T>() - i - 1] = tmp;
      i += 1;
    }
    truncate(bytes)
  }

  #[inline]
  pub const fn into_ne<T: Integer>(int: T) -> T::Repr {
    if T::SPEC.endianness as u8 == internal::Endianness::NE as u8 {
      return transmute(int);
    }
    swap_bytes(int)
  }

  #[inline]
  pub const fn into_be<T: Integer>(int: T) -> T::Repr {
    if T::SPEC.endianness as u8 == internal::Endianness::BE as u8 {
      return transmute(int);
    }
    swap_bytes(int)
  }

  #[inline]
  pub const fn into_le<T: Integer>(int: T) -> T::Repr {
    if T::SPEC.endianness as u8 == internal::Endianness::LE as u8 {
      return transmute(int);
    }
    swap_bytes(int)
  }

  #[inline]
  pub const fn cast<T: Integer, U: Integer<Repr = U>>(int: T) -> U {
    union Cast<T: Copy, U: Copy> {
      int: T,
      dst: U,
    }

    let mut cast = Cast { dst: default() };
    cast.int = into_le(int);
    let dst_le = unsafe { cast.dst };
    if U::SPEC.endianness as u8 == internal::Endianness::LE as u8 {
      return dst_le;
    }
    swap_bytes(dst_le)
  }

  #[inline]
  pub const fn try_cast<T: Integer, U: Integer>(int: T) -> Option<U> {
    union Cast<T: Copy, U: Copy> {
      int: T,
      dst: U,
    }

    let mut cast = Cast { dst: default() };
    cast.int = into_le(int);
    let mut dst_le = unsafe { cast.dst };
    if U::SPEC.endianness as u8 != internal::Endianness::LE as u8 {
      dst_le = swap_bytes(dst_le);
    }
    if U::SPEC.matches(dst_le) {
      Some(transmute(dst_le))
    } else {
      None
    }
  }

  macro_rules! impl_num {
    (($T:ident $Raw:ident $N:literal $endianness:ident)) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $T(pub [u8; $N]);

      unsafe impl Integer for $T {
        type Repr = $T;
        const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::any(Self::MIN)
          .with_endianness(internal::Endianness::$endianness);
      }

      impl $T {
        pub const MIN: Self = Self::from_raw($Raw::MIN);
        pub const MAX: Self = Self::from_raw($Raw::MAX);

        #[inline] pub const fn to_ne_bytes(self) -> [u8; $N] { swap!($endianness => NE, self).0 }
        #[inline] pub const fn to_le_bytes(self) -> [u8; $N] { swap!($endianness => LE, self).0 }
        #[inline] pub const fn to_be_bytes(self) -> [u8; $N] { swap!($endianness => BE, self).0 }

        #[inline] pub const fn from_ne_bytes(b: [u8; $N]) -> Self { swap!(NE => $endianness, Self(b)) }
        #[inline] pub const fn from_le_bytes(b: [u8; $N]) -> Self { swap!(LE => $endianness, Self(b)) }
        #[inline] pub const fn from_be_bytes(b: [u8; $N]) -> Self { swap!(BE => $endianness, Self(b)) }

        #[inline]
        pub const fn new(raw: $Raw) -> Self {
          Self::from_raw(raw)
        }

        #[inline]
        pub const fn from_raw(raw: $Raw) -> Self {
          let (mut off, mut bytes) = (0, [0; $N]);
          while off < $N {
            bytes[off] = (raw >> (le_or_be!($endianness, off, $N - off - 1) * 8)) as u8;
            off += 1;
          }
          Self(bytes)
        }

        #[inline]
        pub const fn into_inner(self) -> $Raw {
          self.into_raw()
        }

        #[inline]
        pub const fn into_raw(self) -> $Raw {
          let (mut off, mut raw) = (0, 0);
          while off < $N {
            raw |= (self.0[off] as $Raw) << (le_or_be!($endianness, off, $N - off - 1) * 8);
            off += 1;
          }
          raw
        }

        #[inline]
        pub const fn swap_bytes(mut self) -> Self {
          let mut i = 0;
          while i < $N / 2 {
            let tmp = self.0[i];
            self.0[i] = self.0[$N - i - 1];
            self.0[$N - i - 1] = tmp;
            i += 1;
          }
          self
        }

        #[inline]
        pub const fn equals(self, other: Self) -> bool {
          self.into_raw() == other.into_raw()
        }
      }

      impl From<$Raw> for $T {
        #[inline]
        fn from(raw: $Raw) -> Self {
          Self::from_raw(raw)
        }
      }

      impl From<$T> for $Raw {
        #[inline]
        fn from(value: $T) -> Self {
          value.into_raw()
        }
      }

      impl Debug for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Debug>::fmt(&self.into_raw(), f)
        }
      }

      impl Display for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Display>::fmt(&self.into_raw(), f)
        }
      }

      impl Binary for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Binary>::fmt(&self.into_raw(), f)
        }
      }

      impl Octal for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as Octal>::fmt(&self.into_raw(), f)
        }
      }

      impl LowerHex for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as LowerHex>::fmt(&self.into_raw(), f)
        }
      }

      impl UpperHex for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Raw as UpperHex>::fmt(&self.into_raw(), f)
        }
      }
    };
    ($($tt:tt)+) => {
      $(impl_num!($tt);)+
    };
  }

  macro_rules! impl_const_num {
    (($ConstT:ident $T:ident)) => {
      impl_const_num!(($ConstT $T $T V, V.to_ne_bytes()));
    };
    (($ConstT:ident $T:ident $VT:ident)) => {
      impl_const_num!(($ConstT $T $VT $T::from_raw(V), $T::from_raw(V).0));
    };
    (($ConstT:ident $T:ident $VT:ident $v2raw:expr, $v2bytes:expr)) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $ConstT<const V: $VT>($T);

      unsafe impl<const V: $VT> Integer for $ConstT<V> {
        type Repr = $T;
        // TODO: use T::SPEC.with_*
        const SPEC: IntegerSpec<Self::Repr> =
          IntegerSpec::new($v2raw, BytePattern::Equ(&$v2raw.to_ne_bytes()));
      }

      impl<const V: $VT> $ConstT<V> {
        #[inline]
        pub const fn new() -> Self {
          Self($v2raw)
        }

        #[inline]
        pub const fn into_raw(self) -> $T {
          $v2raw
        }
      }

      impl<const V: $VT> TryFrom<$T> for $ConstT<V> {
        type Error = ();

        #[inline]
        fn try_from(value: $T) -> Result<Self, Self::Error> {
          if value == $v2raw {
            Ok(Self::new())
          } else {
            Err(())
          }
        }
      }

      impl<const V: $VT> From<$ConstT<V>> for $T {
        #[inline]
        fn from(value: $ConstT<V>) -> Self {
          value.0
        }
      }

      impl<const V: $VT> Default for $ConstT<V> {
        #[inline]
        fn default() -> Self {
          Self::new()
        }
      }

      impl<const V: $VT> Display for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          Display::fmt(&V, f)
        }
      }

      impl<const V: $VT> Debug for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          Debug::fmt(&V, f)
        }
      }

      impl<const V: $VT> Binary for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          Binary::fmt(&V, f)
        }
      }

      impl<const V: $VT> Octal for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          Octal::fmt(&V, f)
        }
      }

      impl<const V: $VT> LowerHex for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          LowerHex::fmt(&V, f)
        }
      }

      impl<const V: $VT> UpperHex for $ConstT<V> {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          UpperHex::fmt(&V, f)
        }
      }
    };
    ($($tt:tt)+) => {
      $(impl_const_num!($tt);)+
    };
  }

  #[allow(non_camel_case_types)]
  #[derive(Eq, PartialEq, Copy, Clone, Hash)]
  #[repr(transparent)]
  pub struct SetDefault<T: Integer, const DEFAULT: u128>(T);

  unsafe impl<T: Integer, const DEFAULT: u128> Integer for SetDefault<T, DEFAULT> {
    type Repr = T::Repr;
    const SPEC: IntegerSpec<Self::Repr> = T::SPEC.with_default(truncate(DEFAULT));
  }

  impl<T: Integer + Debug, const DEFAULT: u128> Debug for SetDefault<T, DEFAULT> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(&self.0, f)
    }
  }

  #[rustfmt::skip]
  macro_rules! le_or_be {
    (NE, $le:expr, $be:expr $(,)?) => { if cfg!(target_endian = "little") { $le } else { $be } };
    (LE, $le:expr, $be:expr $(,)?) => { $le };
    (BE, $le:expr, $be:expr $(,)?) => { $be };
  }

  #[rustfmt::skip]
  macro_rules! swap {
    (NE => NE, $e:expr) => { $e };
    (LE => LE, $e:expr) => { $e };
    (BE => BE, $e:expr) => { $e };
    (LE => BE, $e:expr) => { $e.swap_bytes() };
    (BE => LE, $e:expr) => { $e.swap_bytes() };
    (NE => LE, $e:expr) => { if cfg!(target_endian = "little") { $e } else { $e.swap_bytes() } };
    (NE => BE, $e:expr) => { if cfg!(target_endian = "little") { $e.swap_bytes() } else { $e } };
    (LE => NE, $e:expr) => { if cfg!(target_endian = "little") { $e } else { $e.swap_bytes() } };
    (BE => NE, $e:expr) => { if cfg!(target_endian = "little") { $e.swap_bytes() } else { $e } };
  }

  impl_num! {
    (u16le u16 2 LE) (u16be u16 2 BE)
    (u24le u32 3 LE) (u24be u32 3 BE) (u24 u32 3 NE)
    (u32le u32 4 LE) (u32be u32 4 BE)
    (u40le u64 5 LE) (u40be u64 5 BE) (u40 u64 5 NE)
    (u48le u64 6 LE) (u48be u64 6 BE) (u48 u64 6 NE)
    (u56le u64 7 LE) (u56be u64 7 BE) (u56 u64 7 NE)
    (u64le u64 8 LE) (u64be u64 8 BE)
    (u72le u128 9 LE) (u72be u128 9 BE) (u72 u128 9 NE)
    (u80le u128 10 LE) (u80be u128 10 BE) (u80 u128 10 NE)
    (u88le u128 11 LE) (u88be u128 11 BE) (u88 u128 11 NE)
    (u96le u128 12 LE) (u96be u128 12 BE) (u96 u128 12 NE)
    (u104le u128 13 LE) (u104be u128 13 BE) (u104 u128 13 NE)
    (u112le u128 14 LE) (u112be u128 14 BE) (u112 u128 14 NE)
    (u120le u128 15 LE) (u120be u128 15 BE) (u120 u128 15 NE)
    (u128le u128 16 LE) (u128be u128 16 BE)
  }

  impl_const_num! {
    (u8c u8) (u16c u16) (u32c u32) (u64c u64) (u128c u128)
    (i8c i8) (i16c i16) (i32c i32) (i64c i64) (i128c i128)
    (u16lec u16le u16) (u16bec u16be u16)
    (u24lec u24le u32) (u24bec u24be u32) (u24c u24 u32)
    (u32lec u32le u32) (u32bec u32be u32)
    (u40lec u40le u64) (u40bec u40be u64) (u40c u40 u64)
    (u48lec u48le u64) (u48bec u48be u64) (u48c u48 u64)
    (u56lec u56le u64) (u56bec u56be u64) (u56c u56 u64)
    (u64lec u64le u64) (u64bec u64be u64)
    (u72lec u72le u128) (u72bec u72be u128) (u72c u72 u128)
    (u80lec u80le u128) (u80bec u80be u128) (u80c u80 u128)
    (u88lec u88le u128) (u88bec u88be u128) (u88c u88 u128)
    (u96lec u96le u128) (u96bec u96be u128) (u96c u96 u128)
    (u104lec u104le u128) (u104bec u104be u128) (u104c u104 u128)
    (u112lec u112le u128) (u112bec u112be u128) (u112c u112 u128)
    (u120lec u120le u128) (u120bec u120be u128) (u120c u120 u128)
    (u128lec u128le u128) (u128bec u128be u128)
  }
}

mod internal {
  use core::fmt;
  use core::fmt::{Debug, Display, Formatter};
  use core::num::{NonZeroU128, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8};
  use core::ops::{Deref, DerefMut};

  use crate::*;

  use BytePattern::*;
  use Endianness::*;
  use MetadataChecks::*;

  #[inline]
  const fn repr_contains<R: Field, T: ?Sized + Data>(offset: usize) -> bool {
    offset >= R::OFFSET && (!is_sized::<R::Type>() || offset < R::OFFSET + min_size_of::<R::Type>())
  }

  #[inline]
  const fn repr_field<R: Field, T: ?Sized + Data>(ptr: *const T) -> *const R::Type {
    ptr::from_raw_parts(
      // SAFETY: `ptr` came from a valid reference and `R::OFFSET` is in type bounds.
      unsafe { ptr.cast::<u8>().add(R::OFFSET) }.cast(),
      match is_sized::<R::Type>() {
        true => 0,
        _ => ptr::metadata(ptr),
      },
    )
  }

  const fn bytes_at<T: ?Sized + Data>(offset: usize) -> Bytes<'static> {
    #[inline]
    const fn repr_bytes_at<R: Field, T: ?Sized + Data>(offset: usize) -> Bytes<'static> {
      if repr_contains::<R, T>(offset) {
        return bytes_at::<R::Type>(offset - R::OFFSET);
      }
      match R::INDEX {
        0 => Bytes::uninit(),
        _ => repr_bytes_at::<R::PrevField, T>(offset),
      }
    }

    // Check for `offset` to be in bounds.
    if is_sized::<T>() && offset >= min_size_of::<T>() {
      return Bytes::uninit();
    }
    // Check for `ZST` slices.
    if matches!(T::LAYOUT, Layout::Slice | Layout::Array) && mem::size_of::<T::Item>() == 0 {
      return Bytes::uninit();
    }
    match T::LAYOUT {
      Layout::Slice | Layout::Array => bytes_at::<T::Item>(offset % mem::size_of::<T::Item>()),
      Layout::Struct => repr_bytes_at::<T::TailField, T>(offset),
      Layout::Integer { spec } if spec.is_uninit() => Bytes::uninit(),
      Layout::Integer { spec } => {
        assert!(offset == 0);
        Bytes::new(spec.endianness, spec.byte_pattern)
      }
    }
  }

  #[inline]
  pub(crate) const fn default_at<T: ?Sized + Data>(offset: usize) -> Option<u8> {
    #[inline]
    const fn repr_default_at<R: Field, T: ?Sized + Data>(offset: usize) -> Option<u8> {
      if repr_contains::<R, T>(offset) {
        return default_at::<R::Type>(offset - R::OFFSET);
      }
      match R::INDEX {
        0 => None,
        _ => repr_default_at::<R::PrevField, T>(offset),
      }
    }

    // Check for `offset` to be in bounds.
    if is_sized::<T>() && offset >= min_size_of::<T>() {
      return None;
    }
    // Check for `ZST` slices.
    if matches!(T::LAYOUT, Layout::Slice | Layout::Array) && mem::size_of::<T::Item>() == 0 {
      return None;
    }
    match T::LAYOUT {
      Layout::Slice | Layout::Array => default_at::<T::Item>(offset % mem::size_of::<T::Item>()),
      Layout::Struct => repr_default_at::<T::TailField, T>(offset),
      Layout::Integer { spec } if spec.is_uninit() => None,
      Layout::Integer { spec } => Some(spec.default[offset]),
    }
  }

  #[inline]
  pub(crate) const fn convert_metadata<T: ?Sized + Data, U: ?Sized + Data>(
    metadata: usize,
  ) -> usize {
    #[allow(clippy::let_unit_value)]
    let _ = (Transmutation::<T, U>::CHECK, Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH);

    match size_of_tail_slice_item::<U>() {
      0 => 0,
      n => (size_of_with::<T>(metadata) - min_size_of::<U>()) / n,
    }
  }

  #[inline]
  pub(crate) const fn try_convert_metadata<T: ?Sized + Data, U: ?Sized + Data>(
    metadata: usize,
  ) -> Result<usize, isize> {
    #[allow(clippy::let_unit_value)]
    let _ = Transmutation::<T, U>::CHECK;

    let check = match Transmutation::<T, U>::STATIC_ANALYZE {
      Err(_) => unreachable!(),
      Ok(static_analysis) => match static_analysis.maybe_size_mismatch {
        Some(check) => check.validate(metadata),
        _ => Ok(()),
      },
    };
    match check {
      Err(n) => Err(n * size_of_tail_slice_item::<T>() as isize),
      _ if size_of_tail_slice_item::<U>() == 0 => Ok(0),
      _ => Ok((size_of_with::<T>(metadata) - min_size_of::<U>()) / size_of_tail_slice_item::<U>()),
    }
  }

  #[inline]
  pub const fn tail_offset<T: ?Sized + Data>() -> usize {
    match T::LAYOUT {
      Layout::Struct => T::TailField::OFFSET,
      _ => 0,
    }
  }

  #[cfg(test)]
  #[inline]
  pub const fn tail_item_size<T: ?Sized + Data>() -> usize {
    match T::LAYOUT {
      Layout::Struct => mem::size_of::<<<<T as Data>::TailField as Field>::Type as Data>::Item>(),
      _ => 0,
    }
  }

  #[inline]
  pub(crate) const fn validate<C: ?Sized + Data, T: ?Sized + Data, const RESIZE: bool>(
    container: *const C, ptr: *const T,
  ) -> Result<*const C, Error> {
    #[inline]
    const fn repr_validate<R: Field, T: ?Sized + Data, const RESIZE: bool>(
      mut ptr: *const T,
    ) -> Result<*const T, Error> {
      ptr = bail!(validate::<_, _, RESIZE>(ptr, repr_field::<R, _>(ptr)));
      match R::INDEX {
        0 => Ok(ptr),
        _ => repr_validate::<R::PrevField, _, RESIZE>(ptr),
      }
    }

    match T::LAYOUT {
      Layout::Struct => {
        let ptr = bail!(repr_validate::<T::TailField, T, RESIZE>(ptr));
        if RESIZE && !is_sized::<T>() {
          Ok(ptr::from_raw_parts::<C>(container.cast(), ptr::metadata(ptr)))
        } else {
          Ok(container)
        }
      }
      Layout::Slice | Layout::Array => {
        // Compute the length of `ptr`.
        let len = match T::LAYOUT {
          Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
          _ => ptr::metadata(ptr),
        };
        // Call `resize` on each item of `ptr`.
        let mut i = 0;
        while i < len {
          // SAFETY: `ptr` came from a valid reference and `i` is in type bounds.
          let item_ptr = unsafe { ptr.cast::<T::Item>().add(i) };
          bail!(validate::<_, _, RESIZE>(item_ptr, item_ptr));
          i += 1;
        }
        Ok(container)
      }
      Layout::Integer { spec } => {
        // SAFETY: `ptr` is valid for read since
        // - `ptr` is a valid memory location to `F`.
        check!(unsafe { spec.matches_ptr(ptr.cast()) }, Error::InvalidBitPattern);
        // Retrieve `SizeField` if any.
        let size_field = unwrap!(spec.size_field, return Ok(container));
        // Compute the size from `ptr`.
        let int = unsafe { core::ptr::read_unaligned(ptr.cast()) };
        let size: usize = num::cast::<T::Integer, u64>(int) as usize;
        // Compute the new metadata.
        let metadata = match size_field {
          _ if min_size_of::<T>() == 0 => return Ok(container),
          SizeField::Size if !RESIZE || size_of_tail_slice_item::<C>() == 0 => {
            check!(size == size_of_val_raw(container), Error::SizeMismatch(/* TODO */ 0));
            return Ok(container);
          }
          SizeField::Size => {
            // Check if `size` is at least the minimum size of `C`.
            check!(size >= min_size_of::<C>(), Error::SizeMismatch(/* TODO */ 0));
            // Compute the expected tail size and check for alignment.
            let tail_size = size - min_size_of::<C>();
            check!(tail_size % size_of_tail_slice_item::<C>() == 0, Error::AlignmentMismatch);
            // Compute the new metadata.
            tail_size / size_of_tail_slice_item::<C>()
          }
        };
        // Compute the new container from the new metadata.
        check!(metadata <= ptr::metadata(container), Error::SizeMismatch(/* TODO */ 0));
        Ok(ptr::from_raw_parts::<C>(container.cast(), metadata))
      }
    }
  }

  /// Performs static analysis to check if the transmutation from `Src` to `Dst` is safe.
  pub(crate) const fn static_analyze<Src, Dst>() -> Result<StaticAnalyze, &'static str>
  where
    Src: ?Sized + Data,
    Dst: ?Sized + Data,
  {
    // Try to resolve `xa[ya] => xb[yb]`, where:
    // - `a` and `b` are `Src` and `Dst` respectively,
    // - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
    // - `ya` and `yb` are the tail slice item sizes of `a` and `b` respectively.
    // - `n` is the minimum number of the source tail slice items needed to align `a` and `b`.
    // - `m` is the source tail slice items multiplier.
    // Calculate minimum and tail slice item sizes for both source and target types.
    let (xa, ya) = (min_size_of::<Src>(), size_of_tail_slice_item::<Src>());
    let (xb, yb) = (min_size_of::<Dst>(), size_of_tail_slice_item::<Dst>());
    // Compute if source and target are DSTs, i.e. had a tail slice with a non-zero item size.
    let (src_is_dst, dst_is_dst) = (ya != 0, yb != 0);
    // Compute the minimum source tail slice items needed to align source and target.
    // 1. Source is smaller than target:
    //  - Check for source to be a DST.
    //  - Compute the missing bytes to fit the target.
    //  - In case the target isn't a DST, check if the missing bytes are a multiple of
    //    the source DST tail slice item size.
    //  - The minimum source tail slice items is the missing bytes ceil divided by the source DST
    //    tail slice item size.
    let mut n = if xa < xb {
      // Check for source to be a DST.
      check!(src_is_dst, "source too small to fit target");
      // Compute the missing bytes to fit the target.
      let missing = xb - xa;
      // In case the target isn't a DST:
      if !dst_is_dst {
        // Check if the missing bytes are a multiple of the source DST tail slice item size.
        check!(
          missing % ya == 0,
          "missing bytes to fit the target is not a multiple of the source DST trailing slice \
           item size"
        );
      }
      // The minimum source tail slice items is the missing bytes ceil divided by the source DST
      // tail slice item size.
      (missing + ya - 1) / ya
    }
    // 2. Source is larger or equal to target:
    //  - The minimum source tail slice items is 0.
    else {
      // In case the target is a DST:
      // - Compute the remaining bytes to fit in the target.
      // - In case the source isn't a DST, check if the remaining bytes is a multiple
      //   of the target DST tail slice item size.
      if dst_is_dst {
        // Compute the remaining bytes to fit in the target.
        let remaining = xa - xb;
        // In case the source isn't a DST:
        if !src_is_dst {
          // Check if the remaining bytes is a multiple of the target DST tail slice item size.
          check!(
            remaining % yb == 0,
            "remaining bytes to fit in the target is not a multiple of the target DST trailing \
             slice item size",
          );
        }
      }
      // The minimum source tail slice items is 0.
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
      // source tail slice items count.
      // Otherwise, the source metadata must be empty.
      Some(if xa < xb { Is(n) } else { IsEmpty })
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
      let gcd = gcd(ya, yb);
      // Compute the absolute difference between the source and target DST minimum sizes.
      let absolute_miss = if xa < xb { xb - xa } else { xa - xb };
      // Make sure the source can catch up on the target.
      check!(absolute_miss % gcd == 0, "DST source cannot catch up on DST target");
      // Compute the minimum number of source tail slice items needed to align source and target.
      // We need to find `n` in a way that `xa + n*ya - yb` is a multiple of `yb`. Although this
      // could be resolved using complex equations, for the sake of readability, we can simplify it
      // by using the following loop:
      // - `n * ya` is the total size in bytes of the source tail slice for `n` source items.
      // - `xa + n * ya` is the total size in bytes of the source for `n` source items.
      // - `(xa + n * ya - xb)` is the total size of the target tail slice for `n` source items.
      // If the latter is a multiple of `yb`, then we found the minimum number of source tail slice
      // items needed to align source and target.
      while (xa + n * ya - xb) % yb != 0 {
        n += 1; // Increment the minimum number of source tail slice items by 1.
      }
      // Compute the metadata checks for the source DST tail slice.
      match (n, yb / gcd) {
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
    // Compute the minimum size in bytes need to fully check `Src` and `Dst` bit patterns
    // compatibility (transmutability).
    let min_check_size = match maybe_size_mismatch {
      None if ya != 0 => xa + ya,
      None => xb,
      Some(IsEmpty) => xa,
      Some(IsMultipleOf(m)) => xa + (m * ya),
      Some(Is(n)) => xa + (n * ya),
      Some(IsAtLeast(n)) => xa + (n * ya) + ya,
      Some(IsAtLeastThenMultipleOf(n, m)) => xa + (n * ya) + (m * ya),
    };
    {
      let mut big = [core::ptr::null::<()>(); 1 << 16];
      let mut i = 0;
      while i < min_check_size {
        let ptr = core::ptr::addr_of!(min_check_size);
        big[i] = ptr.cast();
        i += 1;
      }
    }
    // Check for `Src` and `Dst` bit patterns compatibility (transmutability).
    let (src, dst) = (bytes_at::<Src>(0), bytes_at::<Dst>(0));
    let t = bail!(src.intersects::<Src, Dst>(dst, 0, min_check_size));
    if !t.reachable {
      return Err("none of the bit patterns in source are valid in target");
    }
    // Static analyze result.
    let maybe_invalid_bit_pattern = !t.infallible;
    let sound_target_mutation = t.mutable;
    let truncate = xa > xb && yb == 0;
    Ok(StaticAnalyze::new(
      maybe_size_mismatch,
      maybe_invalid_bit_pattern,
      sound_target_mutation,
      truncate,
    ))
  }

  #[rustfmt::skip]
  #[repr(u8)]
  #[derive(Copy, Clone, Debug, Eq, PartialEq)]
  pub enum Endianness { LE, BE }

  impl Endianness {
    #[cfg(target_endian = "little")]
    pub const NE: Self = LE;
    #[cfg(target_endian = "big")]
    pub const NE: Self = BE;
  }

  /// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
  ///
  /// # Internals
  /// DSTs metadata represents the length of it's [tail slice](#tail-slice), such as `.1` in
  /// `(u16, [u16])`. These checks [validate](MetadataChecks::validate) such metadata to ensure safe
  /// and sound transmutations.
  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub(crate) enum MetadataChecks {
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
    pub const fn validate(self, metadata: usize) -> Result<(), isize> {
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

    /// Same as [`MetadataChecks::validate`] but when unpacking, i.e. the `MultipleOf` checks aren't
    /// checked anymore.
    #[inline]
    pub const fn validate_for_unpack(self, metadata: usize) -> Result<(), isize> {
      match self {
        Is(n) | IsAtLeast(n) if metadata >= n => Ok(()),
        Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
        IsMultipleOf(_) => Ok(()),
        IsAtLeastThenMultipleOf(n, _) if metadata >= n => Ok(()),
        IsAtLeastThenMultipleOf(n, _) => Err(n as isize - metadata as isize),
        IsEmpty => Ok(()),
      }
    }
  }

  /// `check_bit_patterns` transmutability context.
  #[derive(Copy, Clone, PartialEq)]
  pub(crate) struct Transmutability {
    pub(crate) reachable: bool, // At least one source bit pattern exists in target.
    pub(crate) infallible: bool, // All source bit patterns exists in target.
    pub(crate) mutable: bool,   // All target bit patterns exists in source.
  }

  impl Transmutability {
    #[inline]
    pub const fn unreachable() -> Self {
      Self { reachable: false, infallible: true, mutable: true }
    }

    #[inline]
    pub const fn reachable_infallible_mutable() -> Self {
      Self { reachable: true, infallible: true, mutable: true }
    }

    #[inline]
    pub const fn and(mut self, other: Self) -> Self {
      self.reachable &= other.reachable;
      self.mutable &= other.mutable;
      self.infallible &= other.infallible;
      self
    }

    #[inline]
    pub const fn or(mut self, other: Self) -> Self {
      self.reachable |= other.reachable;
      if other.reachable {
        self.mutable &= other.mutable;
        self.infallible &= other.infallible;
      }
      self
    }
  }

  #[cfg(test)]
  impl Debug for Transmutability {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      match (self.reachable, self.infallible, self.mutable) {
        (false, _, _) => f.write_str("unreachable"),
        (true, true, true) => f.write_str("reachable_infallible_mutable"),
        (true, true, false) => f.write_str("reachable_infallible"),
        (true, false, true) => f.write_str("reachable_mutable"),
        (true, false, false) => f.write_str("reachable"),
      }
    }
  }

  /// Static analyze result.
  #[derive(Copy, Clone, Eq, PartialEq)]
  pub(crate) struct StaticAnalyze {
    pub(crate) maybe_size_mismatch: Option<MetadataChecks>,
    pub(crate) maybe_invalid_bit_pattern: bool,
    pub(crate) sound_target_mutation: bool,
    pub(crate) truncate: bool,
  }

  impl StaticAnalyze {
    #[inline]
    const fn new(
      maybe_size_mismatch: Option<MetadataChecks>, maybe_invalid_bit_pattern: bool,
      sound_target_mutation: bool, truncate: bool,
    ) -> Self {
      Self { maybe_size_mismatch, maybe_invalid_bit_pattern, sound_target_mutation, truncate }
    }

    #[allow(unused)]
    #[inline]
    pub const fn fallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, true, false, false)
    }

    #[allow(unused)]
    #[inline]
    pub const fn fallible_truncate(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, true, false, true)
    }

    #[allow(unused)]
    #[inline]
    pub const fn fallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, true, true, false)
    }

    #[allow(unused)]
    #[inline]
    pub const fn fallible_mutable_truncate(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, true, true, true)
    }

    #[allow(unused)]
    #[inline]
    pub const fn infallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, false, false, false)
    }

    #[allow(unused)]
    #[inline]
    pub const fn infallible_truncate(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, false, false, true)
    }

    #[allow(unused)]
    #[inline]
    pub const fn infallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, false, true, false)
    }

    #[allow(unused)]
    #[inline]
    pub const fn infallible_mutable_truncate(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self::new(maybe_size_mismatch, false, true, true)
    }
  }

  #[cfg(test)]
  impl Debug for StaticAnalyze {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      let tag = match (self.maybe_invalid_bit_pattern, self.sound_target_mutation, self.truncate) {
        (true, false, false) => "fallible",
        (true, true, false) => "fallible_mutable",
        (false, false, false) => "infallible",
        (false, true, false) => "infallible_mutable",
        (true, false, true) => "fallible_truncate",
        (true, true, true) => "fallible_mutable_truncate",
        (false, false, true) => "infallible_truncate",
        (false, true, true) => "infallible_mutable_truncate",
      };
      let mut d = f.debug_tuple(tag);
      d.field(&self.maybe_size_mismatch);
      d.finish()
    }
  }

  /// Static-analyze a transmutation at compile-time.
  pub(crate) struct Transmutation<Src: ?Sized, Dst: ?Sized> {
    source: PhantomData<Src>,
    target: PhantomData<Dst>,
  }

  impl<Src: ?Sized + Data, Dst: ?Sized + Data> Transmutation<Src, Dst> {
    pub(crate) const STATIC_ANALYZE: Result<StaticAnalyze, &'static str> =
      static_analyze::<Src, Dst>();

    pub const CHECK: () = {
      unwrap_err!(Self::STATIC_ANALYZE);
    };
    pub const CHECK_NO_TRUNCATE: () = {
      if unwrap_err!(Self::STATIC_ANALYZE).truncate {
        panic!("\n  target is too small.\n  use `truncate()` instead.\n")
      }
    };
    pub const CHECK_NO_SIZE_MISMATCH: () = {
      if let Some(checks) = unwrap_err!(Self::STATIC_ANALYZE).maybe_size_mismatch {
        use MetadataChecks::*;
        match checks {
          Is(_) => panic!(
            "\n  cannot transmute between types of incompatible byte layouts, source too small.\n  \
            consider using a `try_*` alternative instead.\n"
          ),
          IsAtLeast(_) => panic! (
            "\n  cannot transmute between types of incompatible byte layouts, target too small.\n  \
             consider using a `try_*` alternative instead.\n"
          ),
          IsMultipleOf(_) => panic!(
            "\n  cannot transmute between types of incompatible byte layouts, tail size multiple \
             of mismatch.\n  consider using a `try_*` alternative instead.\n"
          ),
          IsAtLeastThenMultipleOf(_, _) => panic!(
            "\n  cannot transmute between types of incompatible byte layouts, target too small or \
             tail size multiple of mismatch.\n  consider using a `try_*` alternative instead.\n"
          ),
          IsEmpty => panic!(
            "\n  cannot transmute between types of incompatible byte layouts, source tail might \
             not be empty.\n  consider using a `try_*` alternative instead.\n"
          ),
        }
      }
    };
    pub const CHECK_NO_ALIGNMENT_MISMATCH: () = {
      if align_of::<Dst>() > align_of::<Src>() {
        panic!(
          "\n  source alignment is too small to fulfill target alignment requirements, hence it \
           requires a runtime check.\n  use `try_transmute()` instead.\n",
        )
      }
    };
    pub const CHECK_NO_INVALID_BIT_PATTERN: () = {
      if unwrap_err!(Self::STATIC_ANALYZE).maybe_invalid_bit_pattern {
        panic!(
          "\n  target are not valid for all bit patterns.\n  and not all source bit patterns are \
           valid for target, which requires a runtime check.\n  use `try_transmute()` instead.\n",
        )
      }
    };
    pub const CHECK_SOUND_TARGET_MUTATION: () = {
      if !unwrap_err!(Self::STATIC_ANALYZE).sound_target_mutation {
        panic!(
          "\n  mutating target could result in invalid bit patterns in source, hence mutation is \
           disabled.\n",
        )
      }
    };
  }

  // impl BytesPattern -----------------------------------------------------------------------------

  #[rustfmt::skip]
  #[derive(Copy, Clone)]
  struct ByteInterval { start: u8, end: u8 }

  impl ByteInterval {
    pub const fn any() -> Self {
      Self::new(0, 0xff)
    }

    pub const fn new(start: u8, end: u8) -> Self {
      assert!(end >= start, "end must be greater or equal to start");
      Self { start, end }
    }

    pub const fn from_tuple(tuple: (u8, u8)) -> Self {
      Self::new(tuple.0, tuple.1)
    }

    pub const fn len(self) -> usize {
      self.end as usize - self.start as usize + 1
    }

    pub const fn matches(self, byte: u8) -> bool {
      byte >= self.start && byte <= self.end
    }

    pub const fn intersects(self, other: Self) -> Option<Self> {
      let start = if self.start > other.start { self.start } else { other.start };
      let end = if self.end < other.end { self.end } else { other.end };
      if start > end {
        None
      } else {
        Some(Self::new(start, end))
      }
    }
  }

  impl<'a> BytePattern<'a> {
    pub const fn any<T>() -> Self {
      pub const ANY: [(u8, u8); 16] = [(0, 0xff); 16];
      Cat(&ANY.split_at(mem::size_of::<T>()).0)
    }

    const fn size(&self) -> usize {
      match self {
        Uninit => 1,
        All(&[]) | Any(&[]) | Cat(&[]) => 0,
        Cat(patterns) => patterns.len(),
        Equ(bytes) | Gte(bytes) => bytes.len(),
        All(patterns) | Any(patterns) => {
          let (mut i, size) = (1, patterns[0].size());
          while i < patterns.len() {
            assert!(size == patterns[i].size());
            i += 1;
          }
          size
        }
      }
    }

    #[cfg(test)]
    #[inline(always)]
    pub(crate) const fn matches<T: Integer>(&self, int: T) -> bool {
      let bytes = num::to_bytes(int);
      unsafe { self.matches_ptr(bytes.as_ptr(), T::SPEC.endianness) }
    }

    #[inline(always)]
    const unsafe fn matches_ptr(&self, ptr: *const u8, endianness: Endianness) -> bool {
      match self {
        Uninit => true,
        All(patterns) => fold!(patterns, true, |r, x| r && x.matches_ptr(ptr, endianness)),
        Any(patterns) => fold!(patterns, false, |r, x| r || x.matches_ptr(ptr, endianness)),
        Cat(patterns) => {
          fold!(patterns, true, |i, r, x| r && ByteInterval::from_tuple(*x).matches(*ptr.add(i)))
        }
        Equ(bytes) => fold!(bytes, true, |i, r, x| r && *ptr.add(i) == *x),
        Gte(bytes) => {
          let (mut i, mut a, mut b) = (0, 0u128, 0u128);
          while i < bytes.len() {
            let shift = 8 * if matches!(endianness, LE) { i } else { bytes.len() - i - 1 };
            a |= (*ptr.add(i) as u128) << shift;
            b |= (bytes[i] as u128) << shift;
            i += 1;
          }
          a >= b
        }
      }
    }
  }

  // impl Bytes ------------------------------------------------------------------------------------

  #[derive(Copy, Clone)]
  struct Bytes<'a> {
    endianness: Endianness,
    pattern: BytePattern<'a>,
    offset: usize,
    bytes: [ByteInterval; 16],
  }

  impl<'a> Bytes<'a> {
    pub const fn new(endianness: Endianness, pattern: BytePattern<'a>) -> Self {
      Self { endianness, pattern, offset: 0, bytes: [ByteInterval::any(); 16] }
    }

    pub const fn uninit() -> Self {
      Self::new(Endianness::NE, Uninit)
    }

    pub const fn reduce(self, pattern: BytePattern) -> Bytes {
      Bytes { pattern, ..self }
    }

    pub const fn append(mut self, byte: ByteInterval) -> Self {
      self.bytes[self.offset] = byte;
      self.offset += 1;
      self
    }

    const fn intersects<'b, T: ?Sized + Data, U: ?Sized + Data>(
      &self, mut dst: Bytes<'b>, off: usize, end: usize,
    ) -> Result<Transmutability, &'static str> {
      let (reachable, unreachable) =
        (Transmutability::reachable_infallible_mutable(), Transmutability::unreachable());
      if off >= end {
        return Ok(reachable);
      }
      match (self.pattern, dst.pattern) {
        (Uninit | Cat(&[_]), Uninit) => {
          bytes_at::<T>(off + 1).intersects::<T, U>(bytes_at::<U>(off + 1), off + 1, end)
        }
        (Uninit, _) => return Err("uninitialized byte in source"),
        (All(p), _) | (_, All(p)) => Ok(fold!(p, reachable, |s, x| s.and({
          let (src, dst) = match self.pattern {
            All(_) => (self.reduce(*x), dst),
            _ => (*self, dst.reduce(*x)),
          };
          bail!(src.intersects::<T, U>(dst, off, end))
        }))),
        (Any(p), _) | (_, Any(p)) => Ok(fold!(p, unreachable, |s, x| s.or({
          let (src, dst) = match self.pattern {
            Any(_) => (self.reduce(*x), dst),
            _ => (*self, dst.reduce(*x)),
          };
          bail!(src.intersects::<T, U>(dst, off, end))
        }))),
        (Equ(b), _) | (_, Equ(b)) => {
          let intervals = array!([(0, 0xff); 16], b, |x| (*x, *x));
          let pattern = Cat(intervals.split_at(b.len()).0);
          let (src, dst) = match self.pattern {
            Equ(_) => (self.reduce(pattern), dst),
            _ => (*self, dst.reduce(pattern)),
          };
          src.intersects::<T, U>(dst, off, end)
        }
        (Gte(b), _) | (_, Gte(b)) => {
          let endianness = match self.pattern {
            Gte(_) => self.endianness,
            _ => dst.endianness,
          };
          let (mut i, mut state) = (0, unreachable);
          while i < b.len() {
            let (mut j, mut pattern) = (0, [(0, 0xff); 16]);
            while j <= i {
              let k = if matches!(endianness, LE) { b.len() - j - 1 } else { j };
              pattern[k] = match j {
                _ if j + 1 == b.len() => (b[k], 0xff),
                _ if j == i => (b[k] + 1, 0xff),
                _ if j < i => (b[k], b[k]),
                _ => (0, 0xff),
              };
              j += 1;
            }
            i += 1;
            let pattern = Cat(pattern.split_at(b.len()).0);
            let (src, dst) = match self.pattern {
              Gte(_) => (self.reduce(pattern), dst),
              _ => (*self, dst.reduce(pattern)),
            };
            state = state.or(bail!(src.intersects::<T, U>(dst, off, end)));
          }
          Ok(state)
        }
        (Cat(a), Uninit) => {
          let byte = ByteInterval::from_tuple(a[0]);
          self.append(byte).reduce(Cat(a.split_at(1).1)).intersects::<T, U>(
            bytes_at::<U>(off + 1),
            off + 1,
            end,
          )
        }
        (Cat(a), Cat(b)) => {
          let (mut src, mut i, mut state) = (*self, 0, reachable);
          while i < a.len() && i < b.len() {
            let (a, b) = (ByteInterval::from_tuple(a[i]), ByteInterval::from_tuple(b[i]));
            let intersects = unwrap!(a.intersects(b), return Ok(unreachable));
            src = src.append(intersects);
            dst = dst.append(intersects);
            state.mutable &= b.len() == intersects.len();
            state.infallible &= a.len() == intersects.len();
            i += 1;
          }
          let (src, dst) = if i < a.len() {
            (src.reduce(Cat(a.split_at(i).1)), bytes_at::<U>(off + i))
          } else if i < b.len() {
            (bytes_at::<T>(off + i), dst.reduce(Cat(b.split_at(i).1)))
          } else {
            (bytes_at::<T>(off + i), bytes_at::<U>(off + i))
          };
          Ok(state.and(bail!(src.intersects::<T, U>(dst, off + i, end))))
        }
      }
    }
  }

  // impl IntegerSpec ------------------------------------------------------------------------------

  impl<T: Integer<Repr = T>> IntegerSpec<T> {
    #[inline]
    pub const fn any(default: T) -> Self {
      Self {
        default: num::to_bytes(default),
        size_field: None,
        endianness: Endianness::NE,
        byte_pattern: BytePattern::any::<T>(),
        _type: PhantomData,
      }
    }

    #[inline]
    pub const fn new(default: T, byte_pattern: BytePattern<'static>) -> Self {
      Self::any(default).with_bytes_pattern(byte_pattern)
    }

    #[inline]
    pub const fn with_default(self, default: T) -> Self {
      assert!(self.matches(default));
      Self { default: num::to_bytes(default), ..self }
    }

    #[inline]
    pub const fn with_bytes_pattern(self, byte_pattern: BytePattern<'static>) -> Self {
      assert!(byte_pattern.size() == mem::size_of::<T>());
      let this = Self { byte_pattern, ..self };
      assert!(this.matches_bytes(this.default));
      this
    }

    #[inline]
    pub const fn with_endianness(self, endianness: Endianness) -> Self {
      Self { endianness, ..self }
    }

    #[inline]
    pub const fn with_size_field(self, size_field: SizeField) -> Self {
      Self { size_field: Some(size_field), ..self }
    }

    #[inline]
    pub const fn is_uninit(&self) -> bool {
      matches!(self.byte_pattern, Uninit)
    }

    #[inline]
    pub const fn default(&self) -> T {
      num::from_bytes(self.default)
    }

    #[inline]
    pub const fn matches(&self, value: T) -> bool {
      self.matches_bytes(num::to_bytes(value))
    }

    #[inline]
    pub const fn matches_bytes(&self, bytes: [u8; 16]) -> bool {
      // SAFETY: `bytes` is a valid location to with enough bytes for `T`.
      unsafe { self.byte_pattern.matches_ptr(bytes.as_ptr(), self.endianness) }
    }

    #[inline]
    pub const fn matches_ref(&self, value: &T) -> bool {
      // SAFETY: `value` is a valid reference to `T`.
      unsafe { self.matches_ptr(value) }
    }

    /// # Safety
    /// `value` must be a valid reference to `T`, but not necessarily aligned on `T`.
    #[inline]
    pub const unsafe fn matches_ptr(&self, value: *const T) -> bool {
      self.byte_pattern.matches_ptr(value.cast(), self.endianness)
    }
  }

  // impl Unalign ----------------------------------------------------------------------------------

  impl<T> Unalign<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
      Self(ManuallyDrop::new(value))
    }

    #[inline]
    pub const fn deref(&self) -> &T {
      let x: &T = unsafe { transmute_unchecked(self) };
      const_assert_unaligned_raw(x);
      x
    }
  }

  impl<T: Copy> Unalign<T> {
    #[inline]
    pub const fn into_inner(self) -> T {
      ManuallyDrop::into_inner(self.0)
    }
  }

  impl<T> From<T> for Unalign<T> {
    #[inline]
    fn from(value: T) -> Self {
      Unalign::new(value)
    }
  }

  impl<T> Deref for Unalign<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
      Unalign::deref(self)
    }
  }

  impl<T> DerefMut for Unalign<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
      unaligned_mut!(self.0)
    }
  }

  impl<T: Debug + Copy> Debug for Unalign<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(&self.into_inner(), f)
    }
  }

  impl<T: Display + Copy> Display for Unalign<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Display::fmt(&self.into_inner(), f)
    }
  }

  // impl Size -------------------------------------------------------------------------------------

  unsafe impl<T: Integer> Integer for Size<T> {
    type Repr = T::Repr;
    const SPEC: IntegerSpec<Self::Repr> = T::SPEC.with_size_field(SizeField::Size);
  }

  impl<T: Integer> Size<T> {
    #[inline]
    pub const fn into_usize(self) -> usize {
      num::cast::<_, u64>(self.inner) as usize
    }
  }

  // impl Field -------------------------------------------------------------------------------------

  unsafe impl Field for () {
    type Type = ();
    type PrevField = ();
    const OFFSET: usize = 0;
    const INDEX: usize = 0;
  }

  // impl DataField --------------------------------------------------------------------------------

  impl<C: ?Sized + Data, T: Copy + Data> Copy for DataField<C, T> {}

  impl<C: ?Sized + Data, T: Copy + Data> Clone for DataField<C, T> {
    fn clone(&self) -> Self {
      *self
    }
  }

  impl<C: ?Sized + Data, T: ?Sized + Data> DataField<C, T> {
    const MIN_SIZE: <T::Integer as Integer>::Repr = num::cast(min_size_of::<C>() as u64);
    const MIN_SIZE_BYTES: [u8; 16] = num::to_bytes(Self::MIN_SIZE);
    const DEFAULT: <T::Integer as Integer>::Repr = num::cast(Self::MIN_SIZE);
  }

  // impl Data -------------------------------------------------------------------------------------

  unsafe impl<T: Integer> Data for T {
    type Item = ();
    type TailSliceItem = ();
    type Sized = Self;
    type TailField = ();
    type Integer = T;
    type Container = ();
    const LAYOUT: Layout<Self> = Layout::Integer { spec: T::SPEC };
  }

  unsafe impl<T: Copy + Data> Data for [T] {
    type Item = T;
    type TailSliceItem = T;
    type Sized = ();
    type TailField = ();
    type Integer = ();
    type Container = ();
    const LAYOUT: Layout<Self> = Layout::Slice;
  }

  unsafe impl<T: Copy + Data, const N: usize> Data for [T; N] {
    type Item = T;
    type TailSliceItem = ();
    type Sized = Self;
    type TailField = ();
    type Integer = ();
    type Container = ();
    const LAYOUT: Layout<Self> = Layout::Array;
  }

  unsafe impl<T: ?Sized + Data> Data for ManuallyDrop<T> {
    type Item = T::Item;
    type TailSliceItem = T::TailSliceItem;
    type Sized = T::Sized;
    type TailField = T::TailField;
    type Integer = T::Integer;
    type Container = T::Container;
    const LAYOUT: Layout<Self> = {
      assert!(!mem::needs_drop::<T>());
      match T::LAYOUT {
        Layout::Integer { spec } => Layout::Integer { spec },
        Layout::Slice => Layout::Slice,
        Layout::Array => Layout::Array,
        Layout::Struct => Layout::Struct,
      }
    };
  }

  unsafe impl<C: ?Sized + Data, T: ?Sized + Data> Data for DataField<C, T> {
    type Item = T::Item;
    type TailSliceItem = T::TailSliceItem;
    type Sized = T::Sized;
    type TailField = T::TailField;
    type Integer = T::Integer;
    type Container = C;
    const LAYOUT: Layout<Self> = {
      match T::LAYOUT {
        Layout::Integer { spec } => {
          let spec = match spec.size_field {
            Some(SizeField::Size) => {
              spec.with_default(Self::DEFAULT).with_bytes_pattern(if is_sized::<C>() {
                Equ(Self::MIN_SIZE_BYTES.split_at(mem::size_of::<T::Integer>()).0)
              } else {
                Gte(Self::MIN_SIZE_BYTES.split_at(mem::size_of::<T::Integer>()).0)
              })
            }
            None => spec,
          };
          Layout::Integer { spec }
        }
        Layout::Slice => Layout::Slice,
        Layout::Array => Layout::Array,
        Layout::Struct => Layout::Struct,
      }
    };
  }

  pub struct Tuple0<A: Copy + Data, B: Copy + Data>(PhantomData<(A, B)>);
  pub struct Tuple1<A: Copy + Data, B: Copy + Data>(PhantomData<(A, B)>);

  unsafe impl<A: Copy + Data, B: Copy + Data> Field for Tuple0<A, B> {
    type Type = DataField<(A, B), A>;
    type PrevField = ();
    const OFFSET: usize = offset_of!((A, B), 0);
    const INDEX: usize = 0;
  }

  unsafe impl<A: Copy + Data, B: Copy + Data> Field for Tuple1<A, B> {
    type Type = DataField<(A, B), B>;
    type PrevField = Tuple0<A, B>;
    const OFFSET: usize = offset_of!((A, B), 1);
    const INDEX: usize = 1;
  }

  unsafe impl<A: Copy + Data, B: Copy + Data> Data for (A, B) {
    type Item = ();
    type TailSliceItem = B::TailSliceItem;
    type Sized = Self;
    type TailField = Tuple1<A, B>;
    type Integer = ();
    type Container = ();
    const LAYOUT: Layout<Self> = Layout::Struct;
  }

  // impl Integer ----------------------------------------------------------------------------------

  unsafe impl<T: ?Sized + 'static> Integer for PhantomData<T> {
    type Repr = Self;
    const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::any(PhantomData);
  }

  macro_rules! impl_word {
    ($(($ty:ty as $raw:ty where $($spec:tt)+))+) => {
      $(
        unsafe impl Integer for $ty {
          type Repr = $raw;
          const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::$($spec)+;
        }
      )+
    };
  }

  impl_word!(
    (() as () where any(()))
    (bool as u8 where new(0, Cat(&[(0, 1)])))
    (u8 as u8 where any(0)) (u16 as u16 where any(0)) (u32 as u32 where any(0))
    (u64 as u64 where any(0)) (u128 as u128 where any(0))
    (i8 as i8 where any(0)) (i16 as i16 where any(0)) (i32 as i32 where any(0))
    (i64 as i64 where any(0)) (i128 as i128 where any(0))
    (Option<NonZeroU8> as u8 where any(0)) (Option<NonZeroU16> as u16 where any(0))
    (Option<NonZeroU32> as u32 where any(0)) (Option<NonZeroU64> as u64 where any(0))
    (Option<NonZeroU128> as u128 where any(0))
    (NonZeroU8 as u8 where new(1, Gte(&1u8.to_ne_bytes())))
    (NonZeroU16 as u16 where new(1, Gte(&1u16.to_ne_bytes())))
    (NonZeroU32 as u32 where new(1, Gte(&1u32.to_ne_bytes())))
    (NonZeroU64 as u64 where new(1, Gte(&1u64.to_ne_bytes())))
    (NonZeroU128 as u128 where new(1, Gte(&1u128.to_ne_bytes())))
  );
}
