use core::marker::PhantomData;
use core::mem;

use crate::{DataType, Field, FieldList, is_valid, len_of_val, maybe_uninit};

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Repr<T: ?Sized + DataType, F: FieldList<T>> {
  r#type: PhantomData<T>,
  fields: F,
}

impl<T: ?Sized + DataType, U: FieldList<T>> Repr<T, U> {
  const LEN: usize = mem::size_of::<Self>() / mem::size_of::<Field<T, ()>>();

  #[inline(always)]
  pub const fn new(fields: U) -> Self {
    Self { fields, r#type: PhantomData }
  }

  #[inline(always)]
  pub const fn pop(self) -> (Option<Field<T, U::Type>>, Repr<T, U::Tail>) {
    union Node<T: ?Sized + DataType, U: FieldList<T>> {
      this: Repr<T, U>,
      term: Repr<T, U::Tail>,
      node: (Field<T, U::Type>, Repr<T, U::Tail>)
    }

    let repr = Node { this: self };
    match Self::LEN {
      // SAFETY: on terminal node, `Repr<T>` as the exact same representation as `Repr<T::Tail>`.
      0 => (None, unsafe { repr.term }),
      // SAFETY: `Repr<T>` as the exact same representation as `(Field<T::Head>, Repr<T::Tail>)`.
      _ => (Some(unsafe { repr.node.0 }), unsafe { repr.node.1 })
    }
  }

  #[inline(always)]
  pub const fn maybe_uninit(self, offset: usize) -> bool {
    match self.pop() {
      (Some(field), tail) => if offset >= field.offset && offset < field.offset + field.size() {
        maybe_uninit::<U::Type>(offset - field.offset)
      } else {
        tail.maybe_uninit(offset)
      },
      _ => false,
    }
  }

  #[inline(always)]
  pub const fn validate(self, val: &T) -> Result<(), isize> {
    let (field, tail) = match self.pop() {
      (Some(field), tail) => (field, tail),
      _ => return Ok(())
    };

    // Validate integer field.
    if let Some(uint) = field.try_load(val) {
      // Validate fixed field.
      match field.expect {
        Some(expected)
        if expected != uint => return Err(0),
        _ => {}
      }
      // Validate value bounds.
      match field.factor {
        // TODO: compute error (delta).
        Some(aspect)
        if field.remaining(val) != uint as usize * aspect.get() => return Err(0),
        _ => {}
      }
    }

    match is_valid(field.as_ref(val)) {
      Ok(()) => tail.validate(val),
      Err(err) => Err(err),
    }
  }

  #[inline(always)]
  pub const fn len_of_val(self, val: &T) -> usize {
    match self.pop() {
      (Some(field), _) if Self::LEN == 1 => len_of_val(field.as_ref(val)),
      (Some(_), tail) => tail.len_of_val(val),
      _ => 0
    }
  }
}

impl<T: ?Sized + DataType, F: FieldList<T>> Clone for Repr<T, F> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: ?Sized + DataType, F: FieldList<T>> Copy for Repr<T, F> {

}
