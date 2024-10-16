#![cfg_attr(not(test), no_std)]
/// # mutant
///
/// ## TODO:
/// - add `wip` intersection solution aside current one.
/// - make sure `wip` works.
/// - remove old intersection solution.
/// - transmute functions.
/// - better validation ?
/// - size field ?
/// - tests.
pub mod endian;
pub mod layout;
pub mod num;
pub mod ptr;
pub mod sizing;

mod internal;
#[cfg(test)]
mod tests;

pub use mutant_derive::Type;

// -------------------------------------------------------------------------------------------------
// `transmute_unchecked`

#[inline]
const unsafe fn transmute_unchecked<T: Copy, U: Copy>(val: T) -> U {
  union Transmute<T: Copy, U: Copy> {
    val: T,
    dst: U,
  }
  Transmute { val }.dst
}

// -------------------------------------------------------------------------------------------------
// `offset_of!`

/// A pointer to an aligned allocation of size 2^20.
/// Used to compute the offset of a field within a type using [`offset_of`].
#[doc(hidden)]
pub const BIG_ALLOCATION: *const () = {
  #[repr(C, align(1048576))]
  struct Big([u8; 1 << 20]);

  const REF: &Big = &Big([0; 1 << 20]);
  let ptr: *const Big = REF;
  ptr.cast()
};

/// Expands to the offset in bytes of a field from the beginning of the given type.
///
/// `offset_of!` produces code which is valid in a `const` context.
#[doc(hidden)]
#[macro_export]
macro_rules! offset_of {
  ($Container:ty, $($fields:tt).+) => {{
    use $crate::Default;
    let ptr = $crate::ptr::from_raw_parts::<$Container>(
      $crate::BIG_ALLOCATION,
      <$Container as $crate::ptr::Pointee>::Metadata::DEFAULT
    );
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
    // [1] TODO(https://github.com/rust-lang/rust/pull/116675):
    //     Once these are guaranteed in the Reference, cite it.
    #[allow(unused_unsafe)]
    let offset = unsafe { field.cast::<u8>().offset_from(ptr.cast::<u8>()) };
    // Guaranteed not to be lossy: `field` comes after `ptr`, so the offset
    // from `ptr` to `field` is guaranteed to be positive.
    assert!(offset >= 0);
    offset as usize
  }};
}

// -------------------------------------------------------------------------------------------------
// marker traits

pub trait Literal<T> {
  const LITERAL: T;
}

pub trait Default: Sized {
  const DEFAULT: Self;
}

pub trait Zero: Sized {
  const ZERO: Self;
}

pub trait Ones: Sized {
  const ONES: Self;
}

// impl marker traits.
const _: () = {
  use core::num::NonZero;

  impl Zero for bool {
    const ZERO: Self = false;
  }

  impl Zero for char {
    const ZERO: Self = '\0';
  }

  impl<T: Zero, const N: usize> Zero for [T; N] {
    const ZERO: Self = [T::ZERO; N];
  }

  impl<T: Ones, const N: usize> Ones for [T; N] {
    const ONES: Self = [T::ONES; N];
  }

  macro_rules! impl_non_zero {
    { $($T:ident)+ } => {$(
      impl Ones for NonZero<$T> {
        const ONES: Self = unsafe { NonZero::<$T>::new_unchecked($T::ONES) };
      }
    )+};
  }

  impl_non_zero! { u8 u16 u32 u64 u128 }
};

// -------------------------------------------------------------------------------------------------
// `ByteArray` trait

/// # Safety
/// This trait **must** only be implemented for byte array as of `[u8; Self::LEN]`.
pub unsafe trait ByteArray: Copy + Zero + Ones + 'static {
  const LEN: usize;
}

#[inline]
pub const fn bytes_of<T: ByteArray, const N: usize>(val: T) -> [u8; N] {
  const { assert!(N == T::LEN) };
  unsafe { transmute_unchecked(val) }
}

#[inline]
pub const fn bytes_ref<T: ByteArray>(val: &T) -> &[u8] {
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &*ptr::from_raw_parts(core::ptr::from_ref(val).cast(), T::LEN) }
}

#[inline]
pub const fn bytes_mut<T: ByteArray>(val: &mut T) -> &mut [u8] {
  // SAFETY: The `ByteArray` bound imply that `T` is a byte array as of `[u8; T::LEN]`.
  unsafe { &mut *ptr::from_raw_parts_mut(core::ptr::from_mut(val).cast(), T::LEN) }
}

// impl `ByteArray`.
const _: () = {
  unsafe impl<const N: usize> ByteArray for [u8; N] {
    const LEN: usize = N;
  }
};

// -------------------------------------------------------------------------------------------------
// `MinimallySized` trait

pub trait MinimallySized<S: sizing::Sizing> {
  const MINSIZE: usize;
}

// impl `MinimallySized`
const _: () = {
  impl<T> MinimallySized<sizing::Sized> for T {
    const MINSIZE: usize = size_of::<T>();
  }

  impl<T> MinimallySized<sizing::Unsized> for [T] {
    const MINSIZE: usize = 0;
  }
};

// -------------------------------------------------------------------------------------------------
// `Type` trait

pub trait Type: MinimallySized<Self::Sizing> {
  type Sizing: sizing::Sizing;
  type Layout: layout::Layout;
}

#[inline]
pub const fn min_size_of<T: Type + ?Sized>() -> usize {
  <T as MinimallySized<T::Sizing>>::MINSIZE
}

#[inline]
pub const fn is_sized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_sized()
}

#[inline]
pub const fn is_unsized<T: Type + ?Sized>() -> bool {
  T::Sizing::LITERAL.is_unsized()
}

#[inline]
pub const fn default<T: Type + Copy>() -> T {
  const {
    let mut uninit = core::mem::MaybeUninit::<T>::uninit();
    unsafe {
      layout::initialize_to_default::<T::Layout>(core::ptr::from_mut(&mut uninit).cast());
      transmute_unchecked(uninit)
    }
  }
}

#[inline]
const unsafe fn validate<T: Type + ?Sized>(ptr: *const (), metadata: usize) -> bool {
  layout::satisfy::<T::Layout>(ptr.cast(), metadata)
}

// impl `Type`
const _: () = {
  use core::marker::PhantomData;

  impl<T: Type + Copy> Default for T {
    const DEFAULT: Self = default::<T>();
  }

  impl<T: layout::Raw> Type for T {
    type Sizing = sizing::Sized;
    type Layout = T;
  }

  pub struct ArrayLayout<T, const N: usize>(PhantomData<T>);

  impl<T: Type, const N: usize> layout::Layout for ArrayLayout<T, N> {
    type Lhs = T::Layout;
    type Rhs = ();
    type Raw = ();
    const KIND: layout::LayoutKind = layout::LayoutKind::Repeat(N, size_of::<T>());
  }

  impl<T: Type, const N: usize> Type for [T; N] {
    type Sizing = sizing::Sized;
    type Layout = ArrayLayout<T, N>;
  }

  pub struct SliceLayout<T>(PhantomData<T>);

  impl<T: Type> layout::Layout for SliceLayout<T> {
    type Lhs = T::Layout;
    type Rhs = ();
    type Raw = ();
    const KIND: layout::LayoutKind = layout::LayoutKind::Wildcard(size_of::<T>());
  }

  impl<T: Type> Type for [T] {
    type Sizing = sizing::Unsized;
    type Layout = SliceLayout<T>;
  }
};
