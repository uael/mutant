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
//! assert_eq!(&Position { x: 21, y: 12 }, mutant::transmute_ref(&loot.position));
//! assert_eq!(Position { x: 21, y: 12 }, mutant::transmute(loot.position)); // Position is `Copy`
//!
//! let mut position: &mut Position = mutant::transmute_mut(&mut loot.position);
//! position.x *= 2;
//!
//! let loot: &Loot<[Item]> = &loot;
//! let loot_bytes: &[u8] = mutant::transmute_ref(loot);
//! assert_eq!(
//!   &[21 * 2, 12, 0xfe, 0xca, 0x42, 1, 0x21, 0],
//!   loot_bytes
//! );
//! assert_eq!(
//!   Ok(loot),
//!   mutant::try_transmute_ref::<_, Loot<[Item]>>(loot_bytes)
//! );
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
//! adhere to Rust's safety guarantees. Any use of `unsafe` code is carefully evalcuated and
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
use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;
use core::num::NonZeroU8;
use core::num::NonZeroUsize;
use core::ops::Range;

use const_panic::concat_panic;

use crate::MetadataChecks::{IsEmpty, NoneToRemove};
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
  InvalidBitPattern,
}

/// Specifies the byte order for raw data layout.
///
/// This enum is used in specifying how multi-byte data types are laid out in memory,
/// particularly useful in scenarios like parsing binary data formats or interfacing
/// with hardware.
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering { Lsb = 1, Msb }

/// Represents the checks that needs to be done over the source metadata for a transmutation
/// between two data types.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MetadataChecks {
  Is(usize),
  IsAtLeast(usize),
  IsMultipleOf(usize),
  IsAtLeastThenMultipleOf(usize, usize),
  IsEmpty,
  NoneToRemove,
}

/// Represents the memory layout of a type.
///
/// The [`Layout`] enum is used to categorize the memory representation of a type, which is
/// essential for safe and correct transmutation. It describes how data is laid out in memory,
/// providing the necessary context for transmutation operations.
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Layout { Void, Slice, Array, Struct, Raw { layout: RawLayout } }

/// Describes a byte-level memory layout. It is primarily used as the return type of
/// the [`byte_layout_at`] function.
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ByteLayout { Uninit, OutOfBounds, Init { layout: &'static RawLayout, offset: usize } }

/// Raw data layout (typically for integer types or enum discriminants).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct RawLayout {
  /// Byte ordering.
  ordering: Ordering,
  /// Allowed value ranges.
  bit_patterns: Option<&'static [Range<u64>]>,
  /// Data width in bytes.
  width: usize,
}

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
/// // Check the alignment of a u32 wrapped with `Unalign`
/// assert_eq!(1, mutant::align_of::<Unalign<u32>>());
///
/// let byte: Unalign<u8> = 123.into();
/// assert_eq!(123, byte.into_inner()); // `u8` is `Copy`
/// assert_eq!(123, *byte); // `u8` is already unaligned, `byte` can be  deferenced.
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

pub unsafe trait Repr<T: ?Sized + Data> {
  type Type: ?Sized + Data;
  type Rest: Repr<T>;
  const OFFSET: usize;
  const FIELDS: usize;
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
  /// For [`Sized`](Sized) types, this is equals to [`mem::size_of`](mem::size_of)`::<Self>()`.
  #[doc(hidden)]
  const HEADER: usize;
}

pub unsafe trait RawData: Copy + Data {
  const RAW_LAYOUT: RawLayout;
}

// Macros ------------------------------------------------------------------------------------------

/// Helper marco to evaluate code at compile-time.
#[doc(hidden)]
#[macro_export]
macro_rules! comptime {
  // default return type to `()` when none are provided.
  (|$( $arg:ident $(: $b0:ident $(+ $bn:ident)*)? ),*| $e:expr) => {
    $crate::comptime!(() |$( $arg $(: $b0 $(+ $bn)*)? ),*| $e)
  };
  // this use the `impl X { const Y: Ty = <expr>; }` trick.
  ($ret:path |$( $arg:ident $(: $b0:ident $(+ $bn:ident)*)? ),*| $e:expr) => {{
    // declare a private struct with all type arguments as `PhantomData`.
    struct Comptime<$( $arg: ?Sized ),*>($( PhantomData<$arg> ),*);
    // impl the struct and declare a `RESULT` constant assigned to `$e`.
    impl<$( $arg: ?Sized $(+ $b0 $(+ $bn)*)? ),*> Comptime<$( $arg ),*> { const RESULT: $ret = $e; }
    // returns the compile-time evaluated constant.
    Comptime::<$( $arg ),*>::RESULT
  }};
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
macro_rules! unwrap {
  ($e:expr $(,)?) => {
    match $e {
      Ok(ok) => ok,
      Err(err) => panic!("{}", err),
    }
  };
}

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
macro_rules! transmutation {
  ($($checks:ident),* $(,)?) => {{
    struct Comptime<T: ?Sized, U: ?Sized>(::core::marker::PhantomData<(*const T, *const U)>);

    impl<Src: ?Sized + $crate::Data, Dst: ?Sized + $crate::Data> Comptime<Src, Dst> {
      const TRANSMUTATION: $crate::Transmutation2<Src, Dst> = {
        let transmutation = $crate::Transmutation2::new();
        $crate::unwrap!(transmutation.check());
        $($crate::unwrap!(transmutation.$checks());)+
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
pub const fn byte_layout_at<T: ?Sized + Data>(offset: usize) -> ByteLayout {
  #[inline]
  const fn repr_contains<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> bool {
    offset >= R::OFFSET
      && (is_unsized::<R::Type>() || offset < R::OFFSET + min_size_of::<R::Type>())
  }

  #[inline]
  const fn repr_byte_layout_at<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> ByteLayout {
    match R::FIELDS {
      // Reached the end of the struct, returns an uninitialized layout.
      0 => ByteLayout::Uninit,
      // `offset` does offset in field, defer to its inner layout.
      _ if repr_contains::<R, T>(offset) => byte_layout_at::<R::Type>(offset - R::OFFSET),
      // `offset` does not offset in field, continue with next field.
      _ => repr_byte_layout_at::<R::Rest, T>(offset),
    }
  }

  // Check for `offset` to be in bounds.
  if is_sized::<T>() && offset >= min_size_of::<T>() {
    return ByteLayout::OutOfBounds;
  }
  // Check for `ZST` slices.
  if matches!(T::LAYOUT, Layout::Slice | Layout::Array) && mem::size_of::<T::Item>() == 0 {
    return ByteLayout::OutOfBounds;
  }
  match &T::LAYOUT {
    Layout::Void => unreachable!(),
    Layout::Slice | Layout::Array => byte_layout_at::<T::Item>(offset % mem::size_of::<T::Item>()),
    Layout::Struct => repr_byte_layout_at::<T::Repr, T>(offset),
    Layout::Raw { layout } => layout.offset(offset),
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
      _ => is_bits_valid(repr_field::<R, _>(ptr)) || repr_is_bits_valid::<R::Rest, _>(ptr),
    }
  }

  match Dst::LAYOUT {
    Layout::Void => false,
    Layout::Struct => repr_is_bits_valid::<Dst::Repr, Dst>(ptr),
    Layout::Slice | Layout::Array => {
      let len = match Dst::LAYOUT {
        Layout::Array => min_size_of::<Dst>() / mem::size_of::<Dst::Item>(),
        _ => ptr::metadata(ptr),
      };
      let mut i = 0;
      while i < len {
        // SAFETY: `ptr` came from a valid reference and `i` is in type bounds.
        if is_bits_valid(unsafe { ptr.cast::<Dst::Item>().add(i) }) {
          return true;
        }
        i += 1;
      }
      false
    }
    Layout::Raw { layout } if layout.is_any() => true,
    Layout::Raw { layout } => {
      let (mut offset, mut val) = (0, 0);
      while offset < min_size_of::<Dst>() {
        // SAFETY: `ptr` came from a valid reference and `offset` is in type bounds.
        let byte = unsafe { ptr.cast::<u8>().add(offset).read() };
        val |= (byte as u64) << layout.ordering.shift(offset, min_size_of::<Dst>());
        offset += 1;
      }
      layout.contains(val)
    }
  }
}

#[inline]
#[track_caller]
pub const fn metadata_checks<Src: ?Sized + Data, Dst: ?Sized + Data>() -> MetadataChecks {
  let (ah, at, bh, bt) = (
    min_size_of::<Src>(),
    NonZeroUsize::new(size_of_tail::<Src>()),
    min_size_of::<Dst>(),
    NonZeroUsize::new(size_of_tail::<Dst>()),
  );
  let mut min_at_count = 0; // Minimum number of `at` required to align `ah[at]` and `bh[bt]`.
  if ah < bh {
    let at = match at {
      Some(tail) => tail.get(),
      _ => concat_panic!("cannot transmute between types of different sizes, source too small"),
    };
    let missing = bh - ah;
    if bt.is_none() && missing % at != 0 {
      concat_panic!(
        "cannot transmute between types of incompatible byte layouts, missing bytes to fit the \
         target is not a multiple of the source `DST` trailing slice item size",
      );
    }
    min_at_count = (missing + at - 1) / at; // Ceiling of division.
  } else if ah > bh {
    if let Some(bt) = bt {
      let remaining = ah - bh;
      if at.is_none() && remaining % bt.get() != 0 {
        concat_panic!(
          "cannot transmute between types of incompatible byte layouts, remaining bytes to fit in \
           the target is not a multiple of the target `DST` trailing slice item size",
        );
      }
    }
  }
  match (at, bt) {
    (None, _) => MetadataChecks::NoneToRemove,
    (Some(at), None) => {
      if ah < bh {
        MetadataChecks::Is((bh - ah) / at.get())
      } else {
        MetadataChecks::IsEmpty
      }
    }
    (Some(at), Some(bt)) => {
      #[inline]
      const fn gcd(a: usize, b: usize) -> usize {
        match b {
          0 => a,
          _ => gcd(b, a % b),
        }
      }

      let (at, bt) = (at.get(), bt.get());
      let gcd = gcd(at, bt);
      let lcm = (at * bt) / gcd;
      while (ah + min_at_count * at - bh) % bt != 0 {
        if min_at_count * at > lcm {
          concat_panic!(
            "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
             catchup on target",
          );
        }
        min_at_count += 1;
      }
      // Compute the minimum number of times `at` must be repeated to reach a size that is a
      // multiple of `bt`.
      match (min_at_count, bt / gcd) {
        (0, 1) => MetadataChecks::NoneToRemove,
        (0, m) => MetadataChecks::IsMultipleOf(m),
        (n, 1) => MetadataChecks::IsAtLeast(n),
        (n, m) => MetadataChecks::IsAtLeastThenMultipleOf(n, m),
      }
    }
  }
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  T::HEADER
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
pub const fn transmute_metadata<Src: ?Sized + Data, Dst: ?Sized + Data>(
  metadata: usize,
) -> Result<usize, isize> {
  let checks = comptime!(MetadataChecks |Src: Data, Dst: Data| metadata_checks::<Src, Dst>());
  match checks.check(metadata) {
    0 if size_of_tail::<Dst>() == 0 => Ok(0),
    0 => Ok((size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / size_of_tail::<Dst>()),
    n => Err(n * size_of_tail::<Src>() as isize),
  }
}

#[inline]
#[track_caller]
pub const fn transmute_min_size<Src: ?Sized + Data, Dst: ?Sized + Data>() -> usize {
  use MetadataChecks::*;
  match metadata_checks::<Src, Dst>() {
    Is(n) | IsAtLeast(n) => min_size_of::<Src>() + (n * size_of_tail::<Src>()),
    IsMultipleOf(m) => min_size_of::<Src>() + (m * size_of_tail::<Src>()),
    IsAtLeastThenMultipleOf(n, _) => min_size_of::<Src>() + (n * size_of_tail::<Src>()),
    IsEmpty | NoneToRemove => min_size_of::<Src>(),
  }
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

#[inline]
#[track_caller]
pub const unsafe fn transmute_unchecked<Src: Copy, Dst: Copy>(src: Src) -> Dst {
  union Transmute<T: Copy, U: Copy> {
    src: T,
    dst: U,
  }
  unsafe { Transmute { src }.dst }
}

/// FIXME: Make as `const` fn once `const_mut_refs` is stabilized.
#[inline]
#[track_caller]
pub fn transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(src: &mut T) -> &mut U {
  let metadata = layout::for_infallible_mut::<T, U>(ptr::metadata(src));
  let ptr = ptr::from_raw_parts_mut::<U>(src as *mut T as *mut (), metadata);
  unsafe { &mut *ptr }
}

#[inline]
#[track_caller]
pub const fn transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> &U {
  let metadata = layout::for_infallible_ref::<T, U>(ptr::metadata(src));
  let ptr = ptr::from_raw_parts::<U>(src as *const T as *const (), metadata);
  unsafe { &*ptr }
}

/// FIXME: Make as `const` fn once `const_refs_to_cell` is stabilized.
#[inline]
#[track_caller]
pub fn try_transmute<T: Copy + Data, U: Copy + Data>(src: T) -> Option<U> {
  let _ = layout::for_copy::<T, U>();
  union Transmute<T: Copy, U: Copy> {
    src: T,
    dst: U,
  }
  let dst = unsafe { Transmute { src }.dst };
  match is_bits_valid(&dst) {
    true => Some(dst),
    _ => None,
  }
}

#[inline]
#[track_caller]
pub fn try_transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(src: &mut T) -> Result<&mut U, Error> {
  if !is_aligned_to::<T>(src, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match layout::for_mut::<T, U>(ptr::metadata(src)) {
    Ok(metadata) => metadata,
    Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
  };
  let ptr = ptr::from_raw_parts_mut::<U>(src as *mut T as *mut (), metadata);
  match is_bits_valid(ptr) {
    true => Ok(unsafe { &mut *ptr }),
    _ => Err(Error::InvalidBitPattern),
  }
}

#[inline]
#[track_caller]
pub const fn try_transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> Result<&U, Error> {
  if !is_aligned_to::<T>(src, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match layout::for_ref::<T, U>(ptr::metadata(src)) {
    Ok(metadata) => metadata,
    Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
  };
  let ptr = ptr::from_raw_parts::<U>(src as *const T as *const (), metadata);
  match is_bits_valid(ptr) {
    true => Ok(unsafe { &*ptr }),
    _ => Err(Error::InvalidBitPattern),
  }
}

/// # Examples
/// ```
/// use mutant::{is_infallible};
///
/// assert_eq!(true, is_infallible::<bool, u8>());
/// assert_eq!(false, is_infallible::<u8, bool>());
///
/// assert_eq!(true, is_infallible::<(u8, u8), u16>());
/// assert_eq!(true, is_infallible::<u16, (u8, u8)>());
///
/// assert_eq!(true, is_infallible::<(bool, bool), u16>());
/// assert_eq!(false, is_infallible::<u16, (bool, bool)>());
/// ```
#[inline]
#[track_caller]
pub const fn is_infallible<Src: ?Sized + Data, Dst: ?Sized + Data>() -> bool {
  let (mut off, end) = (0, transmute_min_size::<Src, Dst>());
  while off < end {
    let dst = byte_layout_at::<Dst>(off);
    match dst {
      ByteLayout::OutOfBounds => unreachable!(),
      ByteLayout::Init { layout, .. } => match layout.static_analyze::<Src>(off) {
        Err(err) => panic!("{}", err),
        Ok((need_bits_check, _)) => {
          if need_bits_check {
            return false;
          }
        }
      },
      _ => {}
    };
    off += dst.width();
  }
  true
}

pub struct Transmute<Src: ?Sized, Dst: ?Sized> {
  check_bits: bool,
  check_metadata: Option<MetadataChecks>,
  _types: PhantomData<(*const Src, *const Dst)>,
}

pub struct TransmuteInfallible<Src: ?Sized, Dst: ?Sized> {
  _types: PhantomData<(*const Src, *const Dst)>,
}

pub struct TransmuteAligned<Src: ?Sized, Dst: ?Sized> {
  _types: PhantomData<(*const Src, *const Dst)>,
}

#[derive(Copy, Clone)]
pub struct Runtime<
  const TARGET_MUTATION: bool = false,
  const NO_SIZE_MISMATCH: bool = false,
  const NO_ALIGNMENT_MISMATCH: bool = false,
  const NO_INVALID_BIT_PATTERN: bool = false,
>;

#[derive(Copy, Clone)]
pub struct Transmutation<Src: ?Sized, Dst: ?Sized, Rt = Runtime> {
  sound_target_mutation: bool,
  maybe_invalid_bit_pattern: bool,
  maybe_size_mismatch: Option<MetadataChecks>,
  _types: PhantomData<(Rt, *const Src, *const Dst)>,
}

pub struct Transmutation2<Src: ?Sized, Dst: ?Sized> {
  source: PhantomData<Src>,
  target: PhantomData<Dst>,
}

impl<Src: ?Sized, Dst: ?Sized> Copy for Transmutation2<Src, Dst> {}
impl<Src: ?Sized, Dst: ?Sized> Clone for Transmutation2<Src, Dst> {
  fn clone(&self) -> Self {
    *self
  }
}

#[derive(Copy, Clone)]
struct StaticAnalyze {
  maybe_size_mismatch: Option<MetadataChecks>,
  maybe_invalid_bit_pattern: bool,
  sound_target_mutation: bool,
}

impl<Src: ?Sized + Data, Dst: ?Sized + Data> Transmutation2<Src, Dst> {
  const STATIC_ANALYZE: Result<StaticAnalyze, &'static str> = static_analyze::<Src, Dst>();

  pub const fn new() -> Self {
    Self { source: PhantomData, target: PhantomData }
  }

  pub const fn check(self) -> Result<(), &'static str> {
    bail!(Self::STATIC_ANALYZE);
    Ok(())
  }

  pub const fn check_no_size_mismatch(self) -> Result<(), &'static str> {
    if bail!(Self::STATIC_ANALYZE).maybe_size_mismatch.is_some() {
      return Err(
        "\n  source metadata is not directly convertible to fulfill target requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(())
  }

  pub const fn check_no_alignment_mismatch(self) -> Result<(), &'static str> {
    if align_of::<Dst>() > align_of::<Src>() {
      return Err(
        "\n  source alignment is too small to fulfill target alignment requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(())
  }

  pub const fn check_no_invalid_bit_pattern(self) -> Result<(), &'static str> {
    if bail!(Self::STATIC_ANALYZE).maybe_invalid_bit_pattern {
      return Err(
        "\n  target are not valid for all bit patterns.\n  and not all source bit patterns are \
         valid for target, which requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(())
  }

  pub const fn check_sound_target_mutation(self) -> Result<(), &'static str> {
    if !bail!(Self::STATIC_ANALYZE).sound_target_mutation {
      return Err(
        "\n  mutating target could result in invalid bit patterns in source, hence mutation is \
         disabled.\n",
      );
    }
    Ok(())
  }

  pub const fn transmute(self, src: Src) -> Dst
  where
    Src: Copy,
    Dst: Copy,
  {
    unwrap!(self.check());
    unwrap!(self.check_no_size_mismatch());
    unwrap!(self.check_no_invalid_bit_pattern());
    unsafe { transmute_unchecked(src) }
  }

  pub const fn transmute_copy(self, src: &Src) -> Dst
  where
    Src: Copy,
    Dst: Copy,
  {
    self.transmute(*src)
  }

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

  pub const fn transmute_ref(self, src: &Src) -> &Dst {
    unwrap!(self.check());
    unwrap!(self.check_no_alignment_mismatch());
    unwrap!(self.check_no_size_mismatch());
    unwrap!(self.check_no_invalid_bit_pattern());
    let metadata = self.convert_metadata(ptr::metadata(src));
    let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
    unsafe { &*ptr }
  }

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

  const fn convert_metadata(self, metadata: usize) -> usize {
    unwrap!(self.check());
    unwrap!(self.check_no_size_mismatch());
    match size_of_tail::<Dst>() {
      0 => 0,
      n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
    }
  }

  const fn try_convert_metadata(self, metadata: usize) -> Result<usize, isize> {
    unwrap!(self.check());
    use MetadataChecks::*;
    let delta = match unwrap!(Self::STATIC_ANALYZE).maybe_size_mismatch {
      None => 0,
      Some(check) => match check {
        Is(n) if metadata == n => 0,
        IsAtLeast(n) if metadata >= n => 0,
        Is(n) | IsAtLeast(n) => n as isize - metadata as isize,
        IsMultipleOf(m) if metadata % m == 0 => 0,
        IsMultipleOf(m) => m as isize - (metadata % m) as isize,
        IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => 0,
        IsAtLeastThenMultipleOf(n, _) if metadata < n => n as isize - metadata as isize,
        IsAtLeastThenMultipleOf(_, m) => m as isize - (metadata % m) as isize,
        IsEmpty if metadata == 0 => 0,
        IsEmpty => -(metadata as isize),
        NoneToRemove => todo!(),
      },
    };
    match delta {
      0 if size_of_tail::<Dst>() == 0 => Ok(0),
      0 => Ok((size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / size_of_tail::<Dst>()),
      n => Err(n * size_of_tail::<Src>() as isize),
    }
  }
}

const fn static_analyze<Src, Dst>() -> Result<StaticAnalyze, &'static str>
where
  Src: ?Sized + Data,
  Dst: ?Sized + Data,
{
  use MetadataChecks::*;

  let (sh, st) = (min_size_of::<Src>(), size_of_tail::<Src>());
  let (dh, dt) = (min_size_of::<Dst>(), size_of_tail::<Dst>());
  let mut min_at_count = 0; // Minimum number of `at` required to align `sh[st]` and `dh[dt]`.
  if sh < dh {
    if st == 0 {
      return Err("source too small");
    }
    let missing = dh - sh;
    if dt == 0 && missing % st != 0 {
      return Err(
        "missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
         item size",
      );
    }
    min_at_count = (missing + st - 1) / st; // Ceiling of division.
  } else if sh > dh && dt != 0 {
    let remaining = sh - dh;
    if st == 0 && remaining % dt != 0 {
      return Err(
        "remaining bytes to fit in the target is not a multiple of the target `DST` trailing \
         slice item size",
      );
    }
  }
  let maybe_size_mismatch = match (st, dt) {
    (0, _) => None,
    (st, 0) if sh < dh => Some(Is((dh - sh) / st)),
    (_, 0) => Some(IsEmpty),
    (st, dt) => {
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
      // Compute the minimum number of times `st` must be repeated to reach a size that is a
      // multiple of `dt`.
      match (min_at_count, dt / gcd) {
        (0, 1) => None,
        (0, m) => Some(IsMultipleOf(m)),
        (n, 1) => Some(IsAtLeast(n)),
        (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
      }
    }
  };
  let min_size = match maybe_size_mismatch {
    Some(IsEmpty) | None => sh,
    Some(IsMultipleOf(m)) => sh + (m * st),
    Some(Is(n) | IsAtLeast(n) | IsAtLeastThenMultipleOf(n, _)) => sh + (n * st),
    Some(NoneToRemove) => todo!(),
  };
  let mut off = 0;
  let (mut maybe_invalid_bit_pattern, mut sound_target_mutation) = (false, true);
  while off < min_size {
    match byte_layout_at::<Dst>(off) {
      ByteLayout::Init { layout, .. } => match layout.static_analyze::<Src>(off) {
        Err(err) => return Err(err),
        Ok(ok) => {
          maybe_invalid_bit_pattern = maybe_invalid_bit_pattern || ok.0;
          sound_target_mutation = sound_target_mutation && ok.1;
          off += layout.width;
        }
      },
      _ => off += 1,
    }
  }
  Ok(StaticAnalyze { maybe_size_mismatch, maybe_invalid_bit_pattern, sound_target_mutation })
}

impl<Src: ?Sized, Dst: ?Sized, Rt: Copy> Transmutation<Src, Dst, Rt> {
  const fn cast<NewRt>(self) -> Transmutation<Src, Dst, NewRt> {
    Transmutation {
      sound_target_mutation: self.sound_target_mutation,
      maybe_invalid_bit_pattern: self.maybe_invalid_bit_pattern,
      maybe_size_mismatch: self.maybe_size_mismatch,
      _types: PhantomData,
    }
  }
}

impl<Src: ?Sized, Dst: ?Sized, const A: bool, const B: bool, const C: bool>
  Transmutation<Src, Dst, Runtime<false, A, B, C>>
{
  pub const fn target_mutation(
    self,
  ) -> Result<Transmutation<Src, Dst, Runtime<true, A, B, C>>, &'static str> {
    if !self.sound_target_mutation {
      return Err(
        "\n  mutating target could result in invalid bit patterns in source, hence mutation is \
         disabled.\n",
      );
    }
    Ok(self.cast())
  }
}

impl<Src: ?Sized, Dst: ?Sized, const A: bool, const B: bool, const C: bool>
  Transmutation<Src, Dst, Runtime<A, false, B, C>>
{
  pub const fn no_size_mismatch(
    self,
  ) -> Result<Transmutation<Src, Dst, Runtime<A, true, B, C>>, &'static str> {
    if align_of::<Dst>() > align_of::<Src>() {
      return Err(
        "\n  source metadata is not directly convertible to fulfill target requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(self.cast())
  }
}

impl<Src: ?Sized, Dst: ?Sized, const A: bool, const B: bool, const C: bool>
  Transmutation<Src, Dst, Runtime<A, B, false, C>>
{
  pub const fn no_alignment_mismatch(
    self,
  ) -> Result<Transmutation<Src, Dst, Runtime<A, B, true, C>>, &'static str> {
    if align_of::<Dst>() > align_of::<Src>() {
      return Err(
        "\n  source alignment is too small to fulfill target alignment requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(self.cast())
  }
}

impl<Src: ?Sized, Dst: ?Sized, const A: bool, const B: bool, const C: bool>
  Transmutation<Src, Dst, Runtime<A, B, C, false>>
{
  pub const fn no_invalid_bit_pattern(
    self,
  ) -> Result<Transmutation<Src, Dst, Runtime<A, B, C, true>>, &'static str> {
    if self.maybe_invalid_bit_pattern {
      return Err(
        "\n  target are not valid for all bit patterns.\n  and not all source bit patterns are \
         valid for target, which requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(self.cast())
  }
}

pub const fn transmute_checks<Src, Dst>() -> Result<Transmute<Src, Dst>, &'static str>
where
  Src: ?Sized + Data,
  Dst: ?Sized + Data,
{
  use MetadataChecks::*;

  let (sh, st) = (min_size_of::<Src>(), size_of_tail::<Src>());
  let (dh, dt) = (min_size_of::<Dst>(), size_of_tail::<Dst>());
  let mut min_at_count = 0; // Minimum number of `at` required to align `sh[st]` and `dh[dt]`.
  if sh < dh {
    if st == 0 {
      return Err("source too small");
    }
    let missing = dh - sh;
    if dt == 0 && missing % st != 0 {
      return Err(
        "missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
         item size",
      );
    }
    min_at_count = (missing + st - 1) / st; // Ceiling of division.
  } else if sh > dh && dt != 0 {
    let remaining = sh - dh;
    if st == 0 && remaining % dt != 0 {
      return Err(
        "remaining bytes to fit in the target is not a multiple of the target `DST` trailing \
         slice item size",
      );
    }
  }
  let check_metadata = match (st, dt) {
    (0, _) => None,
    (st, 0) if sh < dh => Some(Is((dh - sh) / st)),
    (_, 0) => Some(IsEmpty),
    (st, dt) => {
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
      // Compute the minimum number of times `st` must be repeated to reach a size that is a
      // multiple of `dt`.
      match (min_at_count, dt / gcd) {
        (0, 1) => None,
        (0, m) => Some(IsMultipleOf(m)),
        (n, 1) => Some(IsAtLeast(n)),
        (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
      }
    }
  };
  let min_size = match check_metadata {
    Some(IsEmpty) | None => sh,
    Some(IsMultipleOf(m)) => sh + (m * st),
    Some(Is(n) | IsAtLeast(n) | IsAtLeastThenMultipleOf(n, _)) => sh + (n * st),
    Some(NoneToRemove) => todo!(),
  };
  let mut off = 0;
  let mut check_bits = false;
  while off < min_size {
    match byte_layout_at::<Dst>(off) {
      ByteLayout::Init { layout, .. } => match layout.static_analyze::<Src>(off) {
        Err(err) => return Err(err),
        Ok((need_bits_check, _)) => {
          check_bits = check_bits || need_bits_check;
          off += layout.width;
        }
      },
      _ => off += 1,
    }
  }
  Ok(Transmute { check_bits, check_metadata, _types: PhantomData })
}

impl<Src: ?Sized + Data, Dst: ?Sized + Data> Transmute<Src, Dst> {
  #[track_caller]
  pub const fn into_infallible(self) -> Result<TransmuteInfallible<Src, Dst>, &'static str> {
    if self.check_bits {
      return Err(
        "\n  target are not valid for all byte patterns.\n  but not all source byte patterns are \
         valid for target, which requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    if self.check_metadata.is_some() {
      return Err(
        "\n  source metadata is not directly convertible to fulfill target requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(TransmuteInfallible { _types: PhantomData })
  }

  #[track_caller]
  pub const fn into_infallible_aligned(self) -> Result<TransmuteAligned<Src, Dst>, &'static str> {
    unwrap!(self.into_infallible()).into_aligned()
  }

  pub const fn convert_metadata(self, metadata: usize) -> Result<usize, isize> {
    use MetadataChecks::*;
    let delta = match self.check_metadata {
      None => 0,
      Some(check) => match check {
        Is(n) if metadata == n => 0,
        IsAtLeast(n) if metadata >= n => 0,
        Is(n) | IsAtLeast(n) => n as isize - metadata as isize,
        IsMultipleOf(m) if metadata % m == 0 => 0,
        IsMultipleOf(m) => m as isize - (metadata % m) as isize,
        IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => 0,
        IsAtLeastThenMultipleOf(n, _) if metadata < n => n as isize - metadata as isize,
        IsAtLeastThenMultipleOf(_, m) => m as isize - (metadata % m) as isize,
        IsEmpty if metadata == 0 => 0,
        IsEmpty => -(metadata as isize),
        NoneToRemove => todo!(),
      },
    };
    match delta {
      0 if size_of_tail::<Dst>() == 0 => Ok(0),
      0 => Ok((size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / size_of_tail::<Dst>()),
      n => Err(n * size_of_tail::<Src>() as isize),
    }
  }

  pub const fn try_transmute_copy(self, src: &Src) -> Option<Dst>
  where
    Src: Copy,
    Dst: Copy,
  {
    let ptr = ptr::from_ref(src).cast::<Dst>();
    match is_bits_valid(ptr) {
      true => Some(unsafe { *ptr }),
      _ => None,
    }
  }

  pub const fn try_transmute_ref(self, src: &Src) -> Result<&Dst, Error> {
    if !is_aligned_to::<Src>(src, align_of::<Dst>()) {
      return Err(Error::AlignmentMismatch);
    }
    let metadata = match self.convert_metadata(ptr::metadata(src)) {
      Ok(metadata) => metadata,
      Err(size_mismatch) => return Err(Error::SizeMismatch(size_mismatch)),
    };
    let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
    match is_bits_valid(ptr) {
      true => Ok(unsafe { &*ptr }),
      _ => Err(Error::InvalidBitPattern),
    }
  }
}

impl<Src: ?Sized + Data, Dst: ?Sized + Data> TransmuteInfallible<Src, Dst> {
  #[track_caller]
  pub const fn into_aligned(self) -> Result<TransmuteAligned<Src, Dst>, &'static str> {
    if align_of::<Dst>() > align_of::<Src>() {
      return Err(
        "\n  source alignment is too small to fulfill target alignment requirements, hence it \
         requires a runtime check.\n  use `try_transmute!()` instead.\n",
      );
    }
    Ok(TransmuteAligned { _types: PhantomData })
  }

  pub const fn transmute(self, src: Src) -> Dst
  where
    Src: Copy,
    Dst: Copy,
  {
    unsafe { transmute_unchecked(src) }
  }

  const fn convert_metadata(self, metadata: usize) -> usize {
    match size_of_tail::<Dst>() {
      0 => 0,
      n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
    }
  }

  pub const fn try_transmute_ref(self, src: &Src) -> Option<&Dst> {
    if !is_aligned_to::<Src>(src, align_of::<Dst>()) {
      return None;
    }
    let metadata = self.convert_metadata(ptr::metadata(src));
    let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
    Some(unsafe { &*ptr })
  }

  pub fn try_transmute_mut(self, src: &mut Src) -> Option<&mut Dst> {
    if !is_aligned_to::<Src>(src, align_of::<Dst>()) {
      return None;
    }
    let metadata = self.convert_metadata(ptr::metadata(src));
    let ptr = ptr::from_raw_parts_mut::<Dst>(ptr::from_mut(src).cast(), metadata);
    Some(unsafe { &mut *ptr })
  }
}

impl<Src: ?Sized + Data, Dst: ?Sized + Data> TransmuteAligned<Src, Dst> {
  const fn convert_metadata(self, metadata: usize) -> usize {
    match size_of_tail::<Dst>() {
      0 => 0,
      n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
    }
  }

  pub const fn transmute_ref(self, src: &Src) -> &Dst {
    let metadata = self.convert_metadata(ptr::metadata(src));
    let ptr = ptr::from_raw_parts::<Dst>(ptr::from_ref(src).cast(), metadata);
    unsafe { &*ptr }
  }

  pub fn transmute_mut(self, src: &mut Src) -> &mut Dst {
    let metadata = self.convert_metadata(ptr::metadata(src));
    let ptr = ptr::from_raw_parts_mut::<Dst>(ptr::from_mut(src).cast(), metadata);
    unsafe { &mut *ptr }
  }
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

// metadata ----------------------------------------------------------------------------------------

// mod layout2 {
//   use core::marker::PhantomData;
//   use const_panic::{concat_, concat_panic};
//   use crate::*;
//
//   /// Metadata requirements.
//   #[derive(Debug, Copy, Clone, Eq, PartialEq)]
//   pub struct Transmute<Src: ?Sized + Data, Dst: ?Sized + Data> {
//     inner: Inner,
//     types: PhantomData<(*const Src, *const Dst)>
//   }
//
//   /// Metadata inner requirements.
//   #[derive(Debug, Copy, Clone, Eq, PartialEq)]
//   enum Inner {
//     Is(usize),
//     IsAtLeast(usize),
//     IsMultipleOf(usize),
//     IsAtLeastThenMultipleOf(usize, usize),
//     IsEmpty,
//     None,
//   }
//
//   #[inline]
//   #[track_caller]
//   pub const fn transmute_requires<Src: ?Sized + Data, Dst: ?Sized + Data>() -> Requires<Src, Dst> {
//     let (ah, at, bh, bt) = (
//       min_size_of::<Src>(),
//       NonZeroUsize::new(size_of_tail::<Src>()),
//       min_size_of::<Dst>(),
//       NonZeroUsize::new(size_of_tail::<Dst>()),
//     );
//     concat_!()
//     let mut min_at_count = 0; // Minimum number of `at` required to align `ah[at]` and `bh[bt]`.
//     if ah < bh {
//       let at = match at {
//         Some(tail) => tail.get(),
//         _ => concat_panic!("cannot transmute between types of different sizes, source too small"),
//       };
//       let missing = bh - ah;
//       if bt.is_none() && missing % at != 0 {
//         concat_panic!(
//           "cannot transmute between types of incompatible byte layouts, missing bytes to fit \
//              the target is not a multiple of the source `DST` trailing slice item size",
//         );
//       }
//       min_at_count = (missing + at - 1) / at; // Ceiling of division.
//     } else if ah > bh {
//       if let Some(bt) = bt {
//         let remaining = ah - bh;
//         if at.is_none() && remaining % bt.get() != 0 {
//           concat_panic!(
//             "cannot transmute between types of incompatible byte layouts, remaining bytes to \
//                fit in the target is not a multiple of the target `DST` trailing slice item size",
//           );
//         }
//       }
//     }
//     let inner = match (at, bt) {
//       (None, _) => Inner::None,
//       (Some(at), None) => {
//         if ah < bh {
//           Inner::Is((bh - ah) / at.get())
//         } else {
//           Inner::IsEmpty
//         }
//       }
//       (Some(at), Some(bt)) => {
//         #[inline]
//         const fn gcd(a: usize, b: usize) -> usize {
//           match b {
//             0 => a,
//             _ => gcd(b, a % b),
//           }
//         }
//
//         let (at, bt) = (at.get(), bt.get());
//         let gcd = gcd(at, bt);
//         let lcm = (at * bt) / gcd;
//         while (ah + min_at_count * at - bh) % bt != 0 {
//           if min_at_count * at > lcm {
//             concat_panic!(
//               "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
//                  catchup on target",
//             );
//           }
//           min_at_count += 1;
//         }
//         // Compute the minimum number of times `at` must be repeated to reach a size that is a
//         // multiple of `bt`.
//         match (min_at_count, bt / gcd) {
//           (0, 1) => Inner::None,
//           (0, m) => Inner::IsMultipleOf(m),
//           (n, 1) => Inner::IsAtLeast(n),
//           (n, m) => Inner::IsAtLeastThenMultipleOf(n, m),
//         }
//       }
//     };
//     Requires { inner, types: PhantomData }
//   }
// }

mod layout {
  use core::marker::PhantomData;
  use core::num::NonZeroUsize;

  use crate::*;

  /// Metadata requirements.
  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  pub enum Requirements {
    Is(usize),
    IsAtLeast(usize),
    IsMultipleOf(usize),
    IsAtLeastThenMultipleOf(usize, usize),
    IsEmpty,
    None,
  }

  impl Requirements {
    #[inline]
    const fn delta(self, metadata: usize) -> isize {
      match self {
        Self::Is(n) if metadata == n => 0,
        Self::IsAtLeast(n) if metadata >= n => 0,
        Self::Is(n) | Self::IsAtLeast(n) => n as isize - metadata as isize,
        Self::IsMultipleOf(m) if metadata % m == 0 => 0,
        Self::IsMultipleOf(m) => m as isize - (metadata % m) as isize,
        Self::IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => 0,
        Self::IsAtLeastThenMultipleOf(n, _) if metadata < n => n as isize - metadata as isize,
        Self::IsAtLeastThenMultipleOf(_, m) => m as isize - (metadata % m) as isize,
        Self::IsEmpty if metadata == 0 => 0,
        Self::IsEmpty => -(metadata as isize),
        Self::None => 0,
      }
    }

    #[inline]
    #[track_caller]
    pub const fn of<Src: ?Sized + Data, Dst: ?Sized + Data>() -> Result<Self, &'static str> {
      let (ah, at, bh, bt) = (
        min_size_of::<Src>(),
        NonZeroUsize::new(size_of_tail::<Src>()),
        min_size_of::<Dst>(),
        NonZeroUsize::new(size_of_tail::<Dst>()),
      );
      let mut min_at_count = 0; // Minimum number of `at` required to align `ah[at]` and `bh[bt]`.
      if ah < bh {
        let at = match at {
          Some(tail) => tail.get(),
          _ => return Err("cannot transmute between types of different sizes, source too small"),
        };
        let missing = bh - ah;
        if bt.is_none() && missing % at != 0 {
          return Err(
            "cannot transmute between types of incompatible byte layouts, missing bytes to fit \
             the target is not a multiple of the source `DST` trailing slice item size",
          );
        }
        min_at_count = (missing + at - 1) / at; // Ceiling of division.
      } else if ah > bh {
        if let Some(bt) = bt {
          let remaining = ah - bh;
          if at.is_none() && remaining % bt.get() != 0 {
            return Err(
              "cannot transmute between types of incompatible byte layouts, remaining bytes to \
               fit in the target is not a multiple of the target `DST` trailing slice item size",
            );
          }
        }
      }
      let requirements = match (at, bt) {
        (None, _) => Self::None,
        (Some(at), None) => {
          if ah < bh {
            Self::Is((bh - ah) / at.get())
          } else {
            Self::IsEmpty
          }
        }
        (Some(at), Some(bt)) => {
          #[inline]
          const fn gcd(a: usize, b: usize) -> usize {
            match b {
              0 => a,
              _ => gcd(b, a % b),
            }
          }

          let (at, bt) = (at.get(), bt.get());
          let gcd = gcd(at, bt);
          let lcm = (at * bt) / gcd;
          while (ah + min_at_count * at - bh) % bt != 0 {
            if min_at_count * at > lcm {
              return Err(
                "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
                 catchup on target",
              );
            }
            min_at_count += 1;
          }
          // Compute the minimum number of times `at` must be repeated to reach a size that is a
          // multiple of `bt`.
          match (min_at_count, bt / gcd) {
            (0, 1) => Self::None,
            (0, m) => Self::IsMultipleOf(m),
            (n, 1) => Self::IsAtLeast(n),
            (n, m) => Self::IsAtLeastThenMultipleOf(n, m),
          }
        }
      };
      Ok(requirements)
    }
  }

  pub(crate) struct Transmute<
    Src: ?Sized,
    Dst: ?Sized,
    const COPY: bool = false,
    const MUTATION: bool = false,
    const INFALLIBLE: bool = false,
  >(PhantomData<(*const Src, Dst)>);

  impl<
      Src: ?Sized + Data,
      Dst: ?Sized + Data,
      const COPY: bool,
      const MUTATION: bool,
      const INFALLIBLE: bool,
    > Transmute<Src, Dst, COPY, MUTATION, INFALLIBLE>
  {
    pub const MIN_SIZE: usize = match Self::REQUIREMENTS {
      Requirements::Is(n) | Requirements::IsAtLeast(n) => {
        min_size_of::<Src>() + (n * size_of_tail::<Src>())
      }
      Requirements::IsMultipleOf(m) => min_size_of::<Src>() + (m * size_of_tail::<Src>()),
      Requirements::IsAtLeastThenMultipleOf(n, _) => {
        min_size_of::<Src>() + (n * size_of_tail::<Src>())
      }
      Requirements::IsEmpty | Requirements::None => min_size_of::<Src>(),
    };
    const REQUIREMENTS: Requirements = {
      if !COPY && INFALLIBLE && align_of::<Dst>() > align_of::<Src>() {
        panic!(
          "target type has higher alignment requirement; consider using a `try_*` alternative \
           instead"
        );
      }
      let req = match Requirements::of::<Src, Dst>() {
        Ok(req) => req,
        Err(reason) => panic!("{}", reason),
      };
      // let is_infallible = is_infallible::<Src, Dst>(0, match req {
      //   Requirements::Is(n) | Requirements::IsAtLeast(n) => min_size_of::<Src>() + (n * size_of_tail::<Src>()),
      //   Requirements::IsMultipleOf(m) => min_size_of::<Src>() + (m * size_of_tail::<Src>()),
      //   Requirements::IsAtLeastThenMultipleOf(n, _) => min_size_of::<Src>() + (n * size_of_tail::<Src>()),
      //   Requirements::IsEmpty | Requirements::None => min_size_of::<Src>(),
      // });
      // match is_infallible {
      //   Transmutability::Unreachable => panic!("invalid transmute"),
      //   Transmutability::MaybeFallible if INFALLIBLE || MUTATION => panic!("use try_*"),
      //   _ => {},
      // }
      if INFALLIBLE {
        match req {
          Requirements::Is(_) => panic!(
            "cannot transmute between types of incompatible byte layouts, source too small; \
             consider using a `try_*` alternative instead"
          ),
          Requirements::IsAtLeast(_) => panic!(
            "cannot transmute between types of incompatible byte layouts, target too small; \
             consider using a `try_*` alternative instead"
          ),
          Requirements::IsMultipleOf(_) => panic!(
            "cannot transmute between types of incompatible byte layouts, tail size multiple of \
             mismatch; consider using a `try_*` alternative instead"
          ),
          Requirements::IsAtLeastThenMultipleOf(_, _) => panic!(
            "cannot transmute between types of incompatible byte layouts, target too small or \
             tail size multiple of mismatch; consider using a `try_*` alternative instead"
          ),
          Requirements::IsEmpty => panic!(
            "cannot transmute between types of incompatible byte layouts, source tail might not \
             be empty; consider using a `try_*` alternative instead"
          ),
          Requirements::None => {}
        }
      }
      req
    };

    #[inline]
    #[track_caller]
    pub const fn apply(metadata: usize) -> Result<usize, isize> {
      match Self::REQUIREMENTS.delta(metadata) {
        0 if size_of_tail::<Dst>() == 0 => Ok(0),
        0 => Ok((size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / size_of_tail::<Dst>()),
        n => Err(n * size_of_tail::<Src>() as isize),
      }
    }
  }

  #[inline]
  #[track_caller]
  pub const fn for_infallible_copy<Src: Copy + Data, Dst: Copy + Data>() {
    let requirements = Transmute::<Src, Dst, true, false, true>::REQUIREMENTS;
    assert!(matches!(requirements, Requirements::None));
  }

  #[inline]
  #[track_caller]
  pub const fn for_infallible_ref<Src: ?Sized + Data, Dst: ?Sized + Data>(
    metadata: usize,
  ) -> usize {
    let requirements = Transmute::<Src, Dst, false, false, true>::REQUIREMENTS;
    assert!(matches!(requirements, Requirements::None));
    match size_of_tail::<Dst>() {
      0 => 0,
      n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
    }
  }

  #[inline]
  #[track_caller]
  pub const fn for_infallible_mut<Src: ?Sized + Data, Dst: ?Sized + Data>(
    metadata: usize,
  ) -> usize {
    let requirements = Transmute::<Src, Dst, false, true, true>::REQUIREMENTS;
    assert!(matches!(requirements, Requirements::None));
    match size_of_tail::<Dst>() {
      0 => 0,
      n => (size_of_with::<Src>(metadata) - min_size_of::<Dst>()) / n,
    }
  }

  #[inline]
  #[track_caller]
  pub const fn for_copy<Src: Copy + Data, Dst: Copy + Data>() {
    let requirements = Transmute::<Src, Dst, true, false, false>::REQUIREMENTS;
    assert!(matches!(requirements, Requirements::None));
  }

  #[inline]
  #[track_caller]
  pub const fn for_ref<Src: ?Sized + Data, Dst: ?Sized + Data>(
    metadata: usize,
  ) -> Result<usize, isize> {
    Transmute::<Src, Dst, false, false, false>::apply(metadata)
  }

  #[inline]
  #[track_caller]
  pub const fn for_mut<Src: ?Sized + Data, Dst: ?Sized + Data>(
    metadata: usize,
  ) -> Result<usize, isize> {
    Transmute::<Src, Dst, false, true, false>::apply(metadata)
  }

  // #[cfg(test)]
  // mod tests {
  //   use super::*;
  //
  //   #[test]
  //   fn test_byte_layout_at() {
  //     use MetadataRequirements::*;
  //
  //     #[derive(Debug, Copy, Clone, Data)]
  //     #[repr(C, packed)]
  //     #[data(crate)]
  //     pub struct BazPacked {
  //       a: u8,
  //       b: u32,
  //       c: [u8; 6],
  //     }
  //
  //     #[derive(Debug, Copy, Clone, Data)]
  //     #[repr(C)]
  //     #[data(crate)]
  //     pub struct Baz(u8, u32, [u8; 6]);
  //
  //     assert_eq!(Ok(None), MetadataRequirements::of::<BazPacked, [u8]>());
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between types of incompatible byte layouts, source padding will left \
  //          uninitialized byte(s) in target"
  //       ),
  //       MetadataRequirements::of::<Baz, [u8]>()
  //     );
  //
  //     assert_eq!(Ok(None), MetadataRequirements::of::<u8, bool>());
  //     assert_eq!(Ok(None), MetadataRequirements::of::<bool, u8>());
  //     assert_eq!(Ok(None), MetadataRequirements::of::<u8, bool>());
  //     assert_eq!(
  //       Err("mutating target type could result in unsound source value"),
  //       MetadataRequirements::of::<bool, u8>()
  //     );
  //   }
  //
  //   #[test]
  //   fn test_requirements() {
  //     use MetadataRequirements::*;
  //
  //     #[derive(Data)]
  //     #[repr(C, packed)]
  //     #[data(crate)]
  //     struct Ty<const H: usize, const T: usize = 0>([u8; H], [[u8; T]]);
  //
  //     // Basic tests for equal head and tail sizes
  //     assert_eq!(Ok(None), MetadataRequirements::of::<Ty<1>, Ty<1>>());
  //     assert_eq!(Ok(None), MetadataRequirements::of::<Ty<42>, Ty<42>>());
  //     assert_eq!(Ok(None), MetadataRequirements::of::<Ty<42, 1>, Ty<42, 1>>());
  //
  //     // Test for equal head sizes with different tail sizes
  //     assert_eq!(Ok(None), MetadataRequirements::of::<Ty<42, 3>, Ty<42, 1>>());
  //     assert_eq!(Ok(IsMultipleOf(10)), MetadataRequirements::of::<Ty<50, 10>, Ty<50, 100>>());
  //     assert_eq!(Ok(IsMultipleOf(20)), MetadataRequirements::of::<Ty<50, 5>, Ty<50, 100>>());
  //
  //     // Test cases for transmuting to and from empty tail
  //     assert_eq!(Ok(IsEmpty), MetadataRequirements::of::<Ty<42, 1>, Ty<42>>());
  //     assert_eq!(Ok(Is(7)), MetadataRequirements::of::<Ty<35, 1>, Ty<42>>());
  //     assert_eq!(Ok(None), MetadataRequirements::of::<Ty<2>, Ty<1>>());
  //
  //     // Test cases for large difference in head sizes
  //     assert_eq!(Ok(Is(80)), MetadataRequirements::of::<Ty<20, 1>, Ty<100>>());
  //
  //     // Test cases for tail sizes as multiples
  //     assert_eq!(Ok(IsMultipleOf(2)), MetadataRequirements::of::<Ty<42, 1>, Ty<42, 2>>());
  //     assert_eq!(Ok(IsMultipleOf(3)), MetadataRequirements::of::<Ty<42, 1>, Ty<42, 3>>());
  //     assert_eq!(Ok(IsMultipleOf(3)), MetadataRequirements::of::<Ty<42, 2>, Ty<42, 3>>());
  //     assert_eq!(Ok(IsMultipleOf(3)), MetadataRequirements::of::<Ty<42, 4>, Ty<42, 3>>());
  //     assert_eq!(Ok(IsMultipleOf(8)), MetadataRequirements::of::<Ty<42, 5>, Ty<42, 8>>());
  //     assert_eq!(Ok(IsMultipleOf(3)), MetadataRequirements::of::<Ty<42, 2>, Ty<42, 6>>());
  //
  //     // Test zero heads
  //     assert_eq!(Ok(Is(42)), MetadataRequirements::of::<Ty<0, 1>, Ty<42>>());
  //     assert_eq!(Ok(IsAtLeast(42)), MetadataRequirements::of::<Ty<0, 1>, Ty<42, 1>>());
  //
  //     // Test cases for alignment and complex transmutations
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(2, 3)),
  //       MetadataRequirements::of::<Ty<42, 1>, Ty<38, 3>>()
  //     );
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(1, 3)),
  //       MetadataRequirements::of::<Ty<42, 2>, Ty<38, 3>>()
  //     );
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(1, 3)),
  //       MetadataRequirements::of::<Ty<43, 2>, Ty<39, 3>>()
  //     );
  //     assert_eq!(Ok(IsMultipleOf(2)), MetadataRequirements::of::<Ty<43, 2>, Ty<39, 4>>());
  //     assert_eq!(Ok(IsAtLeast(2)), MetadataRequirements::of::<Ty<1, 2>, Ty<5, 2>>());
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(1, 3)),
  //       MetadataRequirements::of::<Ty<45, 2>, Ty<38, 3>>()
  //     );
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(2, 4)),
  //       MetadataRequirements::of::<Ty<40, 1>, Ty<38, 4>>()
  //     );
  //     assert_eq!(
  //       Ok(IsAtLeastThenMultipleOf(2, 4)),
  //       MetadataRequirements::of::<Ty<44, 5>, Ty<38, 4>>()
  //     );
  //     assert_eq!(Ok(IsAtLeast(1)), MetadataRequirements::of::<Ty<38, 3>, Ty<41, 3>>());
  //
  //     // Panic test cases for incompatible transmutations
  //     assert_eq!(
  //       Err("cannot transmute between types of different sizes, source too small"),
  //       MetadataRequirements::of::<Ty<1>, Ty<2>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between types of incompatible byte layouts, missing bytes to fit the \
  //          target is not a multiple of the source `DST` trailing slice item size"
  //       ),
  //       MetadataRequirements::of::<Ty<35, 2>, Ty<42>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between types of incompatible byte layouts, missing bytes to fit the \
  //          target is not a multiple of the source `DST` trailing slice item size"
  //       ),
  //       MetadataRequirements::of::<Ty<40, 3>, Ty<42>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between types of incompatible byte layouts, remaining bytes to fit in \
  //          the target is not a multiple of the target `DST` trailing slice item size"
  //       ),
  //       MetadataRequirements::of::<Ty<42>, Ty<38, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<42, 3>, Ty<38, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<41, 3>, Ty<39, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<38, 3>, Ty<42, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<39, 3>, Ty<41, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<39, 3>, Ty<41, 6>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<39, 6>, Ty<41, 3>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<39, 6>, Ty<41, 9>>()
  //     );
  //     assert_eq!(
  //       Err(
  //         "cannot transmute between `DST` types of incompatible byte layouts, source cannot \
  //          catchup on target"
  //       ),
  //       MetadataRequirements::of::<Ty<39, 3>, Ty<41, 9>>()
  //     );
  //   }
  // }
}

// num ---------------------------------------------------------------------------------------------

pub mod num {
  use crate::*;
  use core::fmt::{Debug, Display, Formatter};

  macro_rules! impl_num {
    { $($ty:ident $raw:ident $bytes:literal $ordering:ident),+ } => {
      $(impl_num!($ty $raw $bytes { mem::size_of::<$raw>() - $bytes } $ordering);)+
    };
    ($ty:ident $raw:ident $n:literal $rest:tt $e:ident) => {
      #[allow(non_camel_case_types)]
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[repr(transparent)]
      pub struct $ty(pub [u8; $n]);

      unsafe impl RawData for $ty {
        const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
      }

      impl $ty {
        pub const MIN: Self = Self::from_raw($raw::MIN);
        pub const MAX: Self = Self::from_raw($raw::MAX);

        #[inline] pub const fn to_ne_bytes(self) -> [u8; $n] { swap!($e => ne, self).0 }
        #[inline] pub const fn to_le_bytes(self) -> [u8; $n] { swap!($e => le, self).0 }
        #[inline] pub const fn to_be_bytes(self) -> [u8; $n] { swap!($e => be, self).0 }

        #[inline] pub const fn from_ne_bytes(b: [u8; $n]) -> Self { swap!(ne => $e, Self(b)) }
        #[inline] pub const fn from_le_bytes(b: [u8; $n]) -> Self { swap!(le => $e, Self(b)) }
        #[inline] pub const fn from_be_bytes(b: [u8; $n]) -> Self { swap!(be => $e, Self(b)) }

        #[inline]
        pub const fn from_raw(raw: $raw) -> Self {
          let (mut off, mut bytes) = (0, [0; $n]);
          while off < $n {
            bytes[off] = (raw >> (le_or_be!($e, off, $n - off - 1) * 8)) as u8;
            off += 1;
          }
          Self(bytes)
        }

        #[inline]
        pub const fn into_raw(self) -> $raw {
          let (mut off, mut raw) = (0, 0);
          while off < $n {
            raw |= (self.0[off] as $raw) << (le_or_be!($e, off, $n - off - 1) * 8);
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

  #[cfg(test)]
  #[test]
  fn test_num() {
    use core::array::from_fn;

    #[rustfmt::skip]
    macro_rules! test {
      ($sz:literal, [$($ty:ident),+], $min:literal, $max:literal, $val:literal) => {$(
        assert_eq!($sz, mem::size_of::<$ty>());
        assert_eq!($max, $ty::MAX.into());
        assert_eq!($min, $ty::from($min).into());
        assert_eq!($max, $ty::from($max).into());
        assert_eq!($val, $ty::from($val).into());
        assert_eq!(from_fn(|i| i as u8), $ty::from_ne_bytes(from_fn(|i| i as u8)).to_ne_bytes());
        assert_eq!(from_fn(|i| i as u8), $ty::from_le_bytes(from_fn(|i| i as u8)).to_le_bytes());
        assert_eq!(from_fn(|i| i as u8), $ty::from_be_bytes(from_fn(|i| i as u8)).to_be_bytes());
      )+};
    }

    test!(2, [u16, u16le, u16be], 0u16, 0xffffu16, 0x0f1fu16);
    test!(3, [u24, u24le, u24be], 0u32, 0xffffffu32, 0x0f1f2fu32);
    test!(4, [u32, u32le, u32be], 0u32, 0xffffffffu32, 0x0f1f2f3fu32);
    test!(5, [u40, u40le, u40be], 0u64, 0xffffffffffu64, 0x0f1f2f3f4fu64);
    test!(6, [u48, u48le, u48be], 0u64, 0xffffffffffffu64, 0x0f1f2f3f4f5fu64);
    test!(7, [u56, u56le, u56be], 0u64, 0xffffffffffffffu64, 0x0f1f2f3f4f5f6fu64);
    test!(8, [u64, u64le, u64be], 0u64, 0xffffffffffffffffu64, 0x0f1f2f3f4f5f6f7fu64);
  }
}

// internal ----------------------------------------------------------------------------------------

/// Counts the number of values in one range (`of`) that also exist in another range (`into`).
/// Both `of` and `into` are inclusive ranges.
///
/// # Parameters
/// - `of`: A `Range<u64>` representing the first range.
/// - `into`: A `Range<u64>` representing the second range.
///
/// # Returns
/// `u128`: The number of overlapping elements between the two ranges.
///
/// # Examples
/// ```
/// use mutant::overlaps;
///
/// assert_eq!(2, overlaps(0..1, 0..1));
/// assert_eq!(1, overlaps(0..0, 0..1));
/// assert_eq!(1, overlaps(0..1, 0..0));
/// assert_eq!(0, overlaps(5..10, 11..15));
/// assert_eq!(32, overlaps(0..63, 32..63));
/// ```
#[doc(hidden)]
pub const fn overlaps(of: Range<u64>, into: Range<u64>) -> u128 {
  let start = if of.start > into.start { of.start } else { into.start };
  let end = if of.end < into.end { of.end } else { into.end };

  // Check if there is an overlap.
  size(start..end)
}

/// Counts the number of possible values in an inclusive `Range<u64>`.
///
/// # Parameters
/// - `range`: A `Range<u64>` representing the inclusive range.
///
/// # Returns
/// `u128`: The number of possible values within the range.
///
/// # Examples
/// ```
/// use mutant::size;
///
/// assert_eq!(2, size(0..1)); // Range includes 0 and 1
/// assert_eq!(1, size(0..0)); // Range includes only 0
/// assert_eq!(101, size(0..100)); // Range includes numbers 0 through 100
/// assert_eq!(64, size(32..95)); // Range includes numbers 32 through 95
/// ```
#[doc(hidden)]
pub const fn size(range: Range<u64>) -> u128 {
  match range.start > range.end {
    true => 0,
    _ => (range.end - range.start) as u128 + 1,
  }
}

const _: () = {
  use core::fmt;
  use core::fmt::{Debug, Display, Formatter};
  use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};
  use core::ops::{Deref, DerefMut};

  // impl Ordering ---------------------------------------------------------------------------------

  impl Ordering {
    #[cfg(target_endian = "little")]
    const NATIVE: Self = Self::Lsb;
    #[cfg(target_endian = "big")]
    const NATIVE: Self = Self::Msb;

    #[inline]
    pub const fn shift(self, byte: usize, width: usize) -> usize {
      match self {
        Self::Lsb => byte * 8,
        Self::Msb => (width - byte - 1) * 8,
      }
    }
  }

  // impl MetadataChecks ---------------------------------------------------------------------------

  impl MetadataChecks {
    #[inline]
    const fn check(self, metadata: usize) -> isize {
      use MetadataChecks::*;
      match self {
        Is(n) if metadata == n => 0,
        IsAtLeast(n) if metadata >= n => 0,
        Is(n) | IsAtLeast(n) => n as isize - metadata as isize,
        IsMultipleOf(m) if metadata % m == 0 => 0,
        IsMultipleOf(m) => m as isize - (metadata % m) as isize,
        IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => 0,
        IsAtLeastThenMultipleOf(n, _) if metadata < n => n as isize - metadata as isize,
        IsAtLeastThenMultipleOf(_, m) => m as isize - (metadata % m) as isize,
        IsEmpty if metadata == 0 => 0,
        IsEmpty => -(metadata as isize),
        NoneToRemove => 0,
      }
    }

    #[inline]
    #[track_caller]
    const fn assert_infallible(self) {
      use MetadataChecks::*;
      match self {
        Is(_) => concat_panic!(
          "cannot transmute between types of incompatible byte layouts, source too small; \
           consider using a `try_*` alternative instead"
        ),
        IsAtLeast(_) => concat_panic!(
          "cannot transmute between types of incompatible byte layouts, target too small; \
           consider using a `try_*` alternative instead"
        ),
        IsMultipleOf(_) => concat_panic!(
          "cannot transmute between types of incompatible byte layouts, tail size multiple of \
           mismatch; consider using a `try_*` alternative instead"
        ),
        IsAtLeastThenMultipleOf(_, _) => concat_panic!(
          "cannot transmute between types of incompatible byte layouts, target too small or tail \
           size multiple of mismatch; consider using a `try_*` alternative instead"
        ),
        IsEmpty => concat_panic!(
          "cannot transmute between types of incompatible byte layouts, source tail might not be \
           empty; consider using a `try_*` alternative instead"
        ),
        NoneToRemove => {}
      }
    }
  }

  // impl ByteLayout -------------------------------------------------------------------------------

  impl ByteLayout {
    #[inline]
    pub const fn is_uninit(&self) -> bool {
      !matches!(self, Self::Init { .. })
    }

    #[inline]
    pub const fn is_any(&self) -> bool {
      match self {
        ByteLayout::Init { layout, .. } => layout.is_any(),
        ByteLayout::OutOfBounds => false,
        _ => true,
      }
    }

    #[inline]
    pub const fn width(&self) -> usize {
      match self {
        ByteLayout::Init { layout, offset } => layout.width - *offset,
        _ => 1,
      }
    }
  }

  // impl RawLayout --------------------------------------------------------------------------------

  impl RawLayout {
    #[inline]
    pub const fn new<T>(patterns: &'static [Range<u64>]) -> Self {
      assert!(mem::size_of::<T>() <= 8);
      Self { ordering: Ordering::NATIVE, bit_patterns: Some(patterns), width: mem::size_of::<T>() }
    }

    #[inline]
    pub const fn any<T>() -> Self {
      assert!(mem::size_of::<T>() <= 8);
      Self { ordering: Ordering::NATIVE, bit_patterns: None, width: mem::size_of::<T>() }
    }

    #[inline]
    pub const fn with_ordering(self, ordering: Ordering) -> Self {
      Self { ordering, bit_patterns: self.bit_patterns, width: self.width }
    }

    #[inline]
    pub const fn is_any(&self) -> bool {
      self.bit_patterns.is_none()
    }

    #[inline]
    pub const fn offset(&'static self, offset: usize) -> ByteLayout {
      assert!(offset < self.width);
      ByteLayout::Init { layout: self, offset }
    }

    #[inline]
    pub const fn mask(&self, val: u64) -> u64 {
      let mask = ((1u128 << (self.width * 8)) - 1) as u64;
      val & mask
    }

    #[inline]
    pub const fn bit_patterns(&self) -> u128 {
      match self.bit_patterns {
        None => 1u128 << (self.width * 8),
        Some(pats) => {
          let (mut i, mut values) = (0, 0u128);
          while i < pats.len() {
            values += size(pats[i].start..pats[i].end);
            i += 1;
          }
          values
        }
      }
    }

    #[inline]
    pub const fn contains(&self, value: u64) -> bool {
      match self.bit_patterns {
        None => true,
        Some(pats) => {
          let mut i = 0;
          while i < pats.len() {
            if value >= pats[i].start && value <= pats[i].end {
              return true;
            }
            i += 1;
          }
          false
        }
      }
    }

    #[inline]
    pub const fn overlaps(&self, pat: Range<u64>) -> u128 {
      match self.bit_patterns {
        None => size(pat),
        Some(pats) => {
          let (mut i, mut n) = (0, 0);
          while i < pats.len() {
            n += overlaps(pat.start..pat.end, pats[i].start..pats[i].end);
            i += 1;
          }
          n
        }
      }
    }

    const fn bit_pattern_overlaps<Src: ?Sized + Data>(
      &self,
      pat: Range<u64>,
      off: usize,
      byte: usize,
    ) -> u128 {
      if byte == self.width {
        return 0;
      }

      match byte_layout_at::<Src>(off + byte) {
        ByteLayout::Init { layout, .. } => {
          let start = layout.mask(pat.start >> self.ordering.shift(byte, self.width));
          let end = layout.mask(pat.end >> self.ordering.shift(byte, self.width));
          layout.overlaps(start..end)
            + self.bit_pattern_overlaps::<Src>(pat, off, byte + layout.width)
        }
        _ => unreachable!(),
      }
    }

    #[inline]
    #[track_caller]
    const fn static_analyze<Src: ?Sized + Data>(
      &self,
      mut off: usize,
    ) -> Result<(bool, bool), &'static str> {
      use ByteLayout::*;

      let (mut src_bit_patterns, dst_bit_patterns) = (1, self.bit_patterns());
      let (mut src_is_overlapping, mut src_is_any_bit_pattern) = (false, true);
      let end = off + self.width;
      while off < end {
        match byte_layout_at::<Src>(off) {
          Uninit => return Err("uninitialized bytes in source"),
          OutOfBounds => unreachable!(),
          Init { layout, offset } => {
            src_is_overlapping = src_is_overlapping || offset != 0 || off + layout.width > end;
            src_is_any_bit_pattern = src_is_any_bit_pattern && layout.is_any();
            src_bit_patterns *= layout.bit_patterns();
            // Continue with next source layout.
            off += layout.width;
          }
        }
      }
      // Check for target bit patterns, the target is considered as being "any bit pattern" when
      // the [`RawLayout::patterns`] field is set to [`None`].
      let bit_patterns = match self.bit_patterns {
        // Target layout is "any bit pattern", returns:
        // * `false`: The target layout is "any bit pattern", hence [`Error::InvalidBitPattern`]
        //   is unreachable.
        // * `src_is_overlapping`: Target mutation is only safe and sound if all source layouts are
        //   "any bit pattern".
        None => return Ok((false, src_is_any_bit_pattern)),
        // Target layout isn't "any bit pattern" and source layouts overlaps with target, returns:
        // * `true`: In case of layout overlaps prefer to return a maybe false-positive result
        //   instead of performing complex computations. This path could still be improved later on.
        //   ie. consider [`Error::InvalidBitPattern`] as "maybe" reachable.
        // * `src_is_overlapping`: Target mutation is only safe and sound if all source layouts are
        //   "any bit pattern".
        Some(_) if src_is_overlapping => return Ok((true, src_is_any_bit_pattern)),
        // Target layout isn't "any bit pattern", continue computation.
        Some(bit_patterns) => bit_patterns,
      };
      // Compute the number of valid bit patterns in source that are also valid in target.
      let (mut i, mut bit_pattern_overlaps) = (0, 0);
      while i < bit_patterns.len() {
        let bit_pattern = bit_patterns[i].start..bit_patterns[i].end;
        bit_pattern_overlaps += self.bit_pattern_overlaps::<Src>(bit_pattern, end - self.width, 0);
        i += 1;
      }
      // Check valid bit pattern overlaps.
      if src_bit_patterns == bit_pattern_overlaps {
        // All possible bit patterns in source overlaps with valid bit patterns in target, returns:
        // * `false`: All bit patterns are valid, hence [`Error::InvalidBitPattern`] is unreachable.
        // * `dst_bit_patterns == src_bit_patterns`: Target mutation is only safe and sound if all
        //   possible bit patterns in target are also valid in source.
        Ok((false, dst_bit_patterns == src_bit_patterns))
      } else if bit_pattern_overlaps == 0 {
        // There is no valid bit pattern overlaps, abort.
        Err("none of the bit patterns in source are valid in target")
      } else {
        // Some, but not all, possible bit patterns in source overlaps with valid bit patterns
        // in target, returns:
        // * `true`: Not all bit patterns are valid, hence [`Error::InvalidBitPattern`] is
        //   reachable.
        // * `dst_bit_patterns == bit_pattern_overlaps`: Target mutation is only safe and sound if
        //   all possible bit patterns in target are also valid in source.
        Ok((true, dst_bit_patterns == bit_pattern_overlaps))
      }
    }
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

  unsafe impl Data for () {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Void;
    const HEADER: usize = 0;
  }

  unsafe impl<T: ?Sized + 'static> Data for PhantomData<T> {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Void;
    const HEADER: usize = 0;
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

  unsafe impl<T: RawData> Data for T {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Raw { layout: T::RAW_LAYOUT };
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

  // impl RawData ----------------------------------------------------------------------------------

  unsafe impl RawData for bool {
    const RAW_LAYOUT: RawLayout = RawLayout::new::<Self>(&[0..1]);
  }

  unsafe impl RawData for u8 {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for u16 {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for u32 {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for u64 {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for usize {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for Option<NonZeroU8> {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for Option<NonZeroU16> {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for Option<NonZeroU32> {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for Option<NonZeroU64> {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }

  unsafe impl RawData for Option<NonZeroUsize> {
    const RAW_LAYOUT: RawLayout = RawLayout::any::<Self>();
  }
};

// tests -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use crate::*;

  #[derive(Copy, Clone, Data)]
  #[repr(C)]
  #[data(crate)]
  pub struct Foo<T: ?Sized> {
    a: u8,
    b: u32,
    c: Dropless<T>,
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  #[data(crate)]
  pub struct FooPacked<T: ?Sized> {
    a: u8,
    b: Unalign<num::u16le>,
    c: [u8; 6],
    d: Dropless<T>,
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq, Data)]
  #[repr(C)]
  #[data(crate)]
  pub struct Bar<T: ?Sized> {
    a: [u8; 4],
    b: T,
  }

  #[test]
  fn test_byte_layout_at() {
    assert_eq!(Ok(&true), try_transmute_ref::<u8, bool>(&1u8));
    assert_eq!(Ok(&1u8), try_transmute_ref::<bool, u8>(&true));
    // assert_eq!(Ok(&mut true), try_transmute_mut::<u8, bool>(&mut 1u8));
  }

  #[test]
  fn test_transmute() {
    let mut foo = FooPacked::<[u8; 3]> {
      a: 0x42,
      b: Unalign::new(num::u16le::from_le_bytes([0x42, 0x42])),
      c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
      d: Dropless::new([0xaf, 0xaf, 0xaa]),
    };

    let bar: &mut Bar<[u8]> = transmute_mut(&mut foo);

    assert_eq!([0x42; 4], bar.a);
    bar.a = [0xff; 4];

    assert_eq!(0xff, foo.a);
    assert_eq!(num::u16le::from_le_bytes([0xff, 0xff]), foo.b.into_inner());
    assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

    pub fn pack<T: ?Sized + Data>(value: &T) -> &[u8] {
      transmute_ref(value)
    }

    pub fn pack_foo(foo: &FooPacked<[u8; 3]>) -> &[u8] {
      pack(foo)
    }

    println!("{:?}", pack_foo(&foo));

    let foo = Foo::<[u8; 3]> { a: 0x42, b: 0x42, c: Dropless::new([0xaf, 0xaf, 0xaa]) };

    let _: &Foo<[u8; 3]> = transmute_ref(&foo);
    let _: Result<&Foo<[u8; 3]>, Error> = try_transmute_ref(&foo);
  }

  #[test]
  fn test_try_transmute() {
    assert_eq!(Ok(&[1, 2]), try_transmute_ref::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(Err(Error::SizeMismatch(1)), try_transmute_ref::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(
      Ok(&Bar { a: [1, 2, 3, 4], b: () }),
      try_transmute_ref::<[u8], Bar<()>>(&[1, 2, 3, 4])
    );
    println!("{:?}", try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    println!(
      "{:?}",
      try_transmute_ref::<Bar<[u8]>, Bar<[u8; 2]>>(
        try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
      )
    );
    println!("{:?}", transmute_ref::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
  }

  #[test]
  fn test_split_at() {
    assert_eq!(([1u8, 2].as_slice(), [3u8, 4].as_slice()), split_at([1u8, 2, 3, 4].as_slice(), 2));

    #[derive(Data)]
    #[repr(C, packed)]
    #[data(crate)]
    struct Foo {
      bar: u8,
      qux_len: Unalign<u16>,
      qux: [u8],
    }

    let foo: &Foo = try_transmute_ref::<[u8], _>(&[1, 4, 0, 4, 5, 6, 7, 8, 9]).unwrap();
    let (foo, rest) = split_at(foo, (3 + foo.qux_len.into_inner()) as usize);
    let _ = (foo, rest);
  }
}

#[derive(Data)]
#[data(crate)]
#[repr(C, packed)]
pub struct Test {
  a: u8,
  b: u32,
  c: [bool; 4],
  d: [(u8, bool)],
}

#[derive(Data)]
#[data(crate)]
#[repr(C, packed)]
pub struct Test2 {
  a: u8,
  b: u32,
  c: [u8; 4],
  d: [(u8, u8)],
}

pub fn aa(val: &Test) -> Result<&[u8], Error> {
  try_transmute_ref(val)
}

pub fn bb(val: &[u8]) -> Result<&Test, Error> {
  try_transmute_ref(val)
}

pub fn cc(val: &Test) -> &[u8] {
  transmute_ref(val)
}

pub fn dd(val: &Test2) -> Result<&[u8], Error> {
  try_transmute_ref(val)
}

pub fn ee(val: &[u8]) -> Result<&Test2, Error> {
  try_transmute_ref(val)
}

pub fn ff(val: &Test2) -> &[u8] {
  transmute_ref(val)
}

pub fn hh(u: num::u24le) -> u32 {
  u.into_raw()
}

pub fn ii(u: u32) -> u32 {
  u.swap_bytes()
}

#[no_mangle]
pub fn jj(u: u32) -> num::u24le {
  u.into()
}
