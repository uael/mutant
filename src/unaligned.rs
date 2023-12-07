use crate::Unalign;
use crate::Undrop;

use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::{NonZeroI8, NonZeroU8};

/// Types with no alignment requirement.
///
/// WARNING: Do not implement this trait yourself! Instead, use
/// `#[derive(Unaligned)]` (requires the `derive` Cargo feature).
///
/// If `T: Unaligned`, then `align_of::<T>() == 1`.
///
/// # Safety
///
/// *This section describes what is required in order for `T: Unaligned`, and
/// what unsafe code may assume of such types. `#[derive(Unaligned)]` only
/// permits types which satisfy these requirements. If you don't plan on
/// implementing `Unaligned` manually, and you don't plan on writing unsafe code
/// that operates on `Unaligned` types, then you don't need to read this
/// section.*
///
/// If `T: Unaligned`, then unsafe code may assume that it is sound to produce a
/// reference to `T` at any memory location regardless of alignment. If a type
/// is marked as `Unaligned` which violates this contract, it may cause
/// undefined behavior.
pub unsafe trait Unaligned {}

#[macro_export]
macro_rules! read_aligned {
  ($unaligned:expr $(,)?) => {{
    const fn assert_unaligned<T: ?Sized + Unaligned>(_: *const T) { }
    let addr = core::ptr::addr_of!($unaligned);
    assert_unaligned(addr);
    unsafe { &*addr }
  }};
}

#[macro_export]
macro_rules! read_aligned_mut {
  ($unaligned:expr $(,)?) => {{
    const fn assert_unaligned<T: ?Sized + Unaligned>(_: *const T) { }
    let addr = core::ptr::addr_of_mut!($unaligned);
    assert_unaligned(addr);
    unsafe { &mut *addr }
  }};
}

unsafe impl Unaligned for Option<NonZeroI8> {}

unsafe impl Unaligned for Option<NonZeroU8> {}

unsafe impl Unaligned for bool {}

unsafe impl Unaligned for i8 {}

unsafe impl Unaligned for str {}

unsafe impl Unaligned for u8 {}

unsafe impl Unaligned for () {}

unsafe impl Unaligned for NonZeroI8 {}

unsafe impl Unaligned for NonZeroU8 {}

unsafe impl<T: Unaligned> Unaligned for [T] {}

unsafe impl<T: Copy> Unaligned for Unalign<T> {}

unsafe impl<T: ?Sized + Unaligned> Unaligned for Undrop<T> {}

unsafe impl<T: Unaligned> Unaligned for MaybeUninit<T> {}

unsafe impl<T: ?Sized + Unaligned> Unaligned for ManuallyDrop<T> {}

unsafe impl<T: ?Sized> Unaligned for PhantomData<T> {}

unsafe impl<const N: usize, T: Unaligned> Unaligned for [T; N] {}
