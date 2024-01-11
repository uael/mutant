use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::{mem, slice};
use core::mem::ManuallyDrop;

use crate::{DataType, IntegerSpec, Layout, size_of_val};

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Field<T: ?Sized + DataType, F: Copy + DataType> {
  parent: PhantomData<T>,
  r#type: PhantomData<F>,
  pub offset: usize,
  pub factor: Option<NonZeroUsize>,
  pub expect: Option<u64>,
}

#[inline]
pub const unsafe fn field<T: ?Sized + DataType, U: Copy + DataType>(
  offset: usize,
  factor: Option<usize>,
  expect: Option<U>,
) -> Field<T, U> {
  let factor = match factor {
    Some(n) => Some(match NonZeroUsize::new(n) {
      Some(n) => n,
      _ => panic!("")
    }),
    _ => None
  };
  let expect = match expect {
    Some(expect) => match U::LAYOUT {
      Layout::Integer { .. } => {
        union Expect<T> {
          expect: ManuallyDrop<T>,
          empty: ()
        }

        let expect = Expect { expect: ManuallyDrop::new(expect) };
        let expect_ptr: *const () = unsafe { &expect.empty };
        let bytes = unsafe { slice::from_raw_parts(expect_ptr.cast(), mem::size_of::<U>()) };
        Some(IntegerSpec::new().load(bytes))
      },
      _ => panic!(""),
    },
    _ => None,
  };
  Field { parent: PhantomData, r#type: PhantomData, offset, factor, expect }
}

impl<T: ?Sized + DataType, F: Copy + DataType> Field<T, F> {
  #[inline(always)]
  pub const fn size(self) -> usize {
    mem::size_of::<F>()
  }

  #[inline(always)]
  pub const fn layout(self) -> Layout<F> {
    F::LAYOUT
  }

  #[inline(always)]
  pub const fn as_ref(self, val: &T) -> &F {
    // SAFETY: safety is forwarded to the `unsafe` impl of the `DataType` trait.
    unsafe { &*self.as_ptr(val) }
  }

  #[inline(always)]
  pub const fn as_ptr(self, val: &T) -> *const F {
    let ptr: *const T = val;
    // SAFETY: safety is forwarded to the `unsafe` impl of the `DataType` trait.
    unsafe { ptr.cast::<u8>().offset(self.offset as isize).cast::<F>() }
  }

  #[inline(always)]
  pub(crate) const fn remaining(self, val: &T) -> usize {
    size_of_val(val) - self.offset - self.size()
  }

  #[inline(always)]
  pub(crate) const fn try_load(self, val: &T) -> Option<u64> {
    let spec = match self.layout() {
      Layout::Integer { spec } => spec,
      _ => return None,
    };

    let bytes = unsafe { slice::from_raw_parts(self.as_ptr(val).cast::<u8>(), self.size()) };
    Some(spec.load(bytes))
  }
}

pub trait FieldList<T: ?Sized + DataType>: Copy + 'static {
  type Type: Copy + DataType;
  type Tail: FieldList<T>;
}

impl<T: ?Sized + DataType> FieldList<T> for () {
  type Type = ();
  type Tail = ();
}

impl<T: ?Sized + DataType, F: Copy + DataType> FieldList<T> for Field<T, F> {
  type Type = F;
  type Tail = ();
}

impl<T: ?Sized + DataType, A: Copy + DataType, B: Copy + FieldList<T>> FieldList<T> for (Field<T, A>, B) {
  type Type = A;
  type Tail = B;
}

impl<T: ?Sized + DataType, F: Copy + DataType> Clone for Field<T, F> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: ?Sized + DataType, F: Copy + DataType> Copy for Field<T, F> {}
