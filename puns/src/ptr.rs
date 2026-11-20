//! Raw pointers to sized and to slice-tailed types, split and made in `const fn`.
//!
//! Stable Rust cannot split a `*const T` into an address and a length, or make one from them,
//! when `T` is a generic `?Sized` type. `core::ptr::metadata` is unstable, and `as` casts between
//! wide pointers compile only for concrete types.
//!
//! This module reinterprets a `*const T` as a `*const [u8]`. This assumes that a pointer to a
//! slice-tailed type is represented as a pointer to a slice, whose length is the length of the
//! tail. The compiler does so, but Rust does not guarantee it. So the layout of each unsized type
//! is made with [`verify`], which checks the assumption at compile time against a pointer made
//! with `as` casts (`Pun::PROBE`). If the representation changes, the crate fails to build
//! instead of misbehaving.
//!
//! # Miri
//!
//! CI runs the tests under Miri with `-Zmiri-strict-provenance`, with tree borrows on x86-64, and
//! with stacked borrows on `i686-unknown-linux-gnu` and `s390x-unknown-linux-gnu`. The i686 target
//! has a 32-bit `usize` and a `u64` aligned to 4. The s390x target is big-endian. The selection of
//! the model check for Miri runs there too, with tree borrows.
//!
//! `-Zmiri-symbolic-alignment-check` reports a misaligned reference where `try_pun_ref` checked
//! the address of bytes that happened to be aligned. The check reads the address as an integer
//! ([`is_aligned`]), which the symbolic alignment of Miri cannot follow. The stable `align_offset`
//! is not a `const fn`, so it cannot replace that check. CI does not use that flag.

use crate::Pun;

/// The length of the slice tail of `Pun::PROBE`, for an unsized type.
#[doc(hidden)]
pub const PROBE_LEN: usize = 3;

union Cast<A: Copy, B: Copy> {
  from: A,
  to: B,
}

/// Returns the length of the slice tail of `ptr`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
#[inline(always)]
pub const unsafe fn raw_len<T: ?Sized>(ptr: *const T) -> usize {
  // SAFETY: see the module documentation.
  unsafe { Cast::<*const T, *const [u8]> { from: ptr }.to }.len()
}

/// Returns a pointer to the address of `ptr`, whose slice tail has `len` elements.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
#[inline(always)]
pub const unsafe fn raw_with_len<T: ?Sized>(ptr: *const T, len: usize) -> *const T {
  let slice = core::ptr::slice_from_raw_parts(ptr.cast::<u8>(), len);
  // SAFETY: see the module documentation.
  unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
}

/// Asserts that pointers to `T` are represented as pointers to slices.
///
/// It compares `probe` with what [`raw_len`] reads of it and with what [`raw_with_len`] makes of
/// it: the size and the alignment of the pointer, the length of the tail, and the address.
///
/// # Panics
///
/// Panics if the representation differs. In const evaluation, the panic is a compile error.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
/// - `probe` is `core::ptr::slice_from_raw_parts(core::ptr::null(), PROBE_LEN)`, cast to
///   `*const T` with `as` casts only.
pub const unsafe fn verify<T: ?Sized>(probe: *const T) {
  const MESSAGE: &str = "unsupported compiler: unexpected wide pointer representation";
  assert!(size_of::<*const T>() == size_of::<*const [u8]>(), "{}", MESSAGE);
  assert!(align_of::<*const T>() == align_of::<*const [u8]>(), "{}", MESSAGE);
  // SAFETY: By contract on the caller, `T` is a slice, `str`, or a struct whose last field is one
  // of these. This is what `raw_len` and `raw_with_len` require.
  unsafe {
    assert!(raw_len(probe) == PROBE_LEN, "{}", MESSAGE);
    let other = raw_with_len(probe, PROBE_LEN + 39);
    assert!(raw_len(other) == PROBE_LEN + 39, "{}", MESSAGE);
    // `other` and `probe` must have the same (null) address.
    assert!(other.cast::<u8>().is_null() && probe.cast::<u8>().is_null(), "{}", MESSAGE);
  }
}

/// Returns the length of the slice tail of `ptr`, or 0 if `T` is `Sized`.
#[inline(always)]
pub const fn metadata<T: ?Sized + Pun>(ptr: *const T) -> usize {
  if const { !T::LAYOUT.unsized_ } {
    return 0;
  }
  // SAFETY: `T` is a slice, `str`, or a struct whose last field is one of these, as `raw_len`
  // requires. `T::LAYOUT.unsized_` is `true`. Only `Layout::slice`, `Layout::pattern` and
  // `Layout::structure` of a struct whose last field is unsized make such a layout, and each
  // calls `verify` on the probe of the type. By the `# Safety` of `Pun`, `T::LAYOUT` is the
  // layout of `T`. `ManuallyDrop<T>` reuses the layout of `T`, and is a struct whose only field
  // is a `T`.
  unsafe { raw_len(ptr) }
}

/// Returns a pointer to a `T` at `addr`, whose slice tail has `len` elements.
///
/// `len` is ignored if `T` is `Sized`.
#[inline(always)]
pub const fn from_raw_parts<T: ?Sized + Pun>(addr: *const u8, len: usize) -> *const T {
  if const { T::LAYOUT.unsized_ } {
    let slice = core::ptr::slice_from_raw_parts(addr, len);
    // SAFETY: `T` is unsized, see the module documentation.
    unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
  } else {
    // SAFETY: `T` is `Sized`: both are thin pointers.
    unsafe { Cast::<*const u8, *const T> { from: addr }.to }
  }
}

/// Returns a pointer to the field of type `F` at `offset` in the `T` at `ptr`.
///
/// An unsized field is the last field of `T`, so its tail has the length of the tail of `ptr`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` points to an allocation that holds a `T`.
/// - `T` has a field of type `F` at `offset`.
#[inline(always)]
pub const unsafe fn field<T: ?Sized + Pun, F: ?Sized + Pun>(
  ptr: *const T,
  offset: usize,
) -> *const F {
  // An unsized field is the last one: it shares the slice length of its parent.
  let len = if const { F::LAYOUT.unsized_ } { metadata(ptr) } else { 0 };
  // SAFETY: This satisfies the safety contract of `add` [1]. By contract on the caller, `ptr`
  // points to an allocation that holds a `T`, and `T` has a field at `offset`. So `offset` is at
  // most the size of that `T`, and `ptr..ptr + offset` is in bounds of the allocation.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.add:
  //
  //     If the computed offset is non-zero, then `self` must be derived from a pointer to some
  //     allocation, and the entire memory range between `self` and `result` [...] must be in
  //     bounds of that allocation.
  //
  //     Allocations can never be larger than `isize::MAX` bytes [...], so technically the last
  //     condition implies the first two.
  from_raw_parts(unsafe { ptr.cast::<u8>().add(offset) }, len)
}

/// Returns whether `addr` is a multiple of `align`.
///
/// `align` must be a power of two, or the result is meaningless. The address is read as an
/// integer, which const evaluation cannot do: a call evaluated at compile time does not compile.
#[inline(always)]
pub const fn is_aligned(addr: *const u8, align: usize) -> bool {
  // SAFETY: a pointer can be read as an integer at run time (this does not expose its
  // provenance, which is fine as the integer is never turned back into a pointer).
  let addr = unsafe { Cast::<*const u8, usize> { from: addr }.to };
  addr & (align - 1) == 0
}
