//! Work in progress: see `DESIGN.md`.
#![cfg_attr(not(test), no_std)]

mod analysis;
pub mod layout;
mod ptr;
#[cfg(test)]
mod tests;

use layout::{Layout, Spec};

/// Leaf types: integers and the like, see [`Spec`].
///
/// # Safety
/// `SPEC` must describe every valid value of `Self`, and nothing more.
pub unsafe trait Scalar: Copy {
  const SPEC: Spec;
}

/// Types made of plain data, whose layout and valid values are known at compile time.
///
/// # Safety
/// Not meant to be implemented by hand: use `#[derive(Data)]`.
pub unsafe trait Data {
  /// A null pointer to `Self`, whose slice length is [`ptr::PROBE_LEN`] if `Self` is unsized.
  /// It must be made with `as` casts only: this is what the pointer tricks are checked against.
  #[doc(hidden)]
  const PROBE: *const Self;
  #[doc(hidden)]
  const LAYOUT: Layout;
}

unsafe impl<T: Scalar> Data for T {
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::scalar::<T>(T::SPEC);
}

unsafe impl<T: Data, const N: usize> Data for [T; N] {
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::array(&T::LAYOUT, N);
}

unsafe impl<T: Data> Data for [T] {
  const PROBE: *const Self = core::ptr::slice_from_raw_parts(core::ptr::null(), ptr::PROBE_LEN);
  const LAYOUT: Layout = Layout::slice::<T>(Self::PROBE, &T::LAYOUT);
}

macro_rules! impl_any {
  ($($ty:ty)+) => {$(
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>();
    }
  )+};
}

impl_any!(u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64);

unsafe impl Scalar for bool {
  const SPEC: Spec = Spec::ranges::<bool>(layout::Endian::NATIVE, &[(0, 1)]);
}

unsafe impl Scalar for char {
  const SPEC: Spec =
    Spec::ranges::<char>(layout::Endian::NATIVE, &[(0, 0xD7FF), (0xE000, 0x10FFFF)]);
}

macro_rules! impl_non_zero {
  ($($ty:ident)+) => {$(
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(layout::Endian::NATIVE, &[(1, $ty::MAX as u128)]);
    }
  )+};
}

impl_non_zero!(u8 u16 u32 u64 u128 usize);
