//! Raw pointer helpers that work in `const fn` for both thin and wide (slice-tailed) pointers.
//!
//! Stable Rust has no way to split a `*const T` into an address and a slice length, or to build
//! one back, when `T` is a generic `?Sized` type: `core::ptr::metadata` is unstable and `as` casts
//! between wide pointers are only accepted for concrete types.
//!
//! This module does it by reinterpreting `*const T` as `*const [u8]`, which assumes that every
//! pointer to a slice-tailed type is represented like a pointer to a slice. This is how the
//! compiler works, but it is not a documented guarantee, hence the assumption is **checked at
//! compile time for every unsized type** by [`verify`], against a pointer built with a genuine
//! `as` cast (`Data::PROBE`). If the representation ever changes, the crate fails to build instead
//! of misbehaving.

use crate::Data;

/// Slice length of `Data::PROBE` pointers.
#[doc(hidden)]
pub const PROBE_LEN: usize = 3;

union Cast<A: Copy, B: Copy> {
  from: A,
  to: B,
}

/// Slice length of a wide pointer.
///
/// # Safety
/// `T` must be a slice, or a struct with a slice tail.
#[inline(always)]
pub(crate) const unsafe fn raw_len<T: ?Sized>(ptr: *const T) -> usize {
  // SAFETY: see the module documentation.
  unsafe { Cast::<*const T, *const [u8]> { from: ptr }.to }.len()
}

/// Same pointer as `ptr`, with another slice length.
///
/// # Safety
/// `T` must be a slice, or a struct with a slice tail.
#[inline(always)]
pub(crate) const unsafe fn raw_with_len<T: ?Sized>(ptr: *const T, len: usize) -> *const T {
  let slice = core::ptr::slice_from_raw_parts(ptr.cast::<u8>(), len);
  // SAFETY: see the module documentation.
  unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
}

/// Check, at compile time, that pointers to `T` are represented like pointers to slices.
///
/// # Safety
/// `T` must be a slice, or a struct with a slice tail, and `probe` must come from
/// `core::ptr::slice_from_raw_parts(core::ptr::null(), PROBE_LEN)` through `as` casts only.
pub(crate) const unsafe fn verify<T: ?Sized>(probe: *const T) {
  const MESSAGE: &str = "unsupported compiler: unexpected wide pointer representation";
  assert!(size_of::<*const T>() == size_of::<*const [u8]>(), "{}", MESSAGE);
  assert!(align_of::<*const T>() == align_of::<*const [u8]>(), "{}", MESSAGE);
  // SAFETY: guaranteed by the caller.
  unsafe {
    assert!(raw_len(probe) == PROBE_LEN, "{}", MESSAGE);
    let other = raw_with_len(probe, PROBE_LEN + 39);
    assert!(raw_len(other) == PROBE_LEN + 39, "{}", MESSAGE);
    // `other` and `probe` must have the same (null) address.
    assert!(other.cast::<u8>().is_null() && probe.cast::<u8>().is_null(), "{}", MESSAGE);
  }
}

/// Slice length of `ptr`, `0` if `T` is `Sized`.
#[inline(always)]
pub(crate) const fn metadata<T: ?Sized + Data>(ptr: *const T) -> usize {
  if const { !T::LAYOUT.unsized_ } {
    return 0;
  }
  // SAFETY: `T` is unsized, and every unsized `Data` type has a slice tail.
  unsafe { raw_len(ptr) }
}

/// Pointer to a `T` at `addr`, whose tail slice has `len` elements (ignored if `T` is `Sized`).
#[inline(always)]
pub(crate) const fn from_raw_parts<T: ?Sized + Data>(addr: *const u8, len: usize) -> *const T {
  if const { T::LAYOUT.unsized_ } {
    let slice = core::ptr::slice_from_raw_parts(addr, len);
    // SAFETY: `T` is unsized, see the module documentation.
    unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
  } else {
    // SAFETY: `T` is `Sized`: both are thin pointers.
    unsafe { Cast::<*const u8, *const T> { from: addr }.to }
  }
}

/// Pointer to the field of type `F` at `offset` in the `T` at `ptr`.
///
/// # Safety
/// `ptr` must point to an allocation that holds a `T`, and `T` must have a field of type `F` at
/// `offset`.
#[inline(always)]
pub(crate) const unsafe fn field<T: ?Sized + Data, F: ?Sized + Data>(
  ptr: *const T, offset: usize,
) -> *const F {
  // An unsized field is the last one: it shares the slice length of its parent.
  let len = if const { F::LAYOUT.unsized_ } { metadata(ptr) } else { 0 };
  // SAFETY: the field is in bounds of the allocation.
  from_raw_parts(unsafe { ptr.cast::<u8>().add(offset) }, len)
}

/// Whatever `addr` is a multiple of `align`.
///
/// This cannot be evaluated at compile time, where pointers have no address: doing so is a
/// compilation error.
#[inline(always)]
pub(crate) const fn is_aligned(addr: *const u8, align: usize) -> bool {
  // SAFETY: a pointer can be read as an integer at run time (this does not expose its
  // provenance, which is fine as the integer is never turned back into a pointer).
  let addr = unsafe { Cast::<*const u8, usize> { from: addr }.to };
  addr & (align - 1) == 0
}
