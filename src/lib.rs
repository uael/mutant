#![feature(offset_of)]
use core::{mem, slice};
use core::mem::offset_of;

mod field;
mod layout;
mod num;
mod ptr;
mod repr;
mod unalign;
mod undrop;

#[cfg(test)]
pub(crate) mod tests;

pub use field::{field, Field, FieldList};
pub use layout::Layout;
pub use num::{Integer, IntegerSpec, Ordering, u16le, u32le};
pub use repr::Repr;
pub use unalign::Unalign;
pub use undrop::Undrop;

pub use mutant_derive::ensure;
pub use mutant_derive::DataType;

pub trait DataType: 'static {
  /// Type of the slice or array iem.
  type Item: Copy + DataType;
  /// Type of the sized part of `Self`.
  /// For `Sized` types, `Self::Head` is `Self`.
  type Head: Copy + DataType;
  /// Type of the trailing slice item for slice type or DST, `()` otherwise.
  type Tail: Copy + DataType;
  /// Fields of `Self`. Non-empty only for `struct` types.
  type Fields: FieldList<Self>;

  /// Layout of `Self`.
  const LAYOUT: Layout<Self>;

  #[inline]
  fn cast<T: ?Sized + DataType>(&self) -> &T {
    cast(self)
  }

  #[inline]
  fn try_cast<T: ?Sized + DataType>(&self) -> Result<&T, isize> {
    try_cast(self)
  }
}



// #[macro_export]
// macro_rules! offset_of {
//   ($Container:ty, $($fields:tt).+ $(,)?) => {
//     #[allow(unused_unsafe)]
//     unsafe {
//       union Zeroed<T> {
//         zeroed: ManuallyDrop<T>,
//         empty: ()
//       }
//
//       let zeroed = Zeroed { zeroed: ManuallyDrop::<$Container>::new(mem::zeroed()) };
//       let base_ptr: &$Container = &*(&zeroed.empty as *const ()).cast();
//       let field_ptr = addr_of!(base_ptr.$($fields).+);
//
//       ptr::addr_of(field_ptr) - ptr::addr_of(base_ptr)
//     }
//   };
// }











// fn ----------------------------------------------------------------------------------------------

#[inline]
pub const fn align_of<T: ?Sized + DataType>() -> usize {
  match T::LAYOUT {
    Layout::Slice => mem::align_of::<T::Item>(),
    _ => mem::align_of::<T::Head>(),
  }
}

#[inline]
pub const fn min_size_of<T: ?Sized + DataType>() -> usize {
  mem::size_of::<T::Head>()
}

#[inline]
pub const fn size_of_tail_item<T: ?Sized + DataType>() -> usize {
  mem::size_of::<T::Tail>()
}

#[inline]
pub const fn is_sized<T: ?Sized + DataType>() -> bool {
  size_of_tail_item::<T>() == 0
}

#[inline]
pub const fn is_unsized<T: ?Sized + DataType>() -> bool {
  size_of_tail_item::<T>() > 0
}

#[inline]
pub const fn size_of_val<T: ?Sized + DataType>(val: &T) -> usize {
  min_size_of::<T>() + ptr::metadata(val) * size_of_tail_item::<T>()
}

#[inline]
pub const fn maybe_uninit<T: ?Sized + DataType>(offset: usize) -> bool {
  // Check for `offset` to be in bounds.
  if is_sized::<T>() && offset >= min_size_of::<T>() {
    return true
  }
  match T::LAYOUT {
    Layout::Void => true,
    Layout::Slice | Layout::Array => {
      let offset = offset % mem::size_of::<T::Item>();
      maybe_uninit::<T::Item>(offset)
    }
    Layout::Struct { repr } => {
      // Handle head offset first (ie. not an offset within the trailing slice for `DST`).
      if offset < min_size_of::<T>() {
        return repr.maybe_uninit(offset);
      }
      // The offset point to the trailing slice of this `DST`,
      // compute the new offset withing the tail layout.
      let offset = (offset - min_size_of::<T>()) % size_of_tail_item::<T>();
      maybe_uninit::<T::Item>(offset)
    },
    Layout::Integer { spec } => match spec.bitsmask {
      Some(bitsmask) => bitsmask & (1 << offset) != 0xff,
      _ => false,
    }
  }
}

// #[doc(hidden)]
// #[inline]
// pub const fn tail_of<T: ?Sized + DataType>(val: &T) -> &T::Tail {
//   let tail_offset = min_size_of::<T>() - min_size_of::<T::Tail>();
//   let ptr = unsafe { (val as *const T).cast::<u8>().offset(tail_offset as isize) };
//   unsafe { &*ptr::from_raw_parts(ptr.cast(), ptr::metadata(val)) }
// }

#[inline]
pub const fn is_aligned_to<T: ?Sized + DataType>(value: &T, align: usize) -> bool {
  assert!(align.is_power_of_two(), "alignment must be power of 2");
  // `value` is aligned for `U` if `T` alignment is already big enough.
  align <= align_of::<T>()
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(value) & (align - 1) == 0
}

#[inline]
pub const fn len_of_val<T: ?Sized + DataType>(val: &T) -> usize {
  match T::LAYOUT {
    Layout::Slice => ptr::metadata(val),
    Layout::Array => min_size_of::<T>() / mem::size_of::<T::Item>(),
    Layout::Struct { repr } => repr.len_of_val(val),
    _ => 0,
  }
}

#[inline]
pub const fn is_valid<T: ?Sized + DataType>(val: &T) -> Result<(), isize> {
  match T::LAYOUT {
    Layout::Void => Ok(()),
    Layout::Struct { repr } => repr.validate(val),
    Layout::Slice | Layout::Array => {
      let ptr = (val as *const T).cast::<T::Item>();
      let len = len_of_val(val);
      let mut i = 0;
      while i < len {
        match is_valid(unsafe { &*ptr.offset(i as isize) }) {
          Ok(()) => {}
          Err(err) => return Err(err)
        }
        i += 1;
      }
      Ok(())
    },
    Layout::Integer { spec } => {
      let uint = {
        let ptr = (val as *const T).cast::<u8>();
        // SAFETY: `ptr` store at least `min_size_of2::<T>()` bytes.
        spec.load(unsafe { slice::from_raw_parts(ptr, min_size_of::<T>()) })
      };
      // Validate allowed bits.
      match spec.bitsmask {
        Some(bitsmask) if uint | bitsmask != bitsmask => return Err(0),
        _ => {}
      }
      Ok(())
    },
  }
}

#[inline]
pub const fn cast<T: ?Sized + DataType, U: ?Sized + DataType>(value: &T) -> &U {
  ensure!(is_aligned_to::<T>(value, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
  unsafe { &*ptr }
}

#[inline]
pub fn cast_mut<T: ?Sized + DataType, U: ?Sized + DataType>(value: &mut T) -> &mut U {
  ensure!(is_aligned_to::<T>(value, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts_mut::<U>(value as *mut T as *mut (), metadata);
  unsafe { &mut *ptr }
}

#[inline]
pub const fn try_cast<T: ?Sized + DataType, U: ?Sized + DataType>(value: &T) -> Result<&U, isize> {
  ensure!(is_aligned_to::<T>(value, align_of::<U>()), "alignment mismatch");
  let metadata = match layout::try_transmute::<T, U>(ptr::metadata(value)) {
    Ok(metadata) => metadata,
    Err(err) => return Err(err * size_of_tail_item::<T>() as isize),
  };
  let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
  let val = unsafe { &*ptr };
  match is_valid(val) {
    Ok(()) => Ok(val),
    Err(err) => Err(err),
  }
}

// impl DataType -----------------------------------------------------------------------------------

impl DataType for () {
  type Item = ();
  type Head = ();
  type Tail = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Void;
}

impl<T: Copy + DataType> DataType for [T] {
  type Item = T;
  type Head = ();
  type Tail = T;
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Slice;
}

impl<T: Copy + DataType, const N: usize> DataType for [T; N] {
  type Item = T;
  type Head = Self;
  type Tail = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Array;
}

impl<T: Integer> DataType for T {
  type Item = ();
  type Head = Self;
  type Tail = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Integer { spec: T::SPEC };
}

impl<A: Copy + DataType, B: ?Sized + DataType> DataType for (A, B) {
  type Item = ();
  type Head = (A, B::Head);
  type Tail = B::Tail;
  type Fields = (Field<Self, A>, Field<Self, B::Head>);
  const LAYOUT: Layout<Self> = Layout::Struct {
    repr: Repr::new(unsafe {(
      field::<Self, A>(offset_of!(Self::Head, 0), None, None),
      field::<Self, B::Head>(offset_of!(Self::Head, 1), None, None),
    )})
  };
}
