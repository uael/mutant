use core::fmt;
use core::fmt::{Debug, Display, Formatter};
use core::ops::{Deref, DerefMut};

use crate::{DataType, Field, field, Layout, Repr};

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

#[macro_export]
macro_rules! unaligned_ref {
  (&$($expr:tt).+) => {{
    const fn assert_unaligned<T: ?Sized + DataType>(_: *const T) {
      assert!($crate::align_of::<T>() == 1);
    }

    let _ref: &_ = $crate::unaligned_ref!(__impl@base_of $($expr).+);
    let addr = core::ptr::addr_of!($($expr).+);
    assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
  (&mut $($expr:tt).+) => {{
    const fn assert_unaligned<T: ?Sized + DataType>(_: *mut T) {
      assert!($crate::align_of::<T>() == 1);
    }

    let _ref: &mut _ = $crate::unaligned_ref!(__impl@base_of $($expr).+);
    let addr = core::ptr::addr_of_mut!($($expr).+);
    assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
  (__impl@base_of $base:tt $(.$ex:tt)+) => { $base };
}

impl<T: Copy + DataType> DataType for Unalign<T> {
  type Item = ();
  type Head = T;
  type Tail = T::Tail;
  type Fields = Field<Self, T>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T>(0, None, None) }) };
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

impl<T: Copy + DataType> Deref for Unalign<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unaligned_ref!(&self.0)
  }
}

impl<T: Copy + DataType> DerefMut for Unalign<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unaligned_ref!(&mut self.0)
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
