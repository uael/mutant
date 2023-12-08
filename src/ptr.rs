//! TODO: Use [`ptr_metadata`](1) feature when stabilized
//! Imported from [`rustc/metadata.rs`](2) with some minor non-breaking modifications.
//!
//! [1]: https://github.com/rust-lang/rust/issues/81513
//! [2]: https://github.com/rust-lang/rust/blob/master/library/core/src/ptr/metadata.rs

use crate::Mutant as Pointee;

/// Forms a (possibly-wide) raw pointer from a data address and metadata.
///
/// This function is safe but the returned pointer is not necessarily safe to dereference.
/// For slices, see the documentation of [`slice::from_raw_parts`] for safety requirements.
/// For trait objects, the metadata must come from a pointer to the same underlying erased type.
///
/// [`slice::from_raw_parts`]: crate::slice::from_raw_parts
#[inline]
pub(crate) const fn from_raw_parts<T: ?Sized + Pointee>(
  data_address: *const (),
  metadata: <T as Pointee>::Metadata,
) -> *const T {
  // SAFETY: Accessing the value from the `PtrRepr` union is safe since *const T
  // and PtrComponents<T> have the same memory layouts. Only std can make this
  // guarantee.
  unsafe { PtrRepr { components: PtrComponents { data_address, metadata } }.const_ptr }
}

#[repr(C)]
union PtrRepr<T: ?Sized + Pointee> {
  const_ptr: *const T,
  mut_ptr: *mut T,
  components: PtrComponents<T>,
}

#[repr(C)]
struct PtrComponents<T: ?Sized + Pointee> {
  data_address: *const (),
  metadata: <T as Pointee>::Metadata,
}

// Manual impl needed to avoid `T: Copy` bound.
impl<T: ?Sized + Pointee> Copy for PtrComponents<T> {}

// Manual impl needed to avoid `T: Clone` bound.
impl<T: ?Sized + Pointee> Clone for PtrComponents<T> {
  fn clone(&self) -> Self {
    *self
  }
}
