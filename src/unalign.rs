use crate::Mutant;

use core::fmt::{self, Debug, Display, Formatter};

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

impl<T: Mutant + Copy> Mutant for Unalign<T> {
  type Metadata = T::Metadata;

  type Head = Unalign<T::Head>;
  type Tail = T::Tail;

  #[inline]
  fn validate(head: &Self::Head, tail: &Self::Tail) -> bool {
    T::validate(&head.into_inner(), tail)
  }

  #[inline]
  fn trailing(head: &Self::Head, tail: &[u8]) -> usize {
    T::trailing(&head.into_inner(), tail)
  }

  #[inline]
  unsafe fn metadata(size: usize) -> Option<Self::Metadata> {
    T::metadata(size)
  }
}

impl<T: Copy> Unalign<T> {
  #[inline(always)]
  pub fn new(value: T) -> Self {
    Self(value)
  }

  #[inline(always)]
  pub fn into_inner(self) -> T {
    self.0
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
