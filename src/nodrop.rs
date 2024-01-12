use core::fmt;
use core::fmt::{Debug, Display, Formatter};
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

use crate::{field, Data, Field, Layout, Repr};

#[repr(transparent)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct NoDrop<T: ?Sized>(ManuallyDrop<T>);

unsafe impl<T: ?Sized + Data> Data for NoDrop<T> {
  type Item = ();
  type Fields = Field<Self, T>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T>(0) }) };
  const HEADER: usize = T::HEADER;
}

impl<T: ?Sized> NoDrop<T> {
  const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
}

impl<T> NoDrop<T> {
  #[inline(always)]
  pub const fn new(value: T) -> Self {
    let _ = Self::ASSERT_NOT_DROP;
    Self(ManuallyDrop::new(value))
  }

  #[inline(always)]
  pub const fn into_inner(self) -> T {
    let _ = Self::ASSERT_NOT_DROP;
    ManuallyDrop::into_inner(self.0)
  }
}

impl<T: ?Sized> Deref for NoDrop<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    self.0.deref()
  }
}

impl<T: ?Sized> DerefMut for NoDrop<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.0.deref_mut()
  }
}

impl<T: Display + ?Sized> Display for NoDrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(self.deref(), f)
  }
}

impl<T: Debug + ?Sized> Debug for NoDrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Debug::fmt(self.deref(), f)
  }
}
