use crate::Mutant;

use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::fmt::{self, Debug, Display, Formatter};
use core::ptr::Pointee;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Undrop<T: ?Sized>(ManuallyDrop<T>);

unsafe impl<T: Mutant + ?Sized> Mutant for Undrop<T>
where
  Self: Pointee<Metadata = T::Metadata>
{
  type Head = T::Head;
  type Tail = T::Tail;

  #[inline(always)]
  fn validate(head: &T::Head, bytes: &[u8]) -> Option<T::Metadata> {
    let _ = Check::<T>::ASSERT_NOT_DROP;
    T::validate(head, bytes)
  }
}

impl<T: Sized> Undrop<T> {
  #[inline(always)]
  pub fn new(value: T) -> Self {
    let _ = Check::<T>::ASSERT_NOT_DROP;
    Self(ManuallyDrop::new(value))
  }

  #[inline(always)]
  pub fn into_inner(self) -> T {
    let _ = Check::<T>::ASSERT_NOT_DROP;
    ManuallyDrop::into_inner(self.0)
  }
}

impl<T: Display + ?Sized> Display for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Check::<T>::ASSERT_NOT_DROP;
    Display::fmt(unsafe { core::mem::transmute::<_, &T>(self) }, f)
  }
}

impl<T: Debug + ?Sized> Debug for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Check::<T>::ASSERT_NOT_DROP;
    Debug::fmt(unsafe { core::mem::transmute::<_, &T>(self) }, f)
  }
}

struct Check<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> Check<T> {
  const ASSERT_NOT_DROP: () = assert!(!core::mem::needs_drop::<T>());
}
