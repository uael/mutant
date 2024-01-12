use core::marker::PhantomData;
use core::mem;

use crate::{is_fallible, is_unsized, layout_of, size_of_tail, validate, Data, Field, FieldList};

#[derive(Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Repr<Base: ?Sized + Data, Fields: FieldList<Base>> {
  r#type: PhantomData<Base>,
  fields: Fields,
}

impl<Base: ?Sized + Data, F: FieldList<Base>> Repr<Base, F> {
  const LEN: usize = mem::size_of::<Self>() / mem::size_of::<Field<Base, ()>>();

  #[inline(always)]
  pub const fn new(fields: F) -> Self {
    Self { r#type: PhantomData, fields }
  }

  #[inline(always)]
  pub const fn pop(self) -> (Option<Field<Base, F::Type>>, Repr<Base, F::Rest>) {
    union Node<Base: ?Sized + Data, F: FieldList<Base>> {
      this: Repr<Base, F>,
      term: Repr<Base, F::Rest>,
      node: (Field<Base, F::Type>, Repr<Base, F::Rest>),
    }

    let repr = Node { this: self };
    match Self::LEN {
      // SAFETY: on terminal node, `Repr<T>` as the exact same representation as `Repr<F::Rest>`.
      0 => (None, unsafe { repr.term }),
      // SAFETY: `Repr<T>` as the exact same representation as `(Field<F::Type>, Repr<F::Rest>)`.
      _ => (Some(unsafe { repr.node.0 }), unsafe { repr.node.1 }),
    }
  }

  #[inline(always)]
  pub const fn contains(self, offset: usize) -> bool {
    match self.pop() {
      (Some(field), _) => field.contains(offset),
      (None, _) => false,
    }
  }

  #[inline(always)]
  pub const fn layout_of(self, offset: usize) -> u8 {
    match self.pop() {
      // Reached the end of the struct, returns an uninitialized layout.
      (None, _) => 0x00,
      // `offset` does offset in field, defer to its inner layout.
      (Some(field), _) if field.contains(offset) => layout_of::<F::Type>(offset - field.offset),
      // `offset` does not offset in field, continue with next field.
      (_, tail) => tail.layout_of(offset),
    }
  }

  #[inline(always)]
  pub const fn is_unsized(self) -> bool {
    match self.pop() {
      (Some(_), _) if Self::LEN == 1 => is_unsized::<F::Type>(),
      (Some(_), tail) => tail.is_unsized(),
      (None, _) => false,
    }
  }

  #[inline(always)]
  pub const fn size_of_tail(self) -> usize {
    match self.pop() {
      (Some(_), _) if Self::LEN == 1 => size_of_tail::<F::Type>(),
      (Some(_), tail) => tail.size_of_tail(),
      (None, _) => 0,
    }
  }

  #[inline(always)]
  pub const fn validate(self, val: &Base) -> Result<(), isize> {
    let (field, tail) = match self.pop() {
      (Some(field), tail) => (field, tail),
      _ => return Ok(()),
    };

    match validate(field.as_ref(val)) {
      Ok(()) => {}
      Err(err) => return Err(err),
    }

    // Validate integer field.
    if let Some(uint) = field.try_load(val) {
      // Validate fixed field.
      match field.expect {
        Some(expect) if uint != expect => return Err(0),
        _ => {}
      }
      // Validate value bounds.
      match field.factor {
        Some(factor) => {
          let remaining = field.remaining(val);
          if remaining != uint as usize * factor.get() {
            return Err((uint as usize * factor.get()) as isize - remaining as isize);
          }
        }
        _ => {}
      }
    }

    tail.validate(val)
  }

  #[inline]
  pub const fn is_fallible(self) -> bool {
    match self.pop() {
      (Some(field), tail) => is_fallible::<F::Type>() || field.is_fallible() || tail.is_fallible(),
      (None, _) => false,
    }
  }
}

impl<Base: ?Sized + Data, T: FieldList<Base>> Clone for Repr<Base, T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<Base: ?Sized + Data, T: FieldList<Base>> Copy for Repr<Base, T> {}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{field, transmute_ref, try_transmute_ref, NoDrop};

  #[test]
  fn test_layout_of() {
    assert_eq!(&0u8, transmute_ref(&false));
    assert_eq!(&1u8, transmute_ref(&true));
    assert_eq!(Ok(&false), try_transmute_ref(&0u8));
    assert_eq!(Ok(&true), try_transmute_ref(&1u8));
    assert_eq!(Err(0), try_transmute_ref::<_, bool>(&2u8));

    #[derive(Copy, Clone, Data)]
    #[repr(C)]
    #[data(crate)]
    struct Foo(u8, u32);

    let fake_repr =
      unsafe { Repr::new((field::<(), u8>(0), (field::<(), u32>(4), field::<(), Foo>(8)))) };

    assert_eq!(0xff, fake_repr.layout_of(0));
    assert_eq!(0x00, fake_repr.layout_of(1));
    assert_eq!(0x00, fake_repr.layout_of(2));
    assert_eq!(0x00, fake_repr.layout_of(3));
    assert_eq!(0xff, fake_repr.layout_of(4));
    assert_eq!(0xff, fake_repr.layout_of(5));
    assert_eq!(0xff, fake_repr.layout_of(6));
    assert_eq!(0xff, fake_repr.layout_of(7));
    assert_eq!(0xff, fake_repr.layout_of(8));
    assert_eq!(0x00, fake_repr.layout_of(9));
    assert_eq!(0x00, fake_repr.layout_of(10));
    assert_eq!(0x00, fake_repr.layout_of(11));
    assert_eq!(0xff, fake_repr.layout_of(12));
    assert_eq!(0xff, fake_repr.layout_of(13));
    assert_eq!(0xff, fake_repr.layout_of(14));
    assert_eq!(0xff, fake_repr.layout_of(15));
    assert_eq!(0x00, fake_repr.layout_of(16));
  }

  #[test]
  fn test_validate() {
    #[derive(Copy, Clone, Data)]
    #[repr(C)]
    #[data(crate)]
    struct Foo {
      #[data(expect(Some(&0xcafe)))]
      bar: u16,
      baz: u16,
    }

    assert_eq!(Ok(()), validate(&Foo { bar: 0xcafe, baz: 0 }));
    assert_eq!(Err(0), validate(&Foo { bar: 0xfeca, baz: 0 }));

    #[derive(Data)]
    #[repr(C)]
    #[data(crate)]
    struct Bar<T: ?Sized> {
      #[data(size)]
      data_len: u32,
      data: T,
    }

    assert_eq!(Ok(()), validate(&Bar { data_len: 4, data: [0u8; 4] }));
    assert_eq!(Err(-1), validate(&Bar { data_len: 3, data: [0u8; 4] }));
    assert_eq!(Err(1), validate(&Bar { data_len: 5, data: [0u8; 4] }));

    #[derive(Data)]
    #[repr(C, packed)]
    #[data(crate)]
    struct Baz<T: ?Sized> {
      #[data(size(2))]
      data_len: u32,
      data: NoDrop<T>,
    }

    assert_eq!(Ok(()), validate(&Baz { data_len: 4, data: NoDrop::new([0u16; 4]) }));
    assert_eq!(Err(-2), validate(&Baz { data_len: 3, data: NoDrop::new([0u16; 4]) }));
    assert_eq!(Err(2), validate(&Baz { data_len: 5, data: NoDrop::new([0u16; 4]) }));

    #[derive(Data)]
    #[repr(C, packed)]
    #[data(crate)]
    struct Qux<T: ?Sized> {
      #[data(expect(Some(&0xcafe)))]
      yolo: u32,
      #[data(size)]
      data_len: u32,
      data: NoDrop<T>,
    }

    assert_eq!(Ok(()), validate(&Qux { yolo: 0xcafe, data_len: 4, data: NoDrop::new([0u8; 4]) }));
    assert_eq!(Err(-1), validate(&Qux { yolo: 0xcafe, data_len: 3, data: NoDrop::new([0u8; 4]) }));
    assert_eq!(Err(1), validate(&Qux { yolo: 0xcafe, data_len: 5, data: NoDrop::new([0u8; 4]) }));

    #[derive(Data)]
    #[repr(C, packed)]
    #[data(crate)]
    struct Fuz<T: ?Sized> {
      #[data(expect(Some(&0xcafe)))]
      yolo: u32,
      #[data(size, expect(Some(&4)))]
      data_len: u32,
      data: NoDrop<T>,
    }

    assert_eq!(Ok(()), validate(&Fuz { yolo: 0xcafe, data_len: 4, data: NoDrop::new([0u8; 4]) }));
    assert_eq!(Err(1), validate(&Fuz { yolo: 0xcafe, data_len: 4, data: NoDrop::new([0u8; 3]) }));
    assert_eq!(Err(0), validate(&Fuz { yolo: 0xcafe, data_len: 3, data: NoDrop::new([0u8; 3]) }));
    assert_eq!(Err(0), validate(&Fuz { yolo: 0xcafe, data_len: 5, data: NoDrop::new([0u8; 5]) }));
  }
}
