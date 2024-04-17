//! ***Efficient, safe, transmutation. Even for dynamically-sized types.***
//!
//! `mutant` is a Rust crate providing safe and efficient transmutation between types,
//! including support for dynamically-sized types (`?Sized`). It empowers developers to
//! perform zero-cost, type-safe memory reinterpretations, extending Rust's capabilities
//! in systems programming, embedded systems, and other scenarios requiring fine-grained
//! control over data representation.
//!
//! # Overview
//!
//! `mutant` primarily offers the [`Data`] trait, which, when derived, allows for safe and
//! efficient transmutation between compatible types. The trait can be automatically
//! implemented on structs using the [`Data`] derive macro, ensuring correctness and
//! safety at compile-time. This crate stands out by supporting dynamically-sized types,
//! a feature not commonly found in similar crates like `zerocopy` and `bytemuck`.
//!
//! Key features of `mutant` include:
//! - Safe transmutation between types, including `?Sized` types.
//! - Compile-time checks to ensure transmutation safety.
//! - Utilities for handling unaligned data, critical in systems programming.
//!
//! # Use Cases
//!
//! `mutant` is particularly useful in scenarios where data representation must be changed without
//! altering the underlying bytes, such as:
//! - Systems programming where memory layout is critical.
//! - Embedded programming involving direct manipulation of hardware registers.
//! - Network programming for parsing and constructing packets with specific memory layouts.
//! - Interfacing with C libraries requiring specific data representations.
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
//! assert_eq!(&Position { x: 21, y: 12 }, mutant::transmute_ref!(&loot.position));
//! assert_eq!(Position { x: 21, y: 12 }, mutant::transmute!(loot.position)); // Position is `Copy`
//!
//! let mut position: &mut Position = mutant::transmute_mut!(&mut loot.position);
//! position.x *= 2;
//!
//! let loot: &Loot<[Item]> = &loot;
//! let loot_bytes: &[u8] = mutant::transmute_ref!(loot);
//! assert_eq!(
//!   &[21 * 2, 12, 0xfe, 0xca, 0x42, 1, 0x21, 0],
//!   loot_bytes
//! );
//!
//! // let loot2: &Loot<[Item]> = mutant::transmute_ref!(loot_bytes); Compile fail !
//! let loot2: Result<&Loot<[Item]>, _> = mutant::try_transmute_ref!(loot_bytes);
//! assert_eq!(Ok(loot), loot2);
//! ```
//!
//! Transmute is checked at compile time:
//! ```rust,compile_fail
//! use mutant::transmute_mut;
//! let _: &mut u8 = transmute_mut!(&mut true);
//! ```
//!
//! # Why mutant?
//!
//! `mutant` addresses a key gap in Rust's type system and memory model, specifically the ability to
//! safely reinterpret complex and dynamically-sized types. This capability is vital in low-level
//! programming, where it's often necessary to treat the same byte data as different types without
//! copying or modifying it. While Rust's safety guarantees make it an excellent choice for systems
//! programming, its strict type system can sometimes limit the flexibility needed in these domains.
//! `mutant` offers a solution, enabling these manipulations while upholding Rust's safety
//! principles.
//!
//! The crate is designed to be simple yet powerful, providing just the necessary tools
//! without overcomplicating the user's codebase. It fits seamlessly into Rust's ecosystem,
//! following Rust's conventions and idioms closely. This makes it an intuitive choice for Rust
//! developers looking to leverage type-safe transmutation in their applications.
//!
//! # Cargo Features
//!
//! - **`alloc`** (not yet implemented)
//!   Future plans include an `alloc` feature to integrate with Rust's allocation API, providing
//!   more functionality related to dynamically-sized types.
//!
//! # Getting Started
//!
//! To use `mutant` in your project, add it as a dependency in your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! mutant = "0.1"
//! ```
//!
//! Then, derive the `Data` trait on your structs to enable safe transmutation:
//! ```rust
//! use mutant::Data;
//!
//! #[derive(Data)]
//! #[repr(C)]
//! struct MyStruct {
//!   // ...
//! }
//! ```
//!
//! Use `mutant`'s functions like [`transmute_ref`] or [`try_transmute_ref`] to reinterpret your
//! data safely.
//!
//! # Safety and Soundness
//!
//! Safety is a top priority for `mutant`. The crate undergoes rigorous testing and review to
//! adhere to Rust's safety guarantees. Any use of `unsafe` code is carefully evaluated and
//! documented, ensuring soundness and preventing undefined behavior.
//!
//! # Documentation and Testing
//!
//! `mutant` is committed to providing thorough documentation and extensive tests to ensure
//! reliability and ease of use. The documentation follows Rust's standard library conventions,
//! offering clear explanations and examples. Meanwhile, tests cover a wide range of use cases
//! to guarantee the correctness and safety of the crate's functionality.
//!
//! # Contribution
//!
//! Contributions to `mutant` are welcome! Whether it's improving documentation, adding more tests,
//! or implementing new features, your input is valuable. Check out the repository on GitHub for
//! contributing guidelines and to submit your pull requests or issues.
//!
//! # License
//!
//! `mutant` is open-source software licensed under the MIT license. Feel free to use, modify,
//! and distribute it in accordance with the license terms.
#![cfg_attr(not(test), no_std)]
#[cfg(test)]
mod tests;

use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::Range;

pub use mutant_derive::Data;

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
  /// Source bit pattern is invalid.
  InvalidBitPattern,
}

/// Represents the layout of a [`Word`] type in memory.
#[derive(Debug, PartialEq, Hash)]
pub struct WordLayout {
  width: u8,
  flags: u8,
  bytes: [u8],
}

/// Describes a byte layout at a specific offset within a larger data structure.
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct InitLayout<'a> {
  pub offset: usize,
  pub layout: &'a WordLayout,
}

/// Represents either an initialized or uninitialized byte layout.
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum ByteLayout<'a> {
  Uninit,
  Init(InitLayout<'a>),
}

/// Represents the layout of a type in memory.
///
/// The [`Layout`] enum is used to categorize the memory representation of a type, which is
/// essential for safe and correct transmutation. It describes how data is laid out in memory,
/// providing the necessary context for transmutation operations.
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum Layout { Slice, Array, Struct, Word { layout: &'static WordLayout } }

/// A wrapper to ensure the inner type does not require dropping.
///
/// `Dropless` is used to encapsulate types within packed structs, verifying that
/// they do not implement the `Drop` trait. This check is essential to prevent
/// undefined behavior that can arise if Rust automatically tries to drop a field
/// in a packed struct, where standard drop semantics might lead to memory safety issues.
///
/// # Examples
///
/// ```
/// use mutant::Dropless;
///
/// #[repr(packed)]
/// struct FooPacked<T: ?Sized>(u8, Dropless<T>);
///
/// // Example usage with a packed struct
/// let foo = FooPacked(1, Dropless::new(1234));
/// // The `Dropless` wrapper checks that `1234` (the inner data) does not need to be dropped.
/// ```
///
/// In this example, `Dropless` is used in `FooPacked` to wrap the value `1234`.
/// It ensures that this value, as the inner data, does not require drop semantics.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
#[repr(transparent)]
pub struct Dropless<T: ?Sized>(ManuallyDrop<T>);

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
pub struct Unalign<T>(pub T);

/// Static-analyze a transmutation at compile-time.
///
/// # Examples
/// ```
/// use mutant::Transmutation;
///
/// let bool_to_u8 = Transmutation::<bool, u8>::new();
/// assert!(bool_to_u8.check().is_ok());
/// assert!(bool_to_u8.check_no_size_mismatch().is_ok());
/// assert!(bool_to_u8.check_no_invalid_bit_pattern().is_ok());
/// assert!(bool_to_u8.check_no_alignment_mismatch().is_ok());
/// assert!(bool_to_u8.check_sound_target_mutation().is_err());
///
/// let u8_to_bool = Transmutation::<u8, bool>::new();
/// assert!(u8_to_bool.check().is_ok());
/// assert!(u8_to_bool.check_no_size_mismatch().is_ok());
/// assert!(u8_to_bool.check_no_invalid_bit_pattern().is_err());
/// assert!(u8_to_bool.check_no_alignment_mismatch().is_ok());
/// assert!(u8_to_bool.check_sound_target_mutation().is_ok());
/// ```
pub struct Transmutation<Src: ?Sized, Dst: ?Sized> {
  source: PhantomData<Src>,
  target: PhantomData<Dst>,
}

pub unsafe trait Repr<T: ?Sized + Data> {
  type Type: ?Sized + Data;
  type Rest: Repr<T>;
  const OFFSET: usize;
  const FIELDS: usize;
}

/// Marker trait for plain word types (e.g. integers or enum discriminants).
///
/// # Safety
/// - Word types are fully initialized types, i.e. contains no uninitialized bytes.
/// - Implementor is responsible for `WORD_LAYOUT` to represents only valid values for `Self`.
/// - The size of `Self::Raw` must be equals to the size of `Self`.
/// - The `Self::Raw` type must accept any bit pattern.
pub unsafe trait Word: Copy + 'static {
  type Raw: Word<Raw = Self::Raw>;
  /// Valid bits pattern for this type.
  const WORD_LAYOUT: &'static WordLayout;
}

pub unsafe trait Data: 'static {
  /// Type of slice or array item.
  type Item: Copy + Data;
  /// Type of the trailing slice item.
  type Tail: Copy + Data;
  /// Fields of `Self`. Non-empty only for `struct` types.
  type Repr: Repr<Self>;
  /// Layout of `Self`.
  const LAYOUT: Layout;

  /// Size in bytes of the sized part of `Self`.
  /// For [`Sized`](Sized) types, this is equals to [`mem::size_of`]`::<Self>()`.
  #[doc(hidden)]
  const HEADER: usize;
}

// Macros ------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! bail {
  ($e:expr $(,)?) => {
    match $e {
      Ok(ok) => ok,
      Err(err) => return Err(err),
    }
  };
}

#[macro_export]
macro_rules! layout {
  (any) => (
    $crate::WordLayout::from_ref::<0, 0>(&(::core::mem::size_of::<Self>() as u8, 0b1, []))
  );
  (!$discard:expr) => ($crate::layout!(__impl DISCARD $discard));
  ($($allow:tt)|+) => ($crate::layout!(__impl ALLOW $($allow),+));
  (__impl $flags:ident $($values:expr),+) => ($crate::WordLayout::from_ref(&(
    ::core::mem::size_of::<Self>() as u8,
    $crate::bits::$flags,
    [$($crate::raw::<Self>($values).to_ne_bytes()),+])
  ));
}

/// Expands to the offset in bytes of a field from the beginning of the given type.
///
/// `offset_of!` produces code which is valid in a `const` context.
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

#[macro_export]
macro_rules! transmutation {
  ($($checks:ident),* $(,)?) => {{
    struct Comptime<T: ?Sized, U: ?Sized>(::core::marker::PhantomData<(*const T, *const U)>);

    impl<Src: ?Sized + $crate::Data, Dst: ?Sized + $crate::Data> Comptime<Src, Dst> {
      #[allow(unused)]
      const TRANSMUTATION: $crate::Transmutation<Src, Dst> = {
        let transmutation = $crate::Transmutation::new();
        $crate::unwrap!(transmutation.check());
        $($crate::unwrap!(transmutation.$checks());)*
        transmutation
      };
    }

    Comptime::TRANSMUTATION
  }};
}

#[macro_export]
macro_rules! transmute {
  ($e:expr $(,)?) => (
    $crate::transmutation!(check_no_size_mismatch, check_no_invalid_bit_pattern).transmute($e)
  );
}

#[macro_export]
macro_rules! transmute_copy {
  ($e:expr $(,)?) => (
    $crate::transmutation!(check_no_size_mismatch, check_no_invalid_bit_pattern).transmute_copy($e)
  );
}

#[macro_export]
macro_rules! transmute_mut {
  ($e:expr $(,)?) => {
    $crate::transmutation!(
      check_no_alignment_mismatch, check_no_size_mismatch, check_no_invalid_bit_pattern, check_sound_target_mutation
    ).transmute_mut($e)
  };
}

#[macro_export]
macro_rules! transmute_ref {
  ($e:expr $(,)?) => {
    $crate::transmutation!(
      check_no_alignment_mismatch, check_no_size_mismatch, check_no_invalid_bit_pattern,
    ).transmute_ref($e)
  };
}

#[macro_export]
macro_rules! try_transmute {
  ($e:expr $(,)?) => {
    $crate::transmutation!(check_no_size_mismatch).try_transmute_copy(&$e)
  };
}

#[macro_export]
macro_rules! try_transmute_copy {
  ($e:expr $(,)?) => {
    $crate::transmutation!(check_no_size_mismatch).try_transmute_copy($e)
  };
}

#[macro_export]
macro_rules! try_transmute_mut {
  ($e:expr $(,)?) => {
    $crate::transmutation!(check_no_invalid_bit_pattern).try_transmute_mut($e)
  };
}

#[macro_export]
macro_rules! try_transmute_ref {
  ($e:expr $(,)?) => {
    $crate::transmutation!().try_transmute_ref($e)
  };
}

/// Safely reference unaligned data.
#[macro_export]
macro_rules! unaligned_ref {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_ref!($base.$field);
    $crate::unaligned_ref!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &_ = $base;
    let addr = core::ptr::addr_of!($base.$field);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
}

/// Safely mutably reference unaligned data.
#[macro_export]
macro_rules! unaligned_mut {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_mut!($base.$field);
    $crate::unaligned_mut!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &mut _ = $base;
    let addr = core::ptr::addr_of_mut!($base.$field);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
}

#[macro_export]
macro_rules! unwrap {
  ($e:expr $(,)?) => {
    match $e {
      Ok(ok) => ok,
      Err(err) => panic!("{}", err),
    }
  };
}

/// A pointer to an aligned allocation of size 2^20.
#[doc(hidden)]
pub const BIG_ALLOCATION: *const () = {
  #[repr(C, align(1048576))]
  struct Big([u8; 1 << 20]);

  const REF: &Big = &Big([0; 1 << 20]);
  let ptr: *const Big = REF;
  ptr.cast()
};

/// Assert that a [`Data`] type is unaligned (i.e. his alignment is equals to 1).
#[doc(hidden)]
#[rustfmt::skip]
#[inline]
pub const fn const_assert_unaligned<T: ?Sized + Data>(_: *const T) {
  struct Assert<T: ?Sized>(*const T);
  impl<T: ?Sized + Data> Assert<T> { const ASSERT_UNALIGNED: () = assert!(align_of::<T>() == 1); }
  #[allow(clippy::let_unit_value)]
  let _ = Assert::<T>::ASSERT_UNALIGNED;
}

// Functions ---------------------------------------------------------------------------------------

/// Same as [`align_of`](mem::align_of), returns the minimum alignment of a type in bytes.
/// With the difference that it also accept unsized types with relaxed `?Sized` bound.
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

#[inline]
pub fn as_mut_slice<T: ?Sized + Data>(val: &mut T) -> &mut [T::Item] {
  let ptr: *mut T = val;
  let ptr = ptr.cast::<T::Item>();
  let len = match T::LAYOUT {
    Layout::Slice | Layout::Array if mem::size_of::<T::Item>() == 0 => return &mut [],
    Layout::Slice => ptr::metadata(val),
    Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
    _ => return &mut [],
  };
  unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}

#[inline]
pub const fn as_slice<T: ?Sized + Data>(val: &T) -> &[T::Item] {
  let ptr: *const T = val;
  let ptr = ptr.cast::<T::Item>();
  let len = match T::LAYOUT {
    Layout::Slice | Layout::Array if mem::size_of::<T::Item>() == 0 => return &[],
    Layout::Slice => ptr::metadata(val),
    Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
    _ => return &[],
  };
  unsafe { core::slice::from_raw_parts(ptr, len) }
}

/// Returns the [`ByteLayout`] of a type at a given `offset`.
///
/// For out-of-bounds `offset`, [`ByteLayout::Uninit`] is always returned.
///
/// # Examples
/// ```
/// //TODO:
/// // use mutant::{ByteLayout, Data};
///
/// //assert_eq!(ByteLayout::from_mask(0xff), mutant::byte_layout_at::<u16>(1));
/// //assert_eq!(ByteLayout::Uninit, mutant::byte_layout_at::<u16>(2));
///
/// //#[derive(Copy, Clone, Data)]
/// //#[repr(C)]
/// //struct Foo {
/// //  a: bool,
/// //  b: u16,
/// //}
///
/// //assert_eq!(ByteLayout::from_mask(0b01), mutant::byte_layout_at::<Foo>(0));
/// //assert_eq!(ByteLayout::Uninit, mutant::byte_layout_at::<Foo>(1));
/// //assert_eq!(ByteLayout::from_mask(0xff), mutant::byte_layout_at::<Foo>(2));
/// //assert_eq!(ByteLayout::from_mask(0xff), mutant::byte_layout_at::<Foo>(3));
/// //assert_eq!(ByteLayout::Uninit, mutant::byte_layout_at::<Foo>(4));
///
/// //// And for a slice:
/// //assert_eq!(ByteLayout::from_mask(0b01), mutant::byte_layout_at::<[Foo]>(4));
/// ```
#[inline]
pub const fn byte_layout_at<T: ?Sized + Data>(offset: usize) -> Option<ByteLayout<'static>> {
  #[inline]
  const fn repr_contains<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> bool {
    offset >= R::OFFSET
      && (is_unsized::<R::Type>() || offset < R::OFFSET + min_size_of::<R::Type>())
  }

  #[inline]
  const fn repr_byte_layout_at<R: Repr<T>, T: ?Sized + Data>(
    offset: usize,
  ) -> Option<ByteLayout<'static>> {
    match R::FIELDS {
      // Reached the end of the struct, returns an uninitialized layout.
      0 => Some(ByteLayout::Uninit),
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
    Layout::Slice | Layout::Array => byte_layout_at::<T::Item>(offset % mem::size_of::<T::Item>()),
    Layout::Struct => repr_byte_layout_at::<T::Repr, T>(offset),
    Layout::Word { layout } if layout.is_uninit() => Some(ByteLayout::Uninit),
    Layout::Word { layout } => Some(ByteLayout::Init(InitLayout { offset, layout })),
  }
}

/// Returns whatever a value of type `T` is aligned to `align`.
#[inline]
pub const fn is_aligned_to<T: ?Sized + Data>(val: &T, align: usize) -> bool {
  assert!(align.is_power_of_two(), "alignment must be power of 2");
  // `value` is aligned for `U` if `T` alignment is already big enough.
  align <= align_of::<T>()
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(val) & (align - 1) == 0
}

#[inline]
pub const fn is_sized<T: ?Sized + Data>() -> bool {
  !is_unsized::<T>()
}

#[inline]
pub const fn is_unsized<T: ?Sized + Data>() -> bool {
  #[inline]
  const fn repr_is_unsized<R: Repr<T>, T: ?Sized + Data>() -> bool {
    match R::FIELDS {
      0 => false,
      1 => is_unsized::<R::Type>(),
      _ => repr_is_unsized::<R::Rest, T>(),
    }
  }

  match T::LAYOUT {
    Layout::Slice => true,
    Layout::Struct => repr_is_unsized::<T::Repr, T>(),
    _ => false,
  }
}

#[inline]
const fn is_bits_valid<Dst: ?Sized + Data>(ptr: *const Dst) -> bool {
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
    Layout::Word { layout } => {
      // SAFETY: `ptr` is valid for read since
      // - `ptr` came from a valid reference,
      // - Read type `Dst::Word::Raw` is any bit pattern.
      unsafe { layout.matches_ptr(ptr.cast()) }
    }
  }
}

#[inline]
pub const fn matches<T: Word>(raw: &T::Raw) -> bool {
  T::WORD_LAYOUT.matches(raw)
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  T::HEADER
}

#[doc(hidden)]
#[inline(always)]
pub const fn raw<T: Word>(raw: T::Raw) -> T::Raw {
  assert!(mem::size_of::<T>() == mem::size_of::<T::Raw>());
  assert!(<T::Raw as Word>::WORD_LAYOUT.is_any());
  raw
}

#[inline]
pub const fn size_of_tail<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::Tail>()
}

#[inline]
pub const fn size_of_tail_val<T: ?Sized + Data>(val: &T) -> usize {
  ptr::metadata(val) * size_of_tail::<T>()
}

#[inline]
pub const fn size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of::<T>() + size_of_tail_val(val)
}

#[inline]
const fn size_of_with<T: ?Sized + Data>(metadata: usize) -> usize {
  min_size_of::<T>() + metadata * size_of_tail::<T>()
}

#[inline]
pub const fn split_at<T: ?Sized + Data>(val: &T, mid: usize) -> (&T, &[T::Tail]) {
  match try_split_at(val, mid) {
    Some(result) => result,
    _ => panic!("`try_split_at` failed"),
  }
}

#[inline]
pub const fn transmute<T: Data + Copy, U: Data + Copy>(val: T) -> U {
  transmute!(val)
}

#[inline]
pub const fn transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(val: &T) -> &U {
  transmute_ref!(val)
}

#[inline]
pub const fn try_split_at<T: ?Sized + Data>(val: &T, mid: usize) -> Option<(&T, &[T::Tail])> {
  if mid < min_size_of::<T>() || mid > size_of_val(val) {
    return None;
  }
  if mid == size_of_val(val) {
    return Some((val, &[]));
  }
  let tail = mid - min_size_of::<T>();
  if tail % mem::size_of::<T::Tail>() != 0 {
    return None;
  }
  let ptr: *const u8 = (val as *const T).cast();
  let val_ptr = ptr::from_raw_parts::<T>(ptr.cast(), tail / mem::size_of::<T::Tail>());
  let rest_ptr = ptr::from_raw_parts::<[T::Tail]>(
    unsafe { ptr.add(mid) }.cast(),
    ptr::metadata(val) - ptr::metadata(val_ptr),
  );
  Some(unsafe { (&*val_ptr, &*rest_ptr) })
}

// bits --------------------------------------------------------------------------------------------

#[doc(hidden)]
pub mod bits {
  pub const UNINIT: u8 = 0;
  pub const ANY: u8 = 0b1;
  pub const ALLOW: u8 = 0b10;
  pub const DISCARD: u8 = 0b100;
}

// ptr ---------------------------------------------------------------------------------------------

pub mod ptr {
  use core::mem;

  /// Convert a reference to a raw pointer.
  ///
  /// This is equivalent to `r as *const T`, but is a bit safer since it will never silently change
  /// type or mutability, in particular if the code is refactored.
  #[inline(always)]
  #[must_use]
  pub const fn from_ref<T: ?Sized>(r: &T) -> *const T {
    r
  }

  /// Convert a mutable reference to a raw pointer.
  ///
  /// This is equivalent to `r as *mut T`, but is a bit safer since it will never silently change
  /// type or mutability, in particular if the code is refactored.
  #[inline(always)]
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
  pub(super) const fn addr_of<T: ?Sized>(ptr: *const T) -> usize {
    // Compute `ptr` raw pointer `ptr`.
    let ptr = ptr.cast::<()>();
    // SAFETY: This no-op trick ensure that `ptr` has a provenance:
    // This will result in a compile-error if `ptr` is a dangling pointer (has no provenance).
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

// num ---------------------------------------------------------------------------------------------

pub mod num {
  use super::*;
  use core::fmt::{Debug, Display, Formatter};

  macro_rules! impl_num {
    { $($ty:ident $raw:ident $bytes:literal $ordering:ident),+ } => {
      $(impl_num!($ty $raw $bytes { mem::size_of::<$raw>() - $bytes } $ordering);)+
    };
    ($ty:ident $raw:ident $n:literal $rest:tt $ordering:ident) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $ty(pub [u8; $n]);

      unsafe impl Word for $ty {
        type Raw = $ty;
        const WORD_LAYOUT: &'static WordLayout = layout!(any);
      }

      impl $ty {
        pub const MIN: Self = Self::from_raw($raw::MIN);
        pub const MAX: Self = Self::from_raw($raw::MAX);

        #[inline] pub const fn to_ne_bytes(self) -> [u8; $n] { swap!($ordering => ne, self).0 }
        #[inline] pub const fn to_le_bytes(self) -> [u8; $n] { swap!($ordering => le, self).0 }
        #[inline] pub const fn to_be_bytes(self) -> [u8; $n] { swap!($ordering => be, self).0 }

        #[inline] pub const fn from_ne_bytes(b: [u8; $n]) -> Self { swap!(ne => $ordering, Self(b)) }
        #[inline] pub const fn from_le_bytes(b: [u8; $n]) -> Self { swap!(le => $ordering, Self(b)) }
        #[inline] pub const fn from_be_bytes(b: [u8; $n]) -> Self { swap!(be => $ordering, Self(b)) }

        #[inline]
        pub const fn from_raw(raw: $raw) -> Self {
          let (mut off, mut bytes) = (0, [0; $n]);
          while off < $n {
            bytes[off] = (raw >> (le_or_be!($ordering, off, $n - off - 1) * 8)) as u8;
            off += 1;
          }
          Self(bytes)
        }

        #[inline]
        pub const fn into_raw(self) -> $raw {
          let (mut off, mut raw) = (0, 0);
          while off < $n {
            raw |= (self.0[off] as $raw) << (le_or_be!($ordering, off, $n - off - 1) * 8);
            off += 1;
          }
          raw
        }

        #[inline]
        pub const fn swap_bytes(mut self) -> Self {
          let mut i = 0;
          while i < $n / 2 {
            let tmp = self.0[i];
            self.0[i] = self.0[$n - i - 1];
            self.0[$n - i - 1] = tmp;
            i += 1;
          }
          self
        }
      }

      impl From<$raw> for $ty {
        #[inline]
        fn from(raw: $raw) -> Self {
          Self::from_raw(raw)
        }
      }

      impl From<$ty> for $raw {
        #[inline]
        fn from(value: $ty) -> Self {
          value.into_raw()
        }
      }

      impl Debug for $ty {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
          <$raw as Debug>::fmt(&self.into_raw(), f)
        }
      }

      impl Display for $ty {
        #[inline]
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
          <$raw as Display>::fmt(&self.into_raw(), f)
        }
      }
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
    u16le u16 2 le, u16be u16 2 be,
    u24le u32 3 le, u24be u32 3 be, u24 u32 3 ne,
    u32le u32 4 le, u32be u32 4 be,
    u40le u64 5 le, u40be u64 5 be, u40 u64 5 ne,
    u48le u64 6 le, u48be u64 6 be, u48 u64 6 ne,
    u56le u64 7 le, u56be u64 7 be, u56 u64 7 ne,
    u64le u64 8 le, u64be u64 8 be
  }
}

// internal ----------------------------------------------------------------------------------------

mod internal {
  use core::fmt;
  use core::fmt::{Debug, Display, Formatter};
  use core::hint;
  use core::num::{NonZeroU128, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};
  use core::ops::{Deref, DerefMut};

  use crate::*;

  use ByteLayout::*;
  use MetadataChecks::*;

  // impl WordLayout -------------------------------------------------------------------------------

  impl WordLayout {
    pub(crate) const VOID: &'static Self = Self::from_ref::<0, 0>(&(0, 0, []));

    #[inline(always)]
    pub const fn from_bytes(bytes: &[u8]) -> &Self {
      assert!(bytes.len() >= 2);
      assert!(bytes[0] == 0 || (bytes[0].is_power_of_two() && bytes[0] <= 16));
      let values_bytes = bytes.len() - 2;
      let values = match bytes[0] {
        0 => values_bytes,
        _ => {
          assert!(values_bytes % bytes[0] as usize == 0);
          values_bytes / bytes[0] as usize
        }
      };
      match bytes[1] {
        bits::UNINIT => assert!(values == 0),
        bits::ANY => assert!(values == 0),
        bits::ALLOW => assert!(values != 0 && values < 64),
        bits::DISCARD => assert!(values == 1),
        _ => panic!("invalid bits pattern type"),
      }
      // SAFETY: above checks ensure a valid layout.
      unsafe { &*ptr::from_raw_parts(bytes.as_ptr().cast(), values_bytes) }
    }

    #[inline(always)]
    pub const fn from_ref<const W: usize, const N: usize>(x: &(u8, u8, [[u8; W]; N])) -> &Self {
      Self::from_bytes(unsafe { &*ptr::from_raw_parts(ptr::from_ref(x).cast(), 2 + W * N) })
    }

    #[inline(always)]
    pub const fn is_uninit(&self) -> bool {
      self.flags == bits::UNINIT
    }

    #[inline(always)]
    pub const fn is_any(&self) -> bool {
      self.flags == bits::ANY
    }

    #[inline(always)]
    pub const fn width(&self) -> usize {
      self.width as usize
    }

    #[inline(always)]
    const fn branches(&self) -> usize {
      if self.width() == 0 {
        return 0;
      }
      match self.flags {
        bits::ALLOW => self.bytes.len() / self.width as usize,
        _ => 1,
      }
    }

    #[inline(always)]
    pub const fn matches<T: Word>(&self, word: &T) -> bool {
      if self.width as usize != mem::size_of::<T>() {
        return false;
      }
      // SAFETY: `word` is a valid reference and the above check
      // make sure `word` is valid up to `self.width`.
      unsafe { self.matches_ptr(ptr::from_ref(word).cast()) }
    }

    /// # Safety
    /// `ptr` is valid for read up to `self.width` bytes.
    #[inline(always)]
    pub const unsafe fn matches_ptr(&self, ptr: *const u8) -> bool {
      macro_rules! inner {
        ($ptr:ident, $ty:ident) => {{
          let val = ::core::ptr::read_unaligned::<$ty>($ptr.cast());
          let mut query = self.bytes.as_ptr().cast::<$ty>();
          match self.flags & (bits::ALLOW | bits::DISCARD) {
            bits::DISCARD => val != ::core::ptr::read_unaligned(query),
            bits::ALLOW => {
              let end_ptr = self.bytes.as_ptr().add(self.bytes.len());
              while ptr::addr_of(query) != ptr::addr_of(end_ptr) {
                let allow = ::core::ptr::read_unaligned(query);
                if val == allow {
                  return true;
                }
                query = query.add(1);
              }
              false
            }
            _ => true,
          }
        }};
      }
      match self.width {
        0 => true,
        1 => inner!(ptr, u8),
        2 => inner!(ptr, u16),
        4 => inner!(ptr, u32),
        8 => inner!(ptr, u64),
        16 => inner!(ptr, u128),
        _ => unsafe { hint::unreachable_unchecked() },
      }
    }
  }

  // impl InitLayout -------------------------------------------------------------------------------

  impl<'a> InitLayout<'a> {
    #[inline(always)]
    pub const fn width(&self) -> usize {
      self.layout.width() - self.offset
    }

    #[inline(always)]
    const fn branches(&self) -> usize {
      self.layout.branches()
    }

    #[inline(always)]
    const fn branch(&self, branch: usize) -> BytePattern {
      use BytePattern::*;
      assert!(branch < self.branches());
      match self.layout.flags {
        bits::DISCARD => Discard(self.layout.bytes[self.offset]),
        bits::ALLOW => Allow(self.layout.bytes[(branch * self.layout.width()) + self.offset]),
        _ => Any,
      }
    }

    #[inline(always)]
    const fn intersects(
      mut self,
      mut dst: InitLayout,
      branches: (usize, usize),
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
    #[inline(always)]
    pub const fn width(&self) -> usize {
      match self {
        Uninit => 1,
        Init(layout) => layout.width(),
      }
    }
  }

  // impl Transmutation ----------------------------------------------------------------------------

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
    #[inline(always)]
    pub const fn validate(self, metadata: usize) -> Result<(), isize> {
      match self {
        Is(n) if metadata == n => Ok(()),
        IsAtLeast(n) if metadata >= n => Ok(()),
        Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
        IsMultipleOf(m) if metadata % m == 0 => Ok(()),
        IsMultipleOf(m) => Err(m as isize - (metadata % m) as isize),
        IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => Ok(()),
        IsAtLeastThenMultipleOf(n, _) if metadata < n => Err(n as isize - metadata as isize),
        IsAtLeastThenMultipleOf(_, m) => Err(m as isize - (metadata % m) as isize),
        IsEmpty if metadata == 0 => Ok(()),
        IsEmpty => Err(-(metadata as isize)),
      }
    }
  }

  /// Enumerates the most generic bit patterns for individual bytes.
  ///
  /// # Internals
  /// This enum is used to describe potential byte patterns within a type `A`. During transmutation
  /// checks into another type `B`, as implemented in [`InitLayout::intersects`], this is utilized
  /// through the [`BytePattern::contains_any`] and [`BytePattern::contains_all`] methods to compute
  /// the [transmutability](Transmutability) from `A` to `B`, over each potential combination.
  #[derive(Copy, Clone, PartialEq, Hash)]
  enum BytePattern {
    Any,         // Is any possible byte pattern, indicating no specific constraints.
    Allow(u8),   // Exactly match the given `value`.
    Discard(u8), // Is any other byte pattern than `value`.
  }

  impl BytePattern {
    /// Returns whatever this pattern contains at least one of the possible values of `other`.
    pub const fn contains_any(self, other: Self) -> bool {
      use BytePattern::*;
      match (self, other) {
        (Any, _) => true,                   // Any contains any pattern.
        (_, Any) => true,                   // Not any contains at least one in any.
        (Allow(a), Allow(b)) => a == b,     // `a` contains `b` only if `a == b`.
        (Allow(a), Discard(b)) => a != b,   // `a` contains any but `b` only if `a != b`.
        (Discard(a), Allow(b)) => a != b,   // Any but `a` contains `b` only if `a != b`.
        (Discard(a), Discard(b)) => a == b, // Any but `a` contains any but `b` only if `a == b`.
      }
    }

    /// Returns whatever this pattern contains all possible values of `other`.
    pub const fn contains_all(self, other: Self) -> bool {
      use BytePattern::*;
      match (self, other) {
        (Any, _) => true,                   // Any contains any pattern.
        (_, Any) => false,                  // Not any cannot contain any.
        (Allow(b), Allow(a)) => a == b,     // `a` contains `b` only if `a == b`.
        (Allow(_), Discard(_)) => false,    // `a` cannot contain any but `b`.
        (Discard(b), Allow(a)) => a != b,   // Any but `a` contains `b` only if `a != b`.
        (Discard(b), Discard(a)) => a == b, // Any but `a` contains any but `b` only if `a == b`.
      }
    }
  }

  /// Static analyze result.
  #[derive(Copy, Clone, Eq, PartialEq)]
  pub(crate) struct StaticAnalyze {
    pub(crate) maybe_size_mismatch: Option<MetadataChecks>,
    pub(crate) maybe_invalid_bit_pattern: bool,
    pub(crate) sound_target_mutation: bool,
  }

  impl StaticAnalyze {
    #[allow(unused)]
    #[inline(always)]
    pub const fn fallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self { maybe_size_mismatch, maybe_invalid_bit_pattern: true, sound_target_mutation: false }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn fallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self { maybe_size_mismatch, maybe_invalid_bit_pattern: true, sound_target_mutation: true }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn infallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self { maybe_size_mismatch, maybe_invalid_bit_pattern: false, sound_target_mutation: false }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn infallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
      Self { maybe_size_mismatch, maybe_invalid_bit_pattern: false, sound_target_mutation: true }
    }
  }

  #[cfg(test)]
  impl Debug for StaticAnalyze {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      let tag = match (self.maybe_invalid_bit_pattern, self.sound_target_mutation) {
        (true, false) => "fallible",
        (true, true) => "fallible_mutable",
        (false, false) => "infallible",
        (false, true) => "infallible_mutable",
      };
      let mut d = f.debug_tuple(tag);
      d.field(&self.maybe_size_mismatch);
      d.finish()
    }
  }

  #[derive(Copy, Clone, PartialEq, Hash)]
  struct Branches(u64);

  impl Branches {
    #[inline(always)]
    pub const fn empty() -> Self {
      Self(0)
    }

    #[inline(always)]
    pub const fn new(branches: usize) -> Self {
      if branches == 0 {
        return Self(0);
      }
      Self(u64::MAX >> (64 - branches))
    }

    #[inline(always)]
    pub const fn branches(self) -> usize {
      self.0.count_ones() as usize
    }

    #[inline(always)]
    pub const fn next(self) -> (Option<usize>, Self) {
      if self.0 == 0 {
        return (None, self);
      }
      let index = self.0.trailing_zeros() as usize;
      (Some(index), self.invalidate(index))
    }

    #[inline(always)]
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
    #[inline(always)]
    pub const fn unreachable() -> Self {
      Self { reachable: false, infallible: true, mutable: true }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn reachable() -> Self {
      Self { reachable: true, infallible: false, mutable: false }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn reachable_infallible_mutable() -> Self {
      Self { reachable: true, infallible: true, mutable: true }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn reachable_infallible() -> Self {
      Self { reachable: true, infallible: true, mutable: false }
    }

    #[allow(unused)]
    #[inline(always)]
    pub const fn reachable_mutable() -> Self {
      Self { reachable: true, infallible: false, mutable: true }
    }

    #[inline(always)]
    pub const fn and(mut self, other: Self) -> Self {
      self.reachable &= other.reachable;
      self.mutable &= other.mutable;
      self.infallible &= other.infallible;
      self
    }

    #[inline(always)]
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
    chunks: [&'a WordLayout; 16],
    transmutability: Transmutability,
  }

  impl<'a> Source<'a> {
    #[inline(always)]
    pub const fn empty() -> Self {
      Self {
        len: 0,
        width: 0,
        offset: 0,
        branches: Branches::empty(),
        chunks: [WordLayout::VOID; 16],
        transmutability: Transmutability::reachable_infallible_mutable(),
      }
    }

    /// Append a layout to the aggregate.
    #[inline(always)]
    pub const fn append(mut self, layout: ByteLayout<'a>) -> Self {
      assert!(self.len < self.chunks.len());
      let (layout, width) = match layout {
        Uninit => (WordLayout::VOID, 1),
        Init(layout) if self.len != 0 => (layout.layout, layout.width()),
        Init(layout) => {
          assert!(self.offset == 0 && self.width == 0);
          self.offset = layout.offset;
          self.branches = Branches::new(layout.branches());
          (layout.layout, layout.width())
        }
      };
      self.chunks[self.len] = layout;
      self.width += width;
      self.len += 1;
      self
    }

    /// Shift right the aggregate by `n` bytes.
    #[inline(always)]
    pub const fn shift_bytes(self, n: usize) -> Self {
      let mut shifted = Self::empty();
      shifted.transmutability = self.transmutability;
      while shifted.width + n < self.width {
        shifted = shifted.append(self.offset(shifted.width + n));
      }
      shifted
    }

    /// Shift out the head layout.
    #[inline(always)]
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
      self.chunks[i - 1] = WordLayout::VOID;
      self.branches = Branches::new(self.chunks[0].branches());
      self
    }

    #[inline(always)]
    pub const fn offset(self, mut offset: usize) -> ByteLayout<'a> {
      assert!(offset < self.width);
      let mut i = 0;
      while i < self.len {
        let mut width = self.chunks[i].width();
        if i == 0 {
          width -= self.offset;
        }
        if offset < width {
          return Init(InitLayout { offset, layout: self.chunks[i] });
        }
        i += 1;
        offset -= width;
      }
      unreachable!()
    }

    #[inline(always)]
    pub const fn finish(self) -> Transmutability {
      if !self.transmutability.reachable {
        return Transmutability::unreachable();
      }
      self.transmutability
    }

    #[inline(always)]
    pub const fn intersects(mut self, dst: &WordLayout) -> Result<Self, &'static str> {
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
        let src = InitLayout { offset: self.offset, layout: self.chunks[0] };
        let dst = InitLayout { offset, layout: dst };
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

  impl<Src: ?Sized + Data, Dst: ?Sized + Data> Transmutation<Src, Dst> {
    const STATIC_ANALYZE: Result<StaticAnalyze, &'static str> = static_analyze::<Src, Dst>();

    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
      Self { source: PhantomData, target: PhantomData }
    }

    #[inline(always)]
    pub const fn check(self) -> Result<(), &'static str> {
      bail!(Self::STATIC_ANALYZE);
      Ok(())
    }

    #[inline(always)]
    pub const fn check_no_size_mismatch(self) -> Result<(), &'static str> {
      if let Some(checks) = bail!(Self::STATIC_ANALYZE).maybe_size_mismatch {
        use MetadataChecks::*;
        return Err(match checks {
          Is(_) => {
            "\n  cannot transmute between types of incompatible byte layouts, source too small.\n  \
             consider using a `try_*` alternative instead.\n"
          }
          IsAtLeast(_) => {
            "\n  cannot transmute between types of incompatible byte layouts, target too small.\n  \
             consider using a `try_*` alternative instead.\n"
          }
          IsMultipleOf(_) => {
            "\n  cannot transmute between types of incompatible byte layouts, tail size multiple \
             of mismatch.\n  consider using a `try_*` alternative instead.\n"
          }
          IsAtLeastThenMultipleOf(_, _) => {
            "\n  cannot transmute between types of incompatible byte layouts, target too small or \
             tail size multiple of mismatch.\n  consider using a `try_*` alternative instead.\n"
          }
          IsEmpty => {
            "\n  cannot transmute between types of incompatible byte layouts, source tail might \
             not be empty.\n  consider using a `try_*` alternative instead.\n"
          }
        });
      }
      Ok(())
    }

    #[inline(always)]
    pub const fn check_no_alignment_mismatch(self) -> Result<(), &'static str> {
      if align_of::<Dst>() > align_of::<Src>() {
        return Err(
          "\n  source alignment is too small to fulfill target alignment requirements, hence it \
           requires a runtime check.\n  use `try_transmute!()` instead.\n",
        );
      }
      Ok(())
    }

    #[inline(always)]
    pub const fn check_no_invalid_bit_pattern(self) -> Result<(), &'static str> {
      if bail!(Self::STATIC_ANALYZE).maybe_invalid_bit_pattern {
        return Err(
          "\n  target are not valid for all bit patterns.\n  and not all source bit patterns are \
           valid for target, which requires a runtime check.\n  use `try_transmute!()` instead.\n",
        );
      }
      Ok(())
    }

    #[inline(always)]
    pub const fn check_sound_target_mutation(self) -> Result<(), &'static str> {
      if !bail!(Self::STATIC_ANALYZE).sound_target_mutation {
        return Err(
          "\n  mutating target could result in invalid bit patterns in source, hence mutation is \
           disabled.\n",
        );
      }
      Ok(())
    }

    #[inline(always)]
    pub const fn transmute(self, src: Src) -> Dst
    where
      Src: Copy,
      Dst: Copy,
    {
      unwrap!(self.check());
      unwrap!(self.check_no_size_mismatch());
      unwrap!(self.check_no_invalid_bit_pattern());
      union Transmute<T: Copy, U: Copy> {
        src: T,
        dst: U,
      }
      unsafe { Transmute { src }.dst }
    }

    #[inline(always)]
    pub const fn transmute_copy(self, src: &Src) -> Dst
    where
      Src: Copy,
      Dst: Copy,
    {
      self.transmute(*src)
    }

    #[inline(always)]
    pub fn transmute_mut(self, src: &mut Src) -> &mut Dst {
      unwrap!(self.check());
      unwrap!(self.check_no_alignment_mismatch());
      unwrap!(self.check_no_size_mismatch());
      unwrap!(self.check_no_invalid_bit_pattern());
      unwrap!(self.check_sound_target_mutation());
      let metadata = self.convert_metadata(ptr::metadata(src));
      let ptr = ptr::from_raw_parts_mut::<Dst>(ptr::from_mut(src).cast(), metadata);
      unsafe { &mut *ptr }
    }

    #[inline(always)]
    pub const fn transmute_ref(self, src: &Src) -> &Dst {
      unwrap!(self.check());
      unwrap!(self.check_no_alignment_mismatch());
      unwrap!(self.check_no_size_mismatch());
      unwrap!(self.check_no_invalid_bit_pattern());
      let metadata = self.convert_metadata(ptr::metadata(src));
      let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
      unsafe { &*ptr }
    }

    #[inline(always)]
    pub const fn try_transmute_copy(self, src: &Src) -> Option<Dst>
    where
      Src: Copy,
      Dst: Copy,
    {
      unwrap!(self.check());
      unwrap!(self.check_no_size_mismatch());
      let ptr = ptr::from_ref(src).cast::<Dst>();
      match is_bits_valid(ptr) {
        true => Some(unsafe { *ptr }),
        _ => None,
      }
    }

    #[inline(always)]
    pub fn try_transmute_mut(self, src: &mut Src) -> Result<&mut Dst, Error> {
      unwrap!(self.check());
      unwrap!(self.check_sound_target_mutation());
      if !is_aligned_to::<Src>(src, align_of::<Dst>()) {
        return Err(Error::AlignmentMismatch);
      }
      let metadata = match self.try_convert_metadata(ptr::metadata(src)) {
        Ok(metadata) => metadata,
        Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
      };
      let ptr = ptr::from_raw_parts_mut::<Dst>(ptr::from_mut(src).cast(), metadata);
      match is_bits_valid(ptr) {
        true => Ok(unsafe { &mut *ptr }),
        _ => Err(Error::InvalidBitPattern),
      }
    }

    #[inline(always)]
    pub const fn try_transmute_ref(self, src: &Src) -> Result<&Dst, Error> {
      unwrap!(self.check());
      if !is_aligned_to::<Src>(src, align_of::<Dst>()) {
        return Err(Error::AlignmentMismatch);
      }
      let metadata = match self.try_convert_metadata(ptr::metadata(src)) {
        Ok(metadata) => metadata,
        Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
      };
      let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
      match is_bits_valid(ptr) {
        true => Ok(unsafe { &*ptr }),
        _ => Err(Error::InvalidBitPattern),
      }
    }

    #[inline(always)]
    const fn convert_metadata(self, metadata: usize) -> usize {
      unwrap!(self.check());
      unwrap!(self.check_no_size_mismatch());
      match size_of_tail::<Dst>() {
        0 => 0,
        n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
      }
    }

    #[inline(always)]
    const fn try_convert_metadata(self, metadata: usize) -> Result<usize, isize> {
      unwrap!(self.check());
      let check = match unwrap!(Self::STATIC_ANALYZE).maybe_size_mismatch {
        None => Ok(()),
        Some(check) => check.validate(metadata),
      };
      match check {
        Err(n) => Err(n * size_of_tail::<Src>() as isize),
        _ if size_of_tail::<Dst>() == 0 => Ok(0),
        _ => Ok((size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / size_of_tail::<Dst>()),
      }
    }
  }

  impl<Src: ?Sized, Dst: ?Sized> Copy for Transmutation<Src, Dst> {}

  impl<Src: ?Sized, Dst: ?Sized> Clone for Transmutation<Src, Dst> {
    fn clone(&self) -> Self {
      *self
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
    let (sh, st) = (min_size_of::<Src>(), size_of_tail::<Src>());
    let (dh, dt) = (min_size_of::<Dst>(), size_of_tail::<Dst>());
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
      Some(IsEmpty) | None => sh,
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
        Init(init) => bail!(src.intersects(init.layout)),
        _ => src.shift_bytes(1),
      };
    }
    let t = src.finish();
    if !t.reachable {
      return Err("none of the bit patterns in source are valid in target");
    }
    // Static analyze result.
    let maybe_invalid_bit_pattern = !t.infallible;
    let sound_target_mutation = t.mutable;
    Ok(StaticAnalyze { maybe_size_mismatch, maybe_invalid_bit_pattern, sound_target_mutation })
  }

  // impl Dropless ---------------------------------------------------------------------------------

  #[doc(hidden)]
  pub struct DroplessRepr;

  unsafe impl<T: ?Sized + Data> Repr<Dropless<T>> for DroplessRepr {
    type Type = T;
    type Rest = ();
    const OFFSET: usize = 0;
    const FIELDS: usize = 1;
  }

  unsafe impl<T: ?Sized + Data> Data for Dropless<T> {
    type Item = ();
    type Tail = T::Tail;
    type Repr = DroplessRepr;
    const LAYOUT: Layout = Layout::Struct;
    const HEADER: usize = T::HEADER;
  }

  impl<T: ?Sized> Dropless<T> {
    const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
  }

  impl<T> Dropless<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
      #[allow(clippy::let_unit_value)]
      let _ = Self::ASSERT_NOT_DROP;
      Self(ManuallyDrop::new(value))
    }

    #[inline]
    pub const fn into_inner(self) -> T {
      #[allow(clippy::let_unit_value)]
      let _ = Self::ASSERT_NOT_DROP;
      ManuallyDrop::into_inner(self.0)
    }
  }

  impl<T: ?Sized> Deref for Dropless<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
      self.0.deref()
    }
  }

  impl<T: ?Sized> DerefMut for Dropless<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
      self.0.deref_mut()
    }
  }

  impl<T: Display + ?Sized> Display for Dropless<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Display::fmt(self.deref(), f)
    }
  }

  impl<T: Debug + ?Sized> Debug for Dropless<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(self.deref(), f)
    }
  }

  // impl Unalign ----------------------------------------------------------------------------------

  impl<T> Unalign<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
      Self(value)
    }
  }

  impl<T: Copy> Unalign<T> {
    #[inline]
    pub const fn into_inner(self) -> T {
      self.0
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

  unsafe impl<T: Word> Data for T {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Word { layout: T::WORD_LAYOUT };
    const HEADER: usize = mem::size_of::<Self>();
  }

  unsafe impl<T: Copy + Data> Data for [T] {
    type Item = T;
    type Tail = T;
    type Repr = ();
    const LAYOUT: Layout = Layout::Slice;
    const HEADER: usize = 0;
  }

  unsafe impl<T: Copy + Data, const N: usize> Data for [T; N] {
    type Item = T;
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Array;
    const HEADER: usize = mem::size_of::<Self>();
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
    type Tail = B::Tail;
    type Repr = Tuple0;
    const LAYOUT: Layout = Layout::Struct;
    const HEADER: usize = mem::size_of::<Self>();
  }

  pub struct Start;
  pub struct End;

  unsafe impl<T: Copy + Data> Repr<Range<T>> for Start {
    type Type = T;
    type Rest = End;
    const OFFSET: usize = offset_of!(Range<T>, start);
    const FIELDS: usize = 2;
  }

  unsafe impl<T: Copy + Data> Repr<Range<T>> for End {
    type Type = T;
    type Rest = ();
    const OFFSET: usize = offset_of!(Range<T>, end);
    const FIELDS: usize = 1;
  }

  unsafe impl<T: Copy + Data> Data for Range<T> {
    type Item = ();
    type Tail = ();
    type Repr = Start;
    const LAYOUT: Layout = Layout::Struct;
    const HEADER: usize = mem::size_of::<Self>();
  }

  // impl Word -------------------------------------------------------------------------------------

  unsafe impl<T: ?Sized + 'static> Word for PhantomData<T> {
    type Raw = Self;
    const WORD_LAYOUT: &'static WordLayout = layout!(any);
  }

  macro_rules! impl_word {
    ($($ty:ty : $raw:ty : ($($layout:tt)+))+) => {
      $(
        unsafe impl Word for $ty {
          type Raw = $raw;
          const WORD_LAYOUT: &'static WordLayout = layout!($($layout)+);
        }
      )+
    };
  }

  impl_word!(
    ():():(any)
    u8:Self:(any) u16:Self:(any) u32:Self:(any) u64:Self:(any) u128:Self:(any) usize:Self:(any)
    i8:Self:(any) i16:Self:(any) i32:Self:(any) i64:Self:(any) i128:Self:(any) isize:Self:(any)
    Option<NonZeroU8>:Self:(any) Option<NonZeroU16>:Self:(any) Option<NonZeroU32>:Self:(any)
    Option<NonZeroU64>:Self:(any) Option<NonZeroU128>:Self:(any) Option<NonZeroUsize>:Self:(any)
    bool:u8:(0 | 1)
    NonZeroU8:u8:(!0) NonZeroU16:u16:(!0) NonZeroU32:u32:(!0) NonZeroU64:u64:(!0)
    NonZeroU128:u128:(!0) NonZeroUsize:usize:(!0)
  );
}
