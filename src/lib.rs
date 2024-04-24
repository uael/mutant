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
#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests;

use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::Deref;

pub use mutant_derive::Data;

// Internal public API -----------------------------------------------------------------------------
// Even if part of the public API, the following items are not meant to be used directly by the
// user. They are used internally by the derive macro and other parts of the library and are subject
// to change without notice.
// -------------------------------------------------------------------------------------------------

/// Represents either an initialized or uninitialized byte layout.
#[doc(hidden)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum ByteLayout<'a> { Uninit, Init(InitLayout<'a, usize>), }

/// Represents the layout of a type in memory.
///
/// The [`Layout`] enum is used to categorize the memory representation of a type, which is
/// essential for safe and correct transmutation. It describes how data is laid out in memory,
/// providing the necessary context for transmutation operations.
#[doc(hidden)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone)]
pub enum Layout<T: ?Sized + Data> {
  Slice, Array, Struct,
  Integer { spec: IntegerSpec<'static, T::Bits>, },
}

/// Enumerates the most generic bit patterns for individual bytes.
///
/// # Internals
/// This enum is used to describe potential byte patterns within a type `A`. During transmutation
/// checks into another type `B`, as implemented in [`InitLayout::intersects`], this is utilized
/// through the [`BitsPattern::contains_any`] and [`BitsPattern::contains_all`] methods to compute
/// the [transmutability](internal::Transmutability) from `A` to `B`, over each potential
/// combination.
#[doc(hidden)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone)]
pub enum BitsPattern<'a, T> { #[cfg(test)] Uninit, Any, Allow(&'a [T]), Disallow(&'a T), }

#[doc(hidden)]
#[derive(Debug, Copy, Clone)]
pub struct IntegerSpec<'a, T> {
  pub default: &'a T,
  pub bits_pattern: BitsPattern<'a, T>,
}

/// Describes an initialized byte layout at a specific offset within a larger [`IntegerLayout`].
#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct InitLayout<'a, Offset = ()> {
  offset: Offset,
  width: u8,                     // Size of the integer in bytes.
  bits_pattern: u8,              // Bits pattern type, see [`bits`] module.
  bits_pattern_values: &'a [u8], // Bits pattern value(s) encoded as bytes, i.e. `[u<width>]`.
}

/// This is used as a compile-time field list for struct layout.
#[doc(hidden)]
pub unsafe trait Repr<T: ?Sized + Data> {
  type Type: ?Sized + Data;
  type Rest: Repr<T>;
  const OFFSET: usize;
  const FIELDS: usize;
}

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
    // [1] TODO(#429), TODO(https://github.com/rust-lang/rust/pull/116675):
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
  ($($name:ident ($($t:ident)*) ($($f:tt)*) ($tt:ident $ft:tt))+) => {$(
    #[doc(hidden)]
    #[repr(C, packed)]
    #[derive(Data, Copy, Clone)]
    #[data(crate)]
    pub struct $name<$($t,)* $tt: Data + ?Sized>($(pub $t,)* pub ::core::mem::ManuallyDrop<$tt>);

    impl<$($t: Copy + Debug,)* $tt: Data + Debug + ?Sized>
      Debug for $name<$($t,)* $tt>
    {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_tuple("");
        $(d.field(&{ self.$f });)*
        unaligned_ref!(self.$ft @tail in d.field(&tail.deref()));
        d.finish()
      }
    }

    impl<$($t: Copy + PartialEq,)* $tt: Data + PartialEq + ?Sized>
      PartialEq for $name<$($t,)* $tt>
    {
      fn eq(&self, other: &Self) -> bool {
        $(if !{ self.$f }.eq(&{ other.$f }) { return false; })*
        unaligned_ref!(self.$ft @a in unaligned_ref!(other.$ft @b in a.deref().eq(b.deref())))
      }
    }

    impl<$($t: Copy + Eq,)* $tt: Data + Eq + ?Sized> Eq for $name<$($t,)* $tt> {}
  )+};
}

decl_pack! {
  Pack1 () () (A 0)
  Pack2 (A) (0) (B 1)
  Pack3 (A B) (0 1) (C 2)
  Pack4 (A B C) (0 1 2) (D 3)
  Pack5 (A B C D) (0 1 2 3) (E 4)
  Pack6 (A B C D E) (0 1 2 3 4) (F 5)
  Pack7 (A B C D E F) (0 1 2 3 4 5) (G 6)
  Pack8 (A B C D E F G) (0 1 2 3 4 5 6) (H 7)
  Pack9 (A B C D E F G H) (0 1 2 3 4 5 6 7) (I 8)
}

/// Assert at compile-time that a [`Data`] type is unaligned (i.e. his alignment is equals to 1).
#[inline]
#[doc(hidden)]
#[rustfmt::skip]
pub const fn const_assert_unaligned<T: ?Sized + Data>(_: *const T) {
  struct Assert<T: ?Sized>(*const T);
  impl<T: ?Sized + Data> Assert<T> { const ASSERT_UNALIGNED: () = assert!(align_of::<T>() == 1); }
  #[allow(clippy::let_unit_value)]
  let _ = Assert::<T>::ASSERT_UNALIGNED;
}

/// Identity function used by the [`layout`] macro.
#[inline]
#[doc(hidden)]
pub const fn raw<T: Integer>(raw: T::Bits) -> T::Bits {
  assert!(mem::size_of::<T>() == mem::size_of::<T::Bits>());
  raw
}

/// Constants used to declare bits pattern for [`IntegerLayout`] in the [`layout`] macro.
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

/// Marker trait for plain integer types (e.g. integers or enum discriminants).
///
/// # Safety
/// This marker trait isn't meant to be implemented by the user. If you do so, you're on your own.
pub unsafe trait Integer: Copy + 'static {
  /// Bits type representation of `Self` that can accept any bit pattern.
  /// The size of `Self::Bits` is equals to the size of `Self`.
  type Bits: Integer<Bits = Self::Bits>;

  /// # Safety
  /// Implementor is responsible for `SPEC` to represents only valid values for `Self`.
  #[doc(hidden)]
  const SPEC: IntegerSpec<'static, Self::Bits>;
}

/// Marker trait for "data" types. "data" types are types that can be safely transmuted between each
/// other. They contain no references, pointers or drops types, but they can contain uninitialized
/// bytes.
///
/// # Safety
/// This marker trait isn't meant to be implemented by the user. If you do so, you're on your own.
pub unsafe trait Data: 'static {
  /// Type of slice or array item.
  type Item: Copy + Data;

  /// Type of the trailing slice item.
  type TailItem: Copy + Data;

  /// [`Sized`] representation of `Self`, equals to `Self` for [`Sized`] types.
  #[doc(hidden)]
  type Sized: Copy + Data;

  /// Fields of `Self`, only in case of [`Layout::Struct`] layout.
  #[doc(hidden)]
  type Repr: Repr<Self>;

  /// [`Integer`] representation of `Self`, only in case of [`Layout::Integer`] layout.
  #[doc(hidden)]
  type Bits: Integer<Bits = Self::Bits>;

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
/// // let b = unaligned_ref!(&unaligned, b); // this does not compile.
/// unaligned_ref!(unaligned.b @b in assert_eq!(&2, b));
/// ```
#[macro_export]
macro_rules! unaligned_ref {
  ($base:expr, $($fields:tt).+) => {{
    let base: &_ = $base;
    let addr = core::ptr::addr_of!(base.$($fields).+);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `addr` came from valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
  ($base:expr, $($fields:tt).+ @$var:ident in $ex:expr) => {{
    let base: &_ = $base;
    let mut addr = core::ptr::addr_of!(base.$($fields).+);
    if $crate::is_sized_val_raw(addr) && $crate::align_of_val_raw(addr) > 1 {
      #[inline] // A function for typing inference.
      const fn downcast<T: ?Sized + $crate::Data>(ptr: *const T) -> *const T::Sized { ptr.cast() }

      // `T` is `Sized` hence `T::Sized` is equals to `T`:
      // 1. Load `T::Sized` to stack.
      // SAFETY: `addr` came from valid reference hence not a dangling pointer.
      let sized = unsafe { ::core::ptr::read_unaligned(downcast(addr)) };
      // 2. Make `addr` point to `sized` instead.
      addr = $crate::ptr::from_raw_parts($crate::ptr::from_ref(&sized).cast(), 0);
      // SAFETY: `addr` came from valid reference hence not a dangling pointer.
      let $var = unsafe { &*addr };
      $ex
    } else {
      // `T` is `!Sized`: assert that `T` has no alignment requirement.
      assert!($crate::align_of_val_raw(addr) == 1);
      // SAFETY:
      // - `addr` came from valid reference hence not a dangling pointer.
      // - The call to `assert_unaligned` ensure field un-alignment.
      let $var = unsafe { &*addr };
      $ex
    }
  }};
  ($base:tt.$field:tt.$($fields:tt).+ $(@$var:ident in $ex:expr)?) => {{
    let base = $crate::unaligned_ref!($base.$field);
    $crate::unaligned_ref!(base.$($fields).+ $(@$var in $ex)?)
  }};
  ($base:tt.$field:tt $(@$var:ident in $ex:expr)?) => (
    $crate::unaligned_ref!($base, $field $(@$var in $ex)?)
  );
}

/// Safely mutably reference unaligned data.
#[macro_export]
macro_rules! unaligned_mut {
  ($base:expr, $($fields:tt).+) => {{
    let base: &mut _ = $base;
    let addr = core::ptr::addr_of_mut!(base.$($fields).+);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `addr` came from valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_mut!($base.$field);
    $crate::unaligned_mut!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => ($crate::unaligned_mut!($base, $field));
}

/// Same as [`align_of`](mem::align_of), returns the minimum alignment of a type in bytes.
/// With the difference that it also accept unsized types with relaxed `?`[`Sized`] bound.
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
/// relaxed `?`[`Sized`] bound.
#[inline]
pub const fn align_of_val<T: ?Sized>(_: &T) -> usize {
  align_of::<T>()
}

/// Returns the minimum alignment of a value address in bytes. Also accept unsized types with
/// relaxed `?`[`Sized`] bound.
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

#[inline]
pub const fn equals_ref<T: Integer>(a: &T, b: &T) -> bool {
  // SAFETY: `a` and `b` are valid references.
  unsafe { equals_raw(a, b) }
}

/// # Safety
/// Both `a` and `b` must point to valid `T` memory locations, but not necessarily aligned on `T`.
#[inline]
pub const unsafe fn equals_raw<T: Integer>(a: *const T, b: *const T) -> bool {
  use core::ptr::read_unaligned;
  use num::*;

  match T::SPEC.bits_pattern {
    #[cfg(test)]
    BitsPattern::Uninit => return false,
    BitsPattern::Allow(values) if values.len() == 1 => return true,
    _ => {}
  }

  macro_rules! inner {
    ($($n:literal $ty:ty [$($op:tt)+])+) => (
      match mem::size_of::<T>() {
        $($n => (read_unaligned(a.cast::<$ty>())) $($op)+ (read_unaligned(b.cast::<$ty>())),)+
        _ => unreachable!(),
      }
    );
  }

  inner!(
    1 u8 [==] 2 u16 [==] 3 u24 [.equals] 4 u32 [==] 5 u40 [.equals] 6 u48 [.equals] 7 u56 [.equals]
    8 u64 [==] 9 u72 [.equals] 10 u80 [.equals] 11 u88 [.equals] 12 u96 [.equals] 13 u104 [.equals]
    14 u112 [.equals] 15 u120 [.equals] 16 u128 [==]
  )
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
  #[inline]
  const fn repr_is_sized<R: Repr<T>, T: ?Sized + Data>() -> bool {
    match R::FIELDS {
      0 => false,
      1 => is_sized::<R::Type>(),
      _ => repr_is_sized::<R::Rest, T>(),
    }
  }

  match T::LAYOUT {
    Layout::Slice => false,
    Layout::Struct => repr_is_sized::<T::Repr, T>(),
    _ => true,
  }
}

#[inline]
pub const fn matches<T: Integer>(raw: &T::Bits) -> bool {
  T::SPEC.matches(raw)
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::Sized>()
}

#[inline]
pub const fn min_size_of_val<T: ?Sized + Data>(_: &T) -> usize {
  min_size_of::<T>()
}

#[inline]
pub const fn min_size_of_val_raw<T: ?Sized + Data>(_: *const T) -> usize {
  min_size_of::<T>()
}

#[inline]
pub const fn size_of_tail<T: ?Sized + Data>(val: &T) -> usize {
  ptr::metadata(val) * size_of_tail_item::<T>()
}

#[inline]
pub const fn size_of_tail_item<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::TailItem>()
}

#[inline]
pub const fn size_of_tail_raw<T: ?Sized + Data>(val: *const T) -> usize {
  ptr::metadata(val) * size_of_tail_item::<T>()
}

#[inline]
pub const fn size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of::<T>() + size_of_tail(val)
}

#[inline]
pub const fn size_of_val_raw<T: ?Sized + Data>(val: *const T) -> usize {
  min_size_of::<T>() + size_of_tail_raw(val)
}

#[inline]
pub const fn size_of_with<T: ?Sized + Data>(metadata: usize) -> usize {
  min_size_of::<T>() + metadata * size_of_tail_item::<T>()
}

#[inline]
pub const fn split_at<T: ?Sized + Data>(val: &T, mid: usize) -> (&T, &[T::TailItem]) {
  match try_split_at(val, mid) {
    Some(result) => result,
    _ => panic!("`try_split_at` failed"),
  }
}

#[inline]
pub const fn tail_of<T: ?Sized + Data>(val: &T) -> &[T::TailItem] {
  let ptr: *const T = val;
  let ptr = unsafe { ptr.cast::<T::Sized>().add(1).cast::<T::TailItem>() };
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
pub const unsafe fn transmute_unchecked<T: Copy, U: Copy>(val: T) -> U {
  union Transmute<T: Copy, U: Copy> {
    val: T,
    dst: U,
  }
  Transmute { val }.dst
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
    internal::Transmutation::<T, U>::CHECK_TRUNCATE,
    internal::Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH,
    internal::Transmutation::<T, U>::CHECK_NO_INVALID_BIT_PATTERN,
  );

  unsafe { transmute_unchecked(val) }
}

#[inline]
pub const fn try_split_at<T: ?Sized + Data>(val: &T, mid: usize) -> Option<(&T, &[T::TailItem])> {
  if mid < min_size_of::<T>() || mid > size_of_val(val) {
    return None;
  }
  if mid == size_of_val(val) {
    return Some((val, &[]));
  }
  let tail = mid - min_size_of::<T>();
  if tail % mem::size_of::<T::TailItem>() != 0 {
    return None;
  }
  let ptr: *const u8 = (val as *const T).cast();
  let val_ptr = ptr::from_raw_parts::<T>(ptr.cast(), tail / mem::size_of::<T::TailItem>());
  let rest_ptr = ptr::from_raw_parts::<[T::TailItem]>(
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
  match internal::is_bits_valid(ptr) {
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
  match internal::is_bits_valid(ptr) {
    true => Ok(unsafe { &mut *ptr }),
    _ => Err(Error::InvalidBitPattern),
  }
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
  match internal::is_bits_valid(ptr) {
    true => Ok(unsafe { &*ptr }),
    _ => Err(Error::InvalidBitPattern),
  }
}

#[inline]
pub const fn try_truncate_ref<T: ?Sized + Data, U: ?Sized + Data>(val: &T) -> Result<&U, Error> {
  #[allow(clippy::let_unit_value)]
  let _ = internal::Transmutation::<T, U>::CHECK;

  if !is_aligned_to::<T>(val, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  // TODO
  let metadata = match internal::try_convert_metadata::<T, U>(ptr::metadata(val)) {
    Ok(metadata) => metadata,
    Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
  };
  let ptr = ptr::from_raw_parts::<U>(ptr::from_ref(val).cast(), metadata);
  match internal::is_bits_valid(ptr) {
    true => Ok(unsafe { &*ptr }),
    _ => Err(Error::InvalidBitPattern),
  }
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

  macro_rules! impl_num {
    (($T:ident $Bits:ident $N:literal $ordering:ident)) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $T(pub [u8; $N]);

      unsafe impl Integer for $T {
        type Bits = $T;
        const SPEC: IntegerSpec<'static, Self::Bits> = IntegerSpec {
          default: &Self::MIN,
          bits_pattern: BitsPattern::Any,
        };
      }

      impl $T {
        pub const MIN: Self = Self::from_raw($Bits::MIN);
        pub const MAX: Self = Self::from_raw($Bits::MAX);

        #[inline] pub const fn to_ne_bytes(self) -> [u8; $N] { swap!($ordering => ne, self).0 }
        #[inline] pub const fn to_le_bytes(self) -> [u8; $N] { swap!($ordering => le, self).0 }
        #[inline] pub const fn to_be_bytes(self) -> [u8; $N] { swap!($ordering => be, self).0 }

        #[inline] pub const fn from_ne_bytes(b: [u8; $N]) -> Self { swap!(ne => $ordering, Self(b)) }
        #[inline] pub const fn from_le_bytes(b: [u8; $N]) -> Self { swap!(le => $ordering, Self(b)) }
        #[inline] pub const fn from_be_bytes(b: [u8; $N]) -> Self { swap!(be => $ordering, Self(b)) }

        #[inline]
        pub const fn new(raw: $Bits) -> Self {
          Self::from_raw(raw)
        }

        #[inline]
        pub const fn from_raw(raw: $Bits) -> Self {
          let (mut off, mut bytes) = (0, [0; $N]);
          while off < $N {
            bytes[off] = (raw >> (le_or_be!($ordering, off, $N - off - 1) * 8)) as u8;
            off += 1;
          }
          Self(bytes)
        }

        #[inline]
        pub const fn into_inner(self) -> $Bits {
          self.into_raw()
        }

        #[inline]
        pub const fn into_raw(self) -> $Bits {
          let (mut off, mut raw) = (0, 0);
          while off < $N {
            raw |= (self.0[off] as $Bits) << (le_or_be!($ordering, off, $N - off - 1) * 8);
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

      impl From<$Bits> for $T {
        #[inline]
        fn from(raw: $Bits) -> Self {
          Self::from_raw(raw)
        }
      }

      impl From<$T> for $Bits {
        #[inline]
        fn from(value: $T) -> Self {
          value.into_raw()
        }
      }

      impl Debug for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as Debug>::fmt(&self.into_raw(), f)
        }
      }

      impl Display for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as Display>::fmt(&self.into_raw(), f)
        }
      }

      impl Binary for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as Binary>::fmt(&self.into_raw(), f)
        }
      }

      impl Octal for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as Octal>::fmt(&self.into_raw(), f)
        }
      }

      impl LowerHex for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as LowerHex>::fmt(&self.into_raw(), f)
        }
      }

      impl UpperHex for $T {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          <$Bits as UpperHex>::fmt(&self.into_raw(), f)
        }
      }
    };
    ($($tt:tt)+) => {
      $(impl_num!($tt);)+
    };
  }

  macro_rules! impl_const_num {
    (($ConstT:ident $T:ident)) => {
      impl_const_num!(($ConstT $T $T V));
    };
    (($ConstT:ident $T:ident $VT:ident)) => {
      impl_const_num!(($ConstT $T $VT $T::from_raw(V)));
    };
    (($ConstT:ident $T:ident $VT:ident $v2raw:expr)) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $ConstT<const V: $VT>($T);

      unsafe impl<const V: $VT> Integer for $ConstT<V> {
        type Bits = $T;
        const SPEC: IntegerSpec<'static, Self::Bits> = IntegerSpec {
          default: &$v2raw,
          bits_pattern: BitsPattern::Allow(&[$v2raw]),
        };
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

  #[rustfmt::skip]
  macro_rules! le_or_be {
    (ne, $le:expr, $be:expr $(,)?) => { if cfg!(target_endian = "little") { $le } else { $be } };
    (le, $le:expr, $be:expr $(,)?) => { $le };
    (be, $le:expr, $be:expr $(,)?) => { $be };
  }

  #[rustfmt::skip]
  macro_rules! swap {
    (ne => ne, $e:expr) => { $e };
    (le => le, $e:expr) => { $e };
    (be => be, $e:expr) => { $e };
    (le => be, $e:expr) => { $e.swap_bytes() };
    (be => le, $e:expr) => { $e.swap_bytes() };
    (ne => le, $e:expr) => { if cfg!(target_endian = "little") { $e } else { $e.swap_bytes() } };
    (ne => be, $e:expr) => { if cfg!(target_endian = "little") { $e.swap_bytes() } else { $e } };
    (le => ne, $e:expr) => { if cfg!(target_endian = "little") { $e } else { $e.swap_bytes() } };
    (be => ne, $e:expr) => { if cfg!(target_endian = "little") { $e.swap_bytes() } else { $e } };
  }

  impl_num! {
    (u16le u16 2 le) (u16be u16 2 be)
    (u24le u32 3 le) (u24be u32 3 be) (u24 u32 3 ne)
    (u32le u32 4 le) (u32be u32 4 be)
    (u40le u64 5 le) (u40be u64 5 be) (u40 u64 5 ne)
    (u48le u64 6 le) (u48be u64 6 be) (u48 u64 6 ne)
    (u56le u64 7 le) (u56be u64 7 be) (u56 u64 7 ne)
    (u64le u64 8 le) (u64be u64 8 be)
    (u72le u128 9 le) (u72be u128 9 be) (u72 u128 9 ne)
    (u80le u128 10 le) (u80be u128 10 be) (u80 u128 10 ne)
    (u88le u128 11 le) (u88be u128 11 be) (u88 u128 11 ne)
    (u96le u128 12 le) (u96be u128 12 be) (u96 u128 12 ne)
    (u104le u128 13 le) (u104be u128 13 be) (u104 u128 13 ne)
    (u112le u128 14 le) (u112be u128 14 be) (u112 u128 14 ne)
    (u120le u128 15 le) (u120be u128 15 be) (u120 u128 15 ne)
    (u128le u128 16 le) (u128be u128 16 be)
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

  use ByteLayout::*;
  use MetadataChecks::*;

  /// Unwrap a `Result` and panic with the error if it is `Err`.
  macro_rules! unwrap {
    ($e:expr $(,)?) => {
      match $e {
        Ok(ok) => ok,
        Err(err) => panic!("{}", err),
      }
    };
  }

  #[inline]
  const fn repr_contains<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> bool {
    offset >= R::OFFSET && (!is_sized::<R::Type>() || offset < R::OFFSET + min_size_of::<R::Type>())
  }

  #[inline]
  pub(crate) const fn byte_layout_at<T: ?Sized + Data>(
    offset: usize,
  ) -> Option<ByteLayout<'static>> {
    #[inline]
    const fn repr_byte_layout_at<R: Repr<T>, T: ?Sized + Data>(
      offset: usize,
    ) -> Option<ByteLayout<'static>> {
      match R::FIELDS {
        // Reached the end of the struct, returns an uninitialized layout.
        0 => Some(Uninit),
        // `offset` does offset in field, defer to its inner layout.
        _ if repr_contains::<R, T>(offset) => byte_layout_at::<R::Type>(offset - R::OFFSET),
        // `offset` does not offset in field, continue with next field.
        _ => repr_byte_layout_at::<R::Rest, T>(offset),
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
      Layout::Slice | Layout::Array => {
        byte_layout_at::<T::Item>(offset % mem::size_of::<T::Item>())
      }
      Layout::Struct => repr_byte_layout_at::<T::Repr, T>(offset),
      #[cfg(test)]
      Layout::Integer { spec } if spec.is_uninit() => Some(Uninit),
      Layout::Integer { spec } => Some(Init(spec.init_at(offset))),
    }
  }

  #[inline]
  pub(crate) const fn default_at<T: ?Sized + Data>(offset: usize) -> Option<u8> {
    #[inline]
    const fn repr_default_at<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> Option<u8> {
      match R::FIELDS {
        // Reached the end of the struct.
        0 => None,
        // `offset` does offset in field, defer to its inner layout.
        _ if repr_contains::<R, T>(offset) => default_at::<R::Type>(offset - R::OFFSET),
        // `offset` does not offset in field, continue with next field.
        _ => repr_default_at::<R::Rest, T>(offset),
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
      Layout::Struct => repr_default_at::<T::Repr, T>(offset),
      #[cfg(test)]
      Layout::Integer { spec } if spec.is_uninit() => None,
      Layout::Integer { spec } => {
        Some(unsafe { *ptr::from_ref(spec.default).cast::<u8>().add(offset) })
      }
    }
  }

  #[inline]
  pub(crate) const fn convert_metadata<T: ?Sized + Data, U: ?Sized + Data>(
    metadata: usize,
  ) -> usize {
    #[allow(clippy::let_unit_value)]
    let _ = (Transmutation::<T, U>::CHECK, Transmutation::<T, U>::CHECK_NO_SIZE_MISMATCH);

    match size_of_tail_item::<U>() {
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
      Err(n) => Err(n * size_of_tail_item::<T>() as isize),
      _ if size_of_tail_item::<U>() == 0 => Ok(0),
      _ => Ok((size_of_with::<T>(metadata) - min_size_of::<U>()) / size_of_tail_item::<U>()),
    }
  }

  #[inline]
  pub(crate) const fn is_bits_valid<Dst: ?Sized + Data>(ptr: *const Dst) -> bool {
    #[inline]
    const fn repr_field<R: Repr<T>, T: ?Sized + Data>(ptr: *const T) -> *const R::Type {
      assert!(R::FIELDS != 0);
      ptr::from_raw_parts(
        // SAFETY: `ptr` came from a valid reference and `R::OFFSET` is in type bounds.
        unsafe { ptr.cast::<u8>().add(R::OFFSET) }.cast(),
        match is_sized::<R::Type>() {
          true => 0,
          _ => ptr::metadata(ptr),
        },
      )
    }

    #[inline]
    const fn repr_is_bits_valid<R: Repr<Dst>, Dst: ?Sized + Data>(ptr: *const Dst) -> bool {
      match R::FIELDS {
        0 => true,
        _ => is_bits_valid(repr_field::<R, _>(ptr)) && repr_is_bits_valid::<R::Rest, _>(ptr),
      }
    }

    match Dst::LAYOUT {
      Layout::Struct => repr_is_bits_valid::<Dst::Repr, Dst>(ptr),
      Layout::Slice | Layout::Array => {
        // Compute the length of `ptr`.
        let len = match Dst::LAYOUT {
          Layout::Array => min_size_of::<Dst>() / mem::size_of::<Dst::Item>(),
          _ => ptr::metadata(ptr),
        };
        // Call `is_bits_valid` on each item of `ptr`.
        let mut i = 0;
        while i < len {
          // SAFETY: `ptr` came from a valid reference and `i` is in type bounds.
          if !is_bits_valid(unsafe { ptr.cast::<Dst::Item>().add(i) }) {
            return false;
          }
          i += 1;
        }
        true
      }
      Layout::Integer { spec } => {
        // SAFETY: `ptr` is valid for read since
        // - `ptr` came from a valid reference,
        // - Read type `Dst::Integer::Bits` is any bit pattern.
        unsafe { spec.matches_ptr(ptr.cast()) }
      }
    }
  }

  /// Performs static analysis to check if the transmutation from `Src` to `Dst` is safe.
  ///
  /// This function computes various checks to ensure that the transmutation adheres to the
  /// rules around alignment, size, and valid bit patterns. It takes into account the sizes
  /// and layouts of the source (`Src`) and destination (`Dst`) types, especially considering
  /// dynamically-sized types and their metadata.
  ///
  /// # Type Parameters
  ///
  /// - `Src`: The source type for the transmutation.
  /// - `Dst`: The destination type for the transmutation.
  ///
  /// # Returns
  ///
  /// `Result<StaticAnalyze, &str>`: A `Result` which is `Ok` if the transmutation is
  /// considered safe, containing a `StaticAnalyze` struct with details of the analysis. Returns
  /// `Err` with a string literal describing the reason if the transmutation is potentially unsafe.
  pub(crate) const fn static_analyze<Src, Dst>() -> Result<StaticAnalyze, &'static str>
  where
    Src: ?Sized + Data,
    Dst: ?Sized + Data,
  {
    // Calculate header (fixed-size part) and tail (variable-size part) sizes for both
    // source and destination types.
    let (sh, st) = (min_size_of::<Src>(), size_of_tail_item::<Src>());
    let (dh, dt) = (min_size_of::<Dst>(), size_of_tail_item::<Dst>());
    // Initial calculations to determine the minimum adjustments needed to align
    // source and destination sizes.
    let mut min_at_count = 0;
    if sh < dh {
      if st == 0 {
        return Err("source too small");
      }
      let missing = dh - sh;
      if dt == 0 && missing % st != 0 {
        return Err(
          "missing bytes to fit the target is not a multiple of the source DST trailing slice \
           item size",
        );
      }
      min_at_count = (missing + st - 1) / st; // Ceiling division.
    } else if sh > dh && dt != 0 {
      let remaining = sh - dh;
      if st == 0 && remaining % dt != 0 {
        return Err(
          "remaining bytes to fit in the target is not a multiple of the target DST trailing \
           slice item size",
        );
      }
    }
    let maybe_size_mismatch = match (st, dt) {
      (0, _) => None,
      (st, 0) if sh < dh => Some(Is((dh - sh) / st)),
      (_, 0) => Some(IsEmpty),
      (st, dt) => {
        // Greatest common divisor and least common multiple calculations to
        // determine the alignment and size compatibility.
        #[inline]
        const fn gcd(a: usize, b: usize) -> usize {
          match b {
            0 => a,
            _ => gcd(b, a % b),
          }
        }

        let gcd = gcd(st, dt);
        let lcm = (st * dt) / gcd;
        while (sh + min_at_count * st - dh) % dt != 0 {
          if min_at_count * st > lcm {
            return Err("DST source cannot catchup on DST target");
          }
          min_at_count += 1;
        }
        match (min_at_count, dt / gcd) {
          (0, 1) => None,
          (0, m) => Some(IsMultipleOf(m)),
          (n, 1) => Some(IsAtLeast(n)),
          (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
        }
      }
    };
    // Compute the minimum size in bytes need to fully check `Src` and `Dst` bit patterns
    // compatibility (transmutability).
    let min_check_size = match maybe_size_mismatch {
      None if st != 0 => sh + st,
      None => dh,
      Some(IsEmpty) => sh,
      Some(IsMultipleOf(m)) => sh + (m * st),
      Some(Is(n)) => sh + (n * st),
      Some(IsAtLeast(n)) => sh + (n * st) + st,
      Some(IsAtLeastThenMultipleOf(n, m)) => sh + (n * st) + (m * st),
    };
    // Check for `Src` and `Dst` bit patterns compatibility (transmutability).
    let mut src = Source::empty();
    let (mut src_offset, mut dst_offset) = (0, 0);
    while dst_offset < min_check_size {
      let dst = match byte_layout_at::<Dst>(dst_offset) {
        Some(layout) => {
          dst_offset += layout.width();
          layout
        }
        _ => panic!("internal error"),
      };
      while src.width < dst.width() {
        src = src.append(match byte_layout_at::<Src>(src_offset) {
          Some(layout) => {
            src_offset += layout.width();
            layout
          }
          _ => panic!("internal error"),
        });
      }
      src = match dst {
        Uninit => src.shift_bytes(1),
        Init(init) => match src.intersects(&init.downcast()) {
          Ok(src) => src,
          Err(e) => return Err(e),
        },
      };
    }
    let t = src.finish();
    if !t.reachable {
      return Err("none of the bit patterns in source are valid in target");
    }
    // Static analyze result.
    let maybe_invalid_bit_pattern = !t.infallible;
    let sound_target_mutation = t.mutable;
    let truncate = sh > dh && dt == 0;
    Ok(StaticAnalyze::new(
      maybe_size_mismatch,
      maybe_invalid_bit_pattern,
      sound_target_mutation,
      truncate,
    ))
  }

  /// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
  ///
  /// # Internals
  /// DSTs metadata represents the length of it's trailing slice, such as `.1` in `(u16, [u16])`.
  /// These checks [validate](MetadataChecks::validate) such metadata to ensure safe and correct
  /// transmutations.
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
  }

  #[derive(Copy, Clone, PartialEq, Hash)]
  struct Branches(u64);

  impl Branches {
    #[inline]
    pub const fn empty() -> Self {
      Self(0)
    }

    #[inline]
    pub const fn new(branches: usize) -> Self {
      if branches == 0 {
        return Self(0);
      }
      Self(u64::MAX >> (64 - branches))
    }

    #[inline]
    pub const fn branches(self) -> usize {
      self.0.count_ones() as usize
    }

    #[inline]
    pub const fn next(self) -> (Option<usize>, Self) {
      if self.0 == 0 {
        return (None, self);
      }
      let index = self.0.trailing_zeros() as usize;
      (Some(index), self.invalidate(index))
    }

    #[inline]
    pub const fn invalidate(self, index: usize) -> Self {
      Self(self.0 & !(1 << index))
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

  #[derive(Copy, Clone)]
  pub(crate) struct Source<'a> {
    len: usize,
    pub(crate) width: usize,
    offset: usize,
    branches: Branches,
    chunks: [InitLayout<'a>; 16],
    transmutability: Transmutability,
  }

  impl<'a> Source<'a> {
    #[inline]
    pub const fn empty() -> Self {
      Self {
        len: 0,
        width: 0,
        offset: 0,
        branches: Branches::empty(),
        chunks: [InitLayout::VOID; 16],
        transmutability: Transmutability::reachable_infallible_mutable(),
      }
    }

    /// Append a layout to the aggregate.
    #[inline]
    pub const fn append(mut self, layout: ByteLayout<'a>) -> Self {
      assert!(self.len < self.chunks.len());
      let (layout, width) = match layout {
        Uninit => (InitLayout::VOID, 1),
        Init(layout) if self.len != 0 => (layout.downcast(), layout.width()),
        Init(layout) => {
          assert!(self.offset == 0 && self.width == 0);
          self.offset = layout.offset;
          self.branches = Branches::new(layout.branches());
          (layout.downcast(), layout.width())
        }
      };
      self.chunks[self.len] = layout;
      self.width += width;
      self.len += 1;
      self
    }

    /// Shift right the aggregate by `n` bytes.
    #[inline]
    pub const fn shift_bytes(self, n: usize) -> Self {
      let mut shifted = Self::empty();
      shifted.transmutability = self.transmutability;
      while shifted.width + n < self.width {
        shifted = shifted.append(self.offset(shifted.width + n));
      }
      shifted
    }

    /// Shift out the head layout.
    #[inline]
    pub const fn shift(mut self) -> Self {
      if self.width == 0 {
        return self;
      }
      self.width -= self.chunks[0].width() - self.offset;
      let mut i = 1;
      while i < self.len {
        self.chunks[i - 1] = self.chunks[i];
        i += 1;
      }
      self.len -= 1;
      self.offset = 0;
      self.chunks[i - 1] = InitLayout::VOID;
      self.branches = Branches::new(self.chunks[0].branches());
      self
    }

    #[inline]
    pub const fn offset(self, mut offset: usize) -> ByteLayout<'a> {
      assert!(offset < self.width);
      let mut i = 0;
      while i < self.len {
        let mut width = self.chunks[i].width();
        if i == 0 {
          width -= self.offset;
        }
        if offset < width {
          return Init(self.chunks[i].upcast(offset));
        }
        i += 1;
        offset -= width;
      }
      unreachable!()
    }

    #[inline]
    pub const fn finish(self) -> Transmutability {
      if !self.transmutability.reachable {
        return Transmutability::unreachable();
      }
      self.transmutability
    }

    #[inline]
    pub const fn intersects(mut self, dst: &InitLayout) -> Result<Self, &'static str> {
      assert!(dst.width() <= self.width);
      let mut offset = 0; // Offset in bytes withing `layout`.
      let mut dst_branches = Branches::new(dst.branches());
      // Keep track of the full transmutability against target.
      let mut full_t = Transmutability::reachable_infallible_mutable();
      // Iterate over all source layouts until target is fully covered.
      while offset != dst.width() {
        if self.chunks[0].is_uninit() {
          return Err("uninit bytes");
        }
        let src = self.chunks[0].upcast(self.offset);
        let dst = dst.upcast(offset);
        // Keep track of the target covered branches for this source.
        let mut dst_coverage = 0u64;
        // Keep track of this source chunk transmutability against target.
        let mut chunk_t = Transmutability::unreachable();
        // Try each source branches, invalidate unreachable ones.
        let mut src_branches = self.branches.next();
        while let Some(src_branch) = src_branches.0 {
          // Keep track of this branch transmutability against target.
          let mut branch_t = Transmutability::unreachable();
          // Try each target branches.
          let mut dst_branch_iter = dst_branches.next();
          while let Some(dst_branch) = dst_branch_iter.0 {
            let t = src.intersects(dst, (src_branch, dst_branch));
            // Or this branches combination into this source branch transmutability.
            branch_t = branch_t.or(t);
            // If this branch combination is reachable, mark this target branch as covered.
            if t.reachable {
              dst_coverage |= 1 << dst_branch;
            }
            // Move to next target branch.
            dst_branch_iter = dst_branch_iter.1.next();
          }
          // Or this source branch into chunk transmutability.
          chunk_t = chunk_t.or(branch_t);
          // If this source branch is unreachable, mark it as invalid.
          if !branch_t.reachable {
            self.branches = self.branches.invalidate(src_branch);
          }
          // Move to next source branch.
          src_branches = src_branches.1.next();
        }
        // And chunks into full transmutability.
        full_t = full_t.and(chunk_t);
        // Only continue with target covered branches.
        dst_branches = Branches(dst_coverage);
        // Are all chunk branches covered ?
        full_t.infallible &= self.branches.branches() == src.branches();
        // Move to next chunk if necessary.
        if src.width() > dst.width() {
          // Source chunk is bigger than target, offset by the consumed bytes.
          self.width -= dst.width();
          self.offset += dst.width();
          break;
        }
        // Source is smaller or equals than target, consume whole chunk.
        self = self.shift();
        offset += src.width();
      }
      // Are all target branches covered ?
      full_t.mutable &= dst_branches.branches() == dst.branches();
      // And this transmutability to global state.
      self.transmutability = self.transmutability.and(full_t);
      Ok(self)
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
      unwrap!(Self::STATIC_ANALYZE);
    };
    pub const CHECK_TRUNCATE: () = {
      if !unwrap!(Self::STATIC_ANALYZE).truncate {
        panic!("\n  does not need to be truncated.\n  use `transmute` instead.\n")
      }
    };
    pub const CHECK_NO_TRUNCATE: () = {
      if unwrap!(Self::STATIC_ANALYZE).truncate {
        panic!("\n  target is too small.\n  use `truncate()` instead.\n")
      }
    };
    pub const CHECK_NO_SIZE_MISMATCH: () = {
      if let Some(checks) = unwrap!(Self::STATIC_ANALYZE).maybe_size_mismatch {
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
      if unwrap!(Self::STATIC_ANALYZE).maybe_invalid_bit_pattern {
        panic!(
          "\n  target are not valid for all bit patterns.\n  and not all source bit patterns are \
           valid for target, which requires a runtime check.\n  use `try_transmute()` instead.\n",
        )
      }
    };
    pub const CHECK_SOUND_TARGET_MUTATION: () = {
      if !unwrap!(Self::STATIC_ANALYZE).sound_target_mutation {
        panic!(
          "\n  mutating target could result in invalid bit patterns in source, hence mutation is \
           disabled.\n",
        )
      }
    };
  }

  // impl BitsPattern ------------------------------------------------------------------------------

  impl<'a> BitsPattern<'a, u8> {
    /// Returns whatever this pattern contains at least one of the possible values of `other`.
    const fn contains_any(self, other: Self) -> bool {
      use BitsPattern::*;
      match (self, other) {
        #[cfg(test)]
        (Uninit, _) | (_, Uninit) => panic!("uninitialized layout"),
        (Any, _) => true,                      // Any contains any pattern.
        (_, Any) => true,                      // Not any contains at least one in any.
        (Allow(a), Allow(b)) => a[0] == b[0],  // `a` contains `b` only if `a == b`.
        (Allow(a), Disallow(b)) => a[0] != *b, // `a` contains any but `b` only if `a != b`.
        (Disallow(a), Allow(b)) => *a != b[0], // Any but `a` contains `b` only if `a != b`.
        (Disallow(_), Disallow(_)) => true,    // Any but `a` contains any but `b`.
      }
    }

    /// Returns whatever this pattern contains all possible values of `other`.
    const fn contains_all(self, other: Self) -> bool {
      use BitsPattern::*;
      match (self, other) {
        #[cfg(test)]
        (Uninit, _) | (_, Uninit) => panic!("uninitialized layout"),
        (Any, _) => true,                       // Any contains any pattern.
        (_, Any) => false,                      // Not any cannot contain any.
        (Allow(a), Allow(b)) => a[0] == b[0],   // `a` contains `b` only if `a == b`.
        (Allow(_), Disallow(_)) => false,       // `a` cannot contain any but `b`.
        (Disallow(a), Allow(b)) => *a != b[0],  // Any but `a` contains `b` only if `a != b`.
        (Disallow(a), Disallow(b)) => *a == *b, // Any but `a` contains any but `b` only if `a == b`.
      }
    }
  }

  // impl IntegerSpec ------------------------------------------------------------------------------

  impl<'a, T: Integer<Bits = T>> IntegerSpec<'a, T> {
    #[inline]
    #[cfg(test)]
    pub const fn is_uninit(&self) -> bool {
      matches!(self.bits_pattern, BitsPattern::Uninit)
    }

    #[inline]
    pub const fn is_any(&self) -> bool {
      matches!(self.bits_pattern, BitsPattern::Any)
    }

    #[inline]
    pub const fn init_at<U>(self, offset: U) -> InitLayout<'a, U> {
      let (bits_pattern, bits_pattern_values) = match self.bits_pattern {
        BitsPattern::Allow(values) => (bits::ALLOW, transmute_ref(values)),
        BitsPattern::Disallow(value) => (bits::DISALLOW, transmute_ref(value)),
        #[cfg(test)]
        BitsPattern::Uninit => (bits::UNINIT, [].as_slice()),
        BitsPattern::Any => (bits::ANY, [].as_slice()),
      };

      InitLayout { offset, width: mem::size_of::<T>() as u8, bits_pattern, bits_pattern_values }
    }

    #[inline]
    pub const fn matches(&self, value: &T) -> bool {
      // SAFETY: `value` is a valid reference to `T`.
      unsafe { self.matches_ptr(value) }
    }

    /// # Safety
    /// `value` must be a valid reference to `T`, but not necessarily aligned on `T`.
    #[inline]
    pub const unsafe fn matches_ptr(&self, value: *const T) -> bool {
      match self.bits_pattern {
        #[cfg(test)]
        BitsPattern::Uninit => false,
        BitsPattern::Allow(values) => {
          let mut i = 0;
          while i < values.len() {
            if equals_raw(&values[i], value) {
              return true;
            }
            i += 1;
          }
          false
        }
        BitsPattern::Disallow(disallow) => !equals_raw(disallow, value),
        _ => true,
      }
    }
  }

  // impl InitLayout -------------------------------------------------------------------------------

  impl<'a, Offset> InitLayout<'a, Offset> {
    #[inline]
    pub const fn is_uninit(&self) -> bool {
      self.bits_pattern == bits::UNINIT
    }

    #[inline]
    pub const fn is_any(&self) -> bool {
      self.bits_pattern == bits::ANY
    }

    #[inline]
    const fn branches(&self) -> usize {
      if self.width == 0 {
        return 0;
      }
      match self.bits_pattern {
        bits::ALLOW => self.bits_pattern_values.len() / self.width as usize,
        _ => 1,
      }
    }
  }

  impl<'a> InitLayout<'a> {
    pub(crate) const VOID: Self = Self::new(0, bits::UNINIT, &[]);

    #[inline]
    pub const fn new(width: u8, bits_pattern: u8, bits_pattern_values: &'a [u8]) -> Self {
      Self { offset: (), width, bits_pattern, bits_pattern_values }
    }

    #[inline]
    pub const fn width(&self) -> usize {
      self.width as usize
    }

    #[inline]
    pub const fn upcast(self, offset: usize) -> InitLayout<'a, usize> {
      let InitLayout { width, bits_pattern, bits_pattern_values, .. } = self;
      InitLayout { offset, width, bits_pattern, bits_pattern_values }
    }
  }

  impl<'a> InitLayout<'a, usize> {
    #[inline]
    pub const fn width(&self) -> usize {
      self.width as usize - self.offset
    }

    #[inline]
    pub const fn downcast(self) -> InitLayout<'a> {
      let InitLayout { width, bits_pattern, bits_pattern_values, .. } = self;
      InitLayout { offset: (), width, bits_pattern, bits_pattern_values }
    }

    #[inline]
    pub(crate) const fn branch(&self, branch: usize) -> BitsPattern<'a, u8> {
      use BitsPattern::*;
      assert!(branch < self.branches());
      match self.bits_pattern {
        bits::DISALLOW => Disallow(&self.bits_pattern_values[self.offset]),
        bits::ALLOW => Allow(transmute_ref(
          &self.bits_pattern_values[(branch * self.width as usize) + self.offset],
        )),
        _ => Any,
      }
    }

    #[inline]
    const fn intersects(
      mut self, mut dst: InitLayout<'_, usize>, branches: (usize, usize),
    ) -> Transmutability {
      let mut result = Transmutability::reachable_infallible_mutable();
      // Iterate over all bytes of this branch.
      while dst.width() > 0 && self.width() > 0 {
        // Compute both source and target branch patterns.
        let pattern = (self.branch(branches.0), dst.branch(branches.1));
        // Aggregate results.
        result.reachable &= pattern.0.contains_any(pattern.1);
        result.mutable &= pattern.0.contains_all(pattern.1);
        result.infallible &= pattern.1.contains_all(pattern.0);
        // Move to next offset.
        self.offset += 1;
        dst.offset += 1;
      }
      result
    }
  }

  // impl ByteLayout -------------------------------------------------------------------------------

  impl<'a> ByteLayout<'a> {
    #[inline]
    pub const fn width(&self) -> usize {
      match self {
        Uninit => 1,
        Init(layout) => layout.width(),
      }
    }
  }

  // impl Unalign ----------------------------------------------------------------------------------

  impl<T> Unalign<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
      Self(ManuallyDrop::new(value))
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

  impl<T: Data> Deref for Unalign<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
      unaligned_ref!(self.0)
    }
  }

  impl<T: Data> DerefMut for Unalign<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
      unaligned_mut!(self.0)
    }
  }

  impl<T: Display + Copy> Display for Unalign<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Display::fmt(&self.into_inner(), f)
    }
  }

  impl<T: Debug + Copy> Debug for Unalign<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(&self.into_inner(), f)
    }
  }

  // impl Repr -------------------------------------------------------------------------------------

  unsafe impl<T: ?Sized + Data> Repr<T> for () {
    type Type = ();
    type Rest = ();
    const OFFSET: usize = 0;
    const FIELDS: usize = 0;
  }

  // impl Data -------------------------------------------------------------------------------------

  unsafe impl<T: Integer> Data for T {
    type Item = ();
    type TailItem = ();
    type Sized = Self;
    type Repr = ();
    type Bits = T::Bits;
    const LAYOUT: Layout<Self> = Layout::Integer { spec: T::SPEC };
  }

  unsafe impl<T: Copy + Data> Data for [T] {
    type Item = T;
    type TailItem = T;
    type Sized = ();
    type Repr = ();
    type Bits = ();
    const LAYOUT: Layout<Self> = Layout::Slice;
  }

  unsafe impl<T: Copy + Data, const N: usize> Data for [T; N] {
    type Item = T;
    type TailItem = ();
    type Sized = Self;
    type Repr = ();
    type Bits = ();
    const LAYOUT: Layout<Self> = Layout::Array;
  }

  pub struct ManuallyDropRepr;

  unsafe impl<T: ?Sized + Data> Repr<ManuallyDrop<T>> for ManuallyDropRepr {
    type Type = T;
    type Rest = ();
    const OFFSET: usize = 0;
    const FIELDS: usize = 1;
  }

  unsafe impl<T: ?Sized + Data> Data for ManuallyDrop<T> {
    type Item = ();
    type TailItem = T::TailItem;
    type Sized = T::Sized;
    type Repr = ManuallyDropRepr;
    type Bits = ();
    const LAYOUT: Layout<Self> = {
      assert!(!mem::needs_drop::<T>());
      Layout::Struct
    };
  }

  pub struct Tuple0;
  pub struct Tuple1;

  unsafe impl<A: Copy + crate::Data, B: Copy + crate::Data> Repr<(A, B)> for Tuple0 {
    type Type = A;
    type Rest = Tuple1;
    const OFFSET: usize = offset_of!((A, B), 0);
    const FIELDS: usize = 2;
  }

  unsafe impl<A: Copy + crate::Data, B: Copy + crate::Data> Repr<(A, B)> for Tuple1 {
    type Type = B;
    type Rest = ();
    const OFFSET: usize = offset_of!((A, B), 1);
    const FIELDS: usize = 1;
  }

  unsafe impl<A: Copy + Data, B: Copy + Data> Data for (A, B) {
    type Item = ();
    type TailItem = B::TailItem;
    type Sized = Self;
    type Repr = Tuple0;
    type Bits = ();
    const LAYOUT: Layout<Self> = Layout::Struct;
  }

  // impl Integer ----------------------------------------------------------------------------------

  unsafe impl<T: ?Sized + 'static> Integer for PhantomData<T> {
    type Bits = Self;
    const SPEC: IntegerSpec<'static, Self::Bits> =
      IntegerSpec { default: &PhantomData, bits_pattern: BitsPattern::Any };
  }

  macro_rules! impl_word {
    ($($ty:ty = $raw:ty { $default:expr, $($bits_pattern:tt)+ })+) => {
      $(
        unsafe impl Integer for $ty {
          type Bits = $raw;
          const SPEC: IntegerSpec<'static, Self::Bits> = IntegerSpec {
            default: &$default,
            bits_pattern: BitsPattern::$($bits_pattern)+,
          };
        }
      )+
    };
  }

  impl_word!(
    () = () { (), Any }
    bool = u8 { 0, Allow(&[0, 1]) }
    u8 = u8 { 0, Any } u16 = u16 { 0, Any } u32 = u32 { 0, Any } u64 = u64 { 0, Any }
    i8 = i8 { 0, Any } i16 = i16 { 0, Any } i32 = i32 { 0, Any } i64 = i64 { 0, Any }
    u128 = u128 { 0, Any } i128 = i128 { 0, Any }
    Option<NonZeroU8> = u8 { 0, Any } Option<NonZeroU16> = u16 { 0, Any }
    Option<NonZeroU32> = u32 { 0, Any } Option<NonZeroU64> = u64 { 0, Any }
    Option<NonZeroU128> = u128 { 0, Any }
    NonZeroU8 = u8 { 1, Disallow(&0) }
    NonZeroU16 = u16 { 1, Disallow(&0) }
    NonZeroU32 = u32 { 1, Disallow(&0) }
    NonZeroU64 = u64 { 1, Disallow(&0) }
    NonZeroU128 = u128 { 1, Disallow(&0) }
  );
}
