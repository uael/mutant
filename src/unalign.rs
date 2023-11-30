use crate::{tests::AnyBitPattern, tests::Packed, tests::Zeroable};
use core::fmt::{self, Debug, Display, Formatter};

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Unalign<T: Copy>(T);

unsafe impl<T: Copy + 'static> Packed for Unalign<T> {}
unsafe impl<T: Copy + Zeroable> Zeroable for Unalign<T> {}
unsafe impl<T: Copy + AnyBitPattern> AnyBitPattern for Unalign<T> {}

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
