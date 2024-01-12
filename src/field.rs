use core::marker::PhantomData;
use core::num::NonZeroUsize;

use crate::{
  is_sized, is_unsized, min_size_of, size_of_val, transmute_ref, Data, Integer, IntegerSpec, Layout,
};

#[derive(Debug, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct Field<Base: ?Sized + Data, T: ?Sized + Data> {
  base: PhantomData<Base>,
  r#type: PhantomData<T>,
  pub offset: usize,
  pub factor: Option<NonZeroUsize>,
  pub expect: Option<u64>,
}

/// Instantiate a `Field` of the `Base` struct of type `T`.
///
/// # Safety
///
/// Caller must ensure that the provided `offset` is the right offset of this field within the
/// `Base` struct.
#[inline]
pub const unsafe fn field<Base: ?Sized + Data, T: ?Sized + Data>(offset: usize) -> Field<Base, T> {
  Field { base: PhantomData, r#type: PhantomData, offset, factor: None, expect: None }
}

impl<Base: ?Sized + Data, T: ?Sized + Data> Field<Base, T> {
  #[inline]
  pub const fn with_factor(mut self, factor: Option<usize>) -> Self {
    self.factor = match factor {
      Some(n) => NonZeroUsize::new(n),
      _ => None,
    };
    self
  }

  #[inline]
  pub const fn with_expect(mut self, expect: Option<&T>) -> Self
  where
    T: Integer,
  {
    self.expect = match expect {
      Some(expect) => Some(IntegerSpec::new().load(transmute_ref(expect))),
      _ => None,
    };
    self
  }

  #[inline(always)]
  pub const fn is_fallible(self) -> bool {
    self.expect.is_some() || self.factor.is_some()
  }

  #[inline(always)]
  pub const fn contains(self, offset: usize) -> bool {
    offset >= self.offset && (is_unsized::<T>() || offset < self.offset + min_size_of::<T>())
  }

  #[inline(always)]
  pub const fn as_ref(self, val: &Base) -> &T {
    // SAFETY: safety is forwarded to the `unsafe` impl of the `Data` trait.
    unsafe { &*self.as_ptr(val) }
  }

  #[inline(always)]
  pub const fn as_ptr(self, val: &Base) -> *const T {
    let ptr: *const Base = val;
    crate::ptr::from_raw_parts(
      // SAFETY: safety is forwarded to the `unsafe` impl of the `Data` trait.
      unsafe { ptr.cast::<u8>().offset(self.offset as isize) }.cast(),
      match is_sized::<T>() {
        true => 0,
        _ => crate::ptr::metadata(val),
      },
    )
  }

  #[inline(always)]
  pub(crate) const fn remaining(self, val: &Base) -> usize {
    size_of_val(val) - size_of_val(self.as_ref(val)) - self.offset
  }

  #[inline(always)]
  pub(crate) const fn try_load(self, val: &Base) -> Option<u64> {
    let spec = match T::LAYOUT {
      Layout::Integer { spec } => spec,
      _ => return None,
    };

    Some(spec.load(transmute_ref(self.as_ref(val))))
  }
}

pub trait FieldList<Base: ?Sized + Data>: Copy + 'static {
  type Type: ?Sized + Data;
  type Rest: FieldList<Base>;
}

impl<Base: ?Sized + Data> FieldList<Base> for () {
  type Type = ();
  type Rest = ();
}

impl<Base: ?Sized + Data, T: ?Sized + Data> FieldList<Base> for Field<Base, T> {
  type Type = T;
  type Rest = ();
}

impl<Base: ?Sized + Data, T: ?Sized + Data, Rest: Copy + FieldList<Base>> FieldList<Base>
  for (Field<Base, T>, Rest)
{
  type Type = T;
  type Rest = Rest;
}

impl<Base: ?Sized + Data, T: ?Sized + Data> Clone for Field<Base, T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<Base: ?Sized + Data, T: ?Sized + Data> Copy for Field<Base, T> {}
