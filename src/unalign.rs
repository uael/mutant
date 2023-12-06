use crate::Mutant;
use crate::Tail;
use core::fmt::{self, Debug, Display, Formatter};

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Unalign<T>(pub T);

unsafe impl<T: Mutant> Mutant for Unalign<T> {
  type Error = T::Error;
  type Head = T::Head;
  type Tail = T::Tail;

  #[inline]
  fn validate(bytes: &T::Head) -> Result<<T::Tail as Tail>::Size, T::Error> {
    T::validate(bytes)
  }
}

// unsafe impl<T: Varying + Copy> Varying for Unalign<T> {
//   fn size_initialized(&self) -> usize {
//     { self.0 }.size_initialized()
//   }
// }

impl<T> Unalign<T> {
  #[inline(always)]
  pub fn new(value: T) -> Self {
    Self(value)
  }

  #[inline(always)]
  pub fn into_inner(self) -> T {
    self.0
  }
}

impl<T: Copy + Display> Display for Unalign<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(&self.into_inner(), f)
  }
}

impl<T: Copy + Debug> Debug for Unalign<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Debug::fmt(&self.into_inner(), f)
  }
}
