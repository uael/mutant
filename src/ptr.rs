use crate::Default;
use crate::num::Numeric;

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
      Self::with_components(address, <T::Metadata as Default>::DEFAULT)
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
