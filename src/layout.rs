use crate::Undrop;
use std::ptr::NonNull;
use std::{mem, ptr};

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct StructField {
  name: &'static str,
  offset: usize,
  layout: &'static TypeLayout,
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum TypeLayout {
  Scalar { size: usize, alignment: usize },
  Slice { item: &'static TypeLayout },
  Array { item: &'static TypeLayout, size: usize, alignment: usize },
  Struct { alignment: usize, fields: &'static [StructField] },
}

pub trait Metadata: Copy {
  const DEFAULT: Self;
}

impl Metadata for () {
  const DEFAULT: Self = ();
}

impl Metadata for usize {
  const DEFAULT: Self = 0;
}

pub trait Layout {
  type Metadata: Metadata;
  const LAYOUT: TypeLayout;
}

impl<T: Layout> Layout for [T] {
  type Metadata = usize;
  const LAYOUT: TypeLayout = TypeLayout::Slice { item: &T::LAYOUT };
}

impl<T: Layout, const N: usize> Layout for [T; N] {
  type Metadata = ();
  const LAYOUT: TypeLayout = TypeLayout::Array {
    item: &T::LAYOUT,
    size: mem::size_of::<Self>(),
    alignment: mem::align_of::<Self>(),
  };
}

impl Layout for u8 {
  type Metadata = ();
  const LAYOUT: TypeLayout =
    TypeLayout::Scalar { size: mem::size_of::<Self>(), alignment: mem::align_of::<Self>() };
}

impl Layout for u16 {
  type Metadata = ();
  const LAYOUT: TypeLayout =
    TypeLayout::Scalar { size: mem::size_of::<Self>(), alignment: mem::align_of::<Self>() };
}

impl TypeLayout {
  #[inline(always)]
  pub const fn align_of(&self) -> usize {
    match self {
      TypeLayout::Scalar { alignment, .. } => *alignment,
      TypeLayout::Slice { item } => item.align_of(),
      TypeLayout::Array { alignment, .. } => *alignment,
      TypeLayout::Struct { alignment, .. } => *alignment,
    }
  }

  #[inline(always)]
  pub const fn min_size_of(&self) -> usize {
    self.size_of_with(0)
  }

  #[inline(always)]
  pub const fn size_of_with(&self, metadata: usize) -> usize {
    match self {
      TypeLayout::Scalar { size, .. } => *size,
      TypeLayout::Slice { item } => metadata * item.min_size_of(),
      TypeLayout::Array { size, .. } => *size,
      TypeLayout::Struct { fields, .. } => match fields.last() {
        Some(field) => field.offset + field.layout.size_of_with(metadata),
        _ => 0,
      },
    }
  }
}

pub const fn align_of<T: ?Sized + Layout>() -> usize {
  #[repr(C)]
  struct OffsetOfTrailingIsAlignment<T: ?Sized> {
    _byte: u8,
    _trailing: T,
  }

  impl<T: ?Sized + Layout> Layout for OffsetOfTrailingIsAlignment<T> {
    type Metadata = T::Metadata;
    const LAYOUT: TypeLayout = TypeLayout::Struct { alignment: 0, fields: &[] };
  }

  #[repr(C)]
  union PtrRepr<T: ?Sized + Layout> {
    const_ptr: *const T,
    mut_ptr: *mut T,
    components: PtrComponents<T>,
  }

  #[repr(C)]
  struct PtrComponents<T: ?Sized + Layout> {
    data_address: *const (),
    metadata: <T as Layout>::Metadata,
  }

  // Manual impl needed to avoid `T: Copy` bound.
  impl<T: ?Sized + Layout> Copy for PtrComponents<T> {}

  // Manual impl needed to avoid `T: Clone` bound.
  impl<T: ?Sized + Layout> Clone for PtrComponents<T> {
    fn clone(&self) -> Self {
      *self
    }
  }

  let ptr = unsafe {
    PtrRepr::<OffsetOfTrailingIsAlignment<T>> {
      components: PtrComponents {
        data_address: ALIGNED_64K_ALLOCATION.as_ptr() as *const (),
        metadata: <T as Layout>::Metadata::DEFAULT,
      },
    }
    .const_ptr
  };

  let field = unsafe {
    ptr::addr_of!((*ptr)._trailing)
  };

  let offset = unsafe { field.cast::<u8>().offset_from(ptr.cast::<u8>()) };
  assert!(offset >= 0);
  #[allow(clippy::as_conversions)]
  { offset as usize }
}

#[repr(C, packed)]
struct Dst<T: ?Sized> {
  a: u8,
  b: u16,
  c: [u8; 6],
  d: Undrop<T>,
}

impl<T: ?Sized + Layout> Layout for Dst<T> {
  type Metadata = T::Metadata;
  const LAYOUT: TypeLayout = TypeLayout::Struct { alignment: align_of::<Self>(), fields: &[] };
}

const _64K: usize = 1 << 16;

#[repr(C, align(65536))]
struct Aligned64kAllocation([u8; _64K]);

/// A pointer to an aligned allocation of size 2^16.
///
/// # Safety
///
/// `ALIGNED_64K_ALLOCATION` is guaranteed to point to the entirety of an
/// allocation with size and alignment 2^16, and to have valid provenance.
pub const ALIGNED_64K_ALLOCATION: NonNull<[u8]> = {
  const REF: &Aligned64kAllocation = &Aligned64kAllocation([0; _64K]);
  let ptr: *const Aligned64kAllocation = REF;
  let ptr: *const [u8] = ptr::slice_from_raw_parts(ptr.cast(), _64K);
  // SAFETY:
  // - `ptr` is derived from a Rust reference, which is guaranteed to be
  //   non-null.
  // - `ptr` is derived from an `&Aligned64kAllocation`, which has size and
  //   alignment `_64K` as promised. Its length is initialized to `_64K`,
  //   which means that it refers to the entire allocation.
  // - `ptr` is derived from a Rust reference, which is guaranteed to have
  //   valid provenance.
  //
  // TODO(#429): Once `NonNull::new_unchecked` docs document that it preserves
  // provenance, cite those docs.
  unsafe { NonNull::new_unchecked(ptr.cast_mut()) }
};

/// Computes the offset of the base of the field `$trailing_field_name` within
/// the type `$ty`.
///
/// `offset_of!` produces code which is valid in a `const` context.
#[doc(hidden)] // `#[macro_export]` bypasses this module's `#[doc(hidden)]`.
#[macro_export]
macro_rules! __offset_of {
  ($ty:ty, $trailing_field_name:tt) => {{
    #[repr(C)]
    #[derive(Copy, Clone)]
    union PtrRepr<T: ?Sized + Layout> {
      const_ptr: *const T,
      mut_ptr: *mut T,
      components: PtrComponents<T>,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct PtrComponents<T: ?Sized + Layout> {
      data_address: *const (),
      metadata: <T as Layout>::Metadata,
    }

    let ptr = unsafe { PtrRepr { components: PtrComponents { data_address: ALIGNED_64K_ALLOCATION.as_ptr() as *const (), metadata: <$ty as Metadata>::DEFAULT } }.const_ptr };

    let min_size = {
      let zero_elems: *const [()] =
        core::ptr::slice_from_raw_parts(
          core::ptr::NonNull::<()>::dangling()
            .as_ptr()
            .cast_const(),
          0,
        );
      // SAFETY:
      // - If `$ty` is `Sized`, `size_of_val_raw` is always safe to call.
      // - Otherwise:
      //   - If `$ty` is not a slice DST, this pointer conversion will
      //     fail due to "mismatched vtable kinds", and compilation will
      //     fail.
      //   - If `$ty` is a slice DST, the safety requirement is that "the
      //     length of the slice tail must be an initialized integer, and
      //     the size of the entire value (dynamic tail length +
      //     statically sized prefix) must fit in isize." The length is
      //     initialized to 0 above, and Rust guarantees that no type's
      //     minimum size may overflow `isize`. [1]
      //
      // [1] TODO(#429),
      // TODO(https://github.com/rust-lang/unsafe-code-guidelines/issues/465#issuecomment-1782206516):
      // Citation for this?
      unsafe {
        #[allow(clippy::as_conversions)]
        core::mem::size_of_val(unsafe { &*(zero_elems as *const $ty) })
      }
    };

    assert!(min_size <= _64K);

    #[allow(clippy::as_conversions)]
    let ptr = ALIGNED_64K_ALLOCATION.as_ptr() as *const $ty;

    // SAFETY:
    // - Thanks to the preceding `assert!`, we know that the value with zero
    //   elements fits in `_64K` bytes, and thus in the allocation addressed
    //   by `ALIGNED_64K_ALLOCATION`. The offset of the trailing field is
    //   guaranteed to be no larger than this size, so this field projection
    //   is guaranteed to remain in-bounds of its allocation.
    // - Because the minimum size is no larger than `_64K` bytes, and
    //   because an object's size must always be a multiple of its alignment
    //   [1], we know that `$ty`'s alignment is no larger than `_64K`. The
    //   allocation addressed by `ALIGNED_64K_ALLOCATION` is guaranteed to
    //   be aligned to `_64K`, so `ptr` is guaranteed to satisfy `$ty`'s
    //   alignment.
    //
    //   Note that, as of [2], this requirement is technically unnecessary
    //   for Rust versions >= 1.75.0, but no harm in guaranteeing it anyway
    //   until we bump our MSRV.
    //
    // [1] Per https://doc.rust-lang.org/reference/type-layout.html:
    //
    //   The size of a value is always a multiple of its alignment.
    //
    // [2] https://github.com/rust-lang/reference/pull/1387
    let field = unsafe {
      core::ptr::addr_of!((*ptr).$trailing_field_name)
    };
    // SAFETY:
    // - Both `ptr` and `field` are derived from the same allocated object.
    // - By the preceding safety comment, `field` is in bounds of that
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
    let offset = unsafe { field.cast::<u8>().offset_from(ptr.cast::<u8>()) };
    // Guaranteed not to be lossy: `field` comes after `ptr`, so the offset
    // from `ptr` to `field` is guaranteed to be positive.
    assert!(offset >= 0);
    #[allow(clippy::as_conversions)]
    { offset as usize }
  }};
}

/// Computes alignment of `$ty: ?Sized`.
///
/// `align_of!` produces code which is valid in a `const` context.
#[doc(hidden)] // `#[macro_export]` bypasses this module's `#[doc(hidden)]`.
#[macro_export]
macro_rules! __align_of {
  ($ty:ty) => {{
    // SAFETY: `OffsetOfTrailingIsAlignment` is `repr(C)`, and its layout is
    // guaranteed [1] to begin with the single-byte layout for `_byte`,
    // followed by the padding needed to align `_trailing`, then the layout
    // for `_trailing`, and finally any trailing padding bytes needed to
    // correctly-align the entire struct.
    //
    // This macro computes the alignment of `$ty` by counting the number of
    // bytes preceeding `_trailing`. For instance, if the alignment of `$ty`
    // is `1`, then no padding is required align `_trailing` and it will be
    // located immediately after `_byte` at offset 1. If the alignment of
    // `$ty` is 2, then a single padding byte is required before
    // `_trailing`, and `_trailing` will be located at offset 2.

    // This correspondence between offset and alignment holds for all valid
    // Rust alignments, and we confirm this exhaustively (or, at least up to
    // the maximum alignment supported by `offset_of!`) in
    // `test_align_of_dst`.
    //
    // [1]: https://doc.rust-lang.org/nomicon/other-reprs.html#reprc

    #[repr(C)]
    struct OffsetOfTrailingIsAlignment {
      _byte: u8,
      _trailing: $ty,
    }

    $crate::__offset_of!(OffsetOfTrailingIsAlignment, _trailing)
  }};
}

#[cfg(test)]
mod tests {
  use super::*;

  fn test() {}
}
