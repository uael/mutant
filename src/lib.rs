use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::{mem, slice};

mod field;
mod layout;
mod nodrop;
mod num;
pub mod ptr;
mod repr;
mod unalign;

#[cfg(test)]
pub(crate) mod tests;

pub use field::{field, Field, FieldList};
pub use layout::Layout;
pub use nodrop::NoDrop;
pub use num::*;
pub use repr::Repr;
pub use unalign::{assert_unaligned, Unalign};

pub use mutant_derive::ensure;
pub use mutant_derive::Data;

// offset_of ---------------------------------------------------------------------------------------

#[repr(C, align(1048576))]
struct BigAllocation([u8; 1 << 20]);

/// A pointer to an aligned allocation of size 2^20.
#[doc(hidden)]
pub const BIG_ALLOCATION: *const () = {
  const REF: &BigAllocation = &BigAllocation([0; 1 << 20]);
  let ptr: *const BigAllocation = REF;
  ptr.cast()
};

/// Expands to the offset in bytes of a field from the beginning of the given type.
///
/// `offset_of!` produces code which is valid in a `const` context.
#[macro_export]
macro_rules! offset_of {
  ($Container:ty, $($fields:tt).+) => {{
    let ptr = $crate::ptr::from_raw_parts::<$Container>($crate::BIG_ALLOCATION, 0);
    // SAFETY:
    // `(*ptr)` pointer deference syntax is required for `addr_of!` to work
    // but the content of deference is never actually read.
    #[allow(unused_unsafe)]
    let field = unsafe { ::core::ptr::addr_of!((*ptr).$($fields).+) };
    // SAFETY:
    // - Both `ptr` and `field` are derived from the same allocated object.
    // - `addr_of!` ensure result provenance, hence `field` is in bounds of that
    //   allocated object.
    // - The distance, in bytes, between `ptr` and `field` is required to be
    //   a multiple of the size of `u8`, which is trivially true because
    //   `u8`'s size is 1.
    // - The distance, in bytes, cannot overflow `isize`. This is guaranteed
    //   because no allocated object can have a size larger than can fit in
    //   `isize`. [1]
    // - The distance being in-bounds cannot rely on wrapping around the
    //   address space. This is guaranteed because the same is guaranteed of
    //   allocated objects. [1]
    //
    // [1] TODO(#429), TODO(https://github.com/rust-lang/rust/pull/116675):
    //     Once these are guaranteed in the Reference, cite it.
    #[allow(unused_unsafe)]
    let offset = unsafe { field.cast::<u8>().offset_from(ptr.cast::<u8>()) };
    // Guaranteed not to be lossy: `field` comes after `ptr`, so the offset
    // from `ptr` to `field` is guaranteed to be positive.
    assert!(offset >= 0);
    offset as usize
  }};
}

// Traits ------------------------------------------------------------------------------------------

pub unsafe trait Data: 'static {
  /// Type of slice or array item.
  type Item: Copy + Data;
  /// Fields of `Self`. Non-empty only for `struct` types.
  type Fields: FieldList<Self>;
  /// Layout of `Self`.
  const LAYOUT: Layout<Self>;

  /// Size in bytes of the sized part of `Self`.
  /// For [`Sized`](Sized) types, this is equals to [`mem::size_of`](mem::size_of)`::<Self>()`.
  #[doc(hidden)]
  const HEADER: usize;

  #[inline]
  fn cast<T: ?Sized + Data>(&self) -> &T {
    transmute_ref(self)
  }

  #[inline]
  fn try_cast<T: ?Sized + Data>(&self) -> Result<&T, isize> {
    try_transmute_ref(self)
  }
}

#[inline]
pub const fn align_of<T: ?Sized>() -> usize {
  #[repr(C)]
  struct AlignOf<T: ?Sized>(u8, T);

  // `AlignOf` is `repr(C)`, and its layout is guaranteed [1] to begin
  // with the single-byte layout for `AlignOf::0`, followed by the padding
  // needed to align `AlignOf::1`, then the layout for `AlignOf::1`, and
  // finally any trailing padding bytes needed to correctly-align the entire
  // struct.
  //
  // This macro computes the alignment of `$ty` by counting the number of
  // bytes preceding `AlignOf::1`. For instance, if the alignment of `$ty`
  // is `1`, then no padding is required align `AlignOf::1` and it will be
  // located immediately after `AlignOf::0` at offset 1. If the alignment of
  // `$ty` is 2, then a single padding byte is required before
  // `AlignOf::1`, and `AlignOf::1` will be located at offset 2.
  //
  // [1]: https://doc.rust-lang.org/nomicon/other-reprs.html#reprc
  offset_of!(AlignOf<T>, 1)
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  T::HEADER
}

#[inline]
pub const fn size_of_tail<T: ?Sized + Data>() -> usize {
  match T::LAYOUT {
    Layout::Slice => mem::size_of::<T::Item>(),
    Layout::Struct { repr } => repr.size_of_tail(),
    _ => 0,
  }
}

#[inline]
pub const fn size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of::<T>() + size_of_tail_val(val)
}

#[inline]
pub const fn size_of_tail_val<T: ?Sized + Data>(val: &T) -> usize {
  ptr::metadata(val) * size_of_tail::<T>()
}

#[inline]
pub const fn is_unsized<T: ?Sized + Data>() -> bool {
  match T::LAYOUT {
    Layout::Slice => true,
    Layout::Struct { repr } => repr.is_unsized(),
    _ => false,
  }
}

#[inline]
pub const fn is_sized<T: ?Sized + Data>() -> bool {
  !is_unsized::<T>()
}

#[inline]
pub const fn layout_of<T: ?Sized + Data>(offset: usize) -> u8 {
  // Check for `offset` to be in bounds.
  if is_sized::<T>() && offset >= min_size_of::<T>() {
    return 0x00;
  }
  match T::LAYOUT {
    Layout::Void => unreachable!(),
    Layout::Slice | Layout::Array => layout_of::<T::Item>(offset % mem::size_of::<T::Item>()),
    Layout::Struct { repr } => repr.layout_of(offset),
    Layout::Integer { spec } => match spec.layout {
      Some(layout) => ((layout >> (offset * 8)) & 0xff) as u8,
      _ => 0xff,
    },
  }
}

#[inline]
pub const fn is_aligned_to<T: ?Sized + Data>(val: &T, align: usize) -> bool {
  assert!(align.is_power_of_two(), "alignment must be power of 2");
  // `value` is aligned for `U` if `T` alignment is already big enough.
  align <= align_of::<T>()
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(val) & (align - 1) == 0
}

#[inline]
pub const fn validate<T: ?Sized + Data>(val: &T) -> Result<(), isize> {
  match T::LAYOUT {
    Layout::Void => Ok(()),
    Layout::Struct { repr } => repr.validate(val),
    Layout::Slice | Layout::Array => {
      let ptr = (val as *const T).cast::<T::Item>();
      let len = match T::LAYOUT {
        Layout::Slice => ptr::metadata(val),
        _ => min_size_of::<T>() / mem::size_of::<T::Item>(),
      };
      let mut i = 0;
      while i < len {
        match validate(unsafe { &*ptr.offset(i as isize) }) {
          Ok(()) => {}
          Err(err) => return Err(err),
        }
        i += 1;
      }
      Ok(())
    }
    Layout::Integer { spec } => {
      let uint = {
        let ptr = (val as *const T).cast::<u8>();
        // SAFETY: `ptr` store at least `min_size_of2::<T>()` bytes.
        spec.load(unsafe { slice::from_raw_parts(ptr, min_size_of::<T>()) })
      };
      // Validate allowed bits.
      match spec.layout {
        Some(bitsmask) if uint | bitsmask != bitsmask => return Err(0),
        _ => {}
      }
      Ok(())
    }
  }
}

#[inline]
pub const fn is_fallible<T: ?Sized + Data>() -> bool {
  match T::LAYOUT {
    Layout::Void => false,
    Layout::Struct { repr } => repr.is_fallible(),
    Layout::Slice | Layout::Array => is_fallible::<T::Item>(),
    Layout::Integer { spec } => spec.layout.is_some(),
  }
}

#[inline]
pub const fn transmute<T: Copy + Data, U: Copy + Data>(src: T) -> U {
  let _ = layout::transmute::<T, U>(0);
  unsafe { transmute_unchecked(src) }
}

#[inline]
pub const fn transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> &U {
  ensure!(is_aligned_to::<T>(src, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(src));
  let ptr = ptr::from_raw_parts::<U>(src as *const T as *const (), metadata);
  unsafe { &*ptr }
}

#[inline]
pub fn transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(src: &mut T) -> &mut U {
  ensure!(is_aligned_to::<T>(src, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(src));
  let ptr = ptr::from_raw_parts_mut::<U>(src as *mut T as *mut (), metadata);
  unsafe { &mut *ptr }
}

#[inline]
pub const fn try_transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> Result<&U, isize> {
  ensure!(is_aligned_to::<T>(src, align_of::<U>()), "alignment mismatch");
  let metadata = match layout::try_transmute::<T, U>(ptr::metadata(src)) {
    Ok(metadata) => metadata,
    Err(err) => return Err(err * size_of_tail::<T>() as isize),
  };
  let ptr = ptr::from_raw_parts::<U>(src as *const T as *const (), metadata);
  let val = unsafe { &*ptr };
  match validate(val) {
    Ok(()) => Ok(val),
    Err(err) => Err(err),
  }
}

#[inline]
pub const fn try_transmute<T: Copy + Data, U: Copy + Data>(src: T) -> Result<U, isize> {
  let _ = match layout::try_transmute::<T, U>(0) {
    Ok(metadata) => metadata,
    Err(err) => return Err(err * size_of_tail::<T>() as isize),
  };
  union Transmute<T: Copy, U: Copy> {
    src: T,
    dst: U,
    empty: (),
  }
  let t = Transmute { src };
  match validate(unsafe { &*(&t.empty as *const ()).cast::<U>() }) {
    Ok(()) => Ok(unsafe { t.dst }),
    Err(err) => Err(err),
  }
}

#[inline]
pub const unsafe fn transmute_unchecked<T, U>(src: T) -> U {
  union Transmute<T, U> {
    src: ManuallyDrop<T>,
    dst: ManuallyDrop<U>,
  }

  ManuallyDrop::<U>::into_inner(Transmute::<T, U> { src: ManuallyDrop::new(src) }.dst)
}

// impl Data -----------------------------------------------------------------------------------

unsafe impl Data for () {
  type Item = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Void;
  const HEADER: usize = 0;
}

unsafe impl<T: ?Sized + 'static> Data for PhantomData<T> {
  type Item = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Void;
  const HEADER: usize = 0;
}

unsafe impl<T: Copy + Data> Data for [T] {
  type Item = T;
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Slice;
  const HEADER: usize = 0;
}

unsafe impl<T: Copy + Data, const N: usize> Data for [T; N] {
  type Item = T;
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Array;
  const HEADER: usize = mem::size_of::<Self>();
}

unsafe impl<T: Integer> Data for T {
  type Item = ();
  type Fields = ();
  const LAYOUT: Layout<Self> = Layout::Integer { spec: T::SPEC };
  const HEADER: usize = mem::size_of::<Self>();
}

unsafe impl<A: Copy + Data, B: Copy + Data> Data for (A, B) {
  type Item = ();
  type Fields = (Field<Self, A>, Field<Self, B>);
  const LAYOUT: Layout<Self> = Layout::Struct {
    repr: unsafe {
      Repr::new((field::<Self, A>(offset_of!(Self, 0)), field::<Self, B>(offset_of!(Self, 1))))
    },
  };
  const HEADER: usize = mem::size_of::<Self>();
}
