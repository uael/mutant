use core::fmt;
use core::fmt::{Debug, Display, Formatter};
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

use crate::{DataType, Field, field, Layout, Repr};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Undrop<T: ?Sized>(ManuallyDrop<T>);

impl<T: ?Sized + DataType> DataType for Undrop<T> {
  type Item = ();
  type Head = T::Head;
  type Tail = T::Tail;
  type Fields = Field<Self, T::Head>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T::Head>(0, None, None) }) };
}

impl<T: ?Sized> Undrop<T> {
  const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
}

impl<T> Undrop<T> {
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

impl<T: ?Sized> Deref for Undrop<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    self.0.deref()
  }
}

impl<T: ?Sized> DerefMut for Undrop<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.0.deref_mut()
  }
}

impl<T: Display + ?Sized> Display for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Self::ASSERT_NOT_DROP;
    Display::fmt(unsafe { mem::transmute::<_, &T>(self) }, f)
  }
}

impl<T: Debug + ?Sized> Debug for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Self::ASSERT_NOT_DROP;
    Debug::fmt(unsafe { mem::transmute::<_, &T>(self) }, f)
  }
}
