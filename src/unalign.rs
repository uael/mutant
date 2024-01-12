use core::fmt;
use core::fmt::{Debug, Display, Formatter};
use core::ops::{Deref, DerefMut};

use crate::{align_of, field, Data, Field, Layout, Repr};

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

#[doc(hidden)]
pub const fn assert_unaligned<T: ?Sized + Data>(_: *const T) {
  struct Assert<T: ?Sized>(*const T);

  impl<T: ?Sized + Data> Assert<T> {
    const ASSERT_UNALIGNED: () = assert!(align_of::<T>() == 1);
  }

  let _ = Assert::<T>::ASSERT_UNALIGNED;
}

#[macro_export]
macro_rules! unaligned_ref {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_ref!($base.$field);
    $crate::unaligned_ref!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &_ = $base;
    let addr = core::ptr::addr_of!($base.$field);
    $crate::assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
}

#[macro_export]
macro_rules! unaligned_mut {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_mut!($base.$field);
    $crate::unaligned_mut!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &mut _ = $base;
    let addr = core::ptr::addr_of_mut!($base.$field);
    $crate::assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
}

unsafe impl<T: Copy + Data> Data for Unalign<T> {
  type Item = ();
  type Fields = Field<Self, T>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T>(0) }) };
  const HEADER: usize = T::HEADER;
}

impl<T: Copy> Unalign<T> {
  #[inline(always)]
  pub const fn new(value: T) -> Self {
    Self(value)
  }

  #[inline(always)]
  pub const fn into_inner(self) -> T {
    self.0
  }
}

impl<T: Copy + Data> Deref for Unalign<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unaligned_ref!(self.0)
  }
}

impl<T: Copy + Data> DerefMut for Unalign<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unaligned_mut!(self.0)
  }
}

impl<T: Display + Copy> Display for Unalign<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(&self.into_inner(), f)
  }
}

impl<T: Debug + Copy> Debug for Unalign<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Debug::fmt(&self.into_inner(), f)
  }
}
