use core::mem::ManuallyDrop;
use core::num::NonZeroU8;
use core::{mem, slice};

pub use mutant_derive::Data;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Error {
  AlignmentMismatch,
  SizeMismatch(isize),
  InvalidTransmute(&'static str),
}

#[rustfmt::skip]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering {
  Lsb = 1, Msb,
}

#[rustfmt::skip]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Layout {
  Void, Slice, Array, Struct,
  Raw { layout: RawLayout },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ByteLayout {
  Uninit,
  AllowedBits(NonZeroU8),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct RawLayout {
  pub ordering: Option<Ordering>,
  pub allowed_bits: Option<u64>, // TODO(uael): use something like `&'static [ByteLayout]`
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
#[repr(transparent)]
pub struct Dropless<T: ?Sized>(ManuallyDrop<T>);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default, Data)]
#[data(crate)]
#[repr(C, packed)]
pub struct Unalign<T: Copy>(pub T);

pub unsafe trait Repr<T: ?Sized + Data> {
  type Type: ?Sized + Data;
  type Rest: Repr<T>;
  const OFFSET: usize;
  const FIELDS: usize;
}

pub unsafe trait Data: 'static {
  /// Type of slice or array item.
  type Item: Copy + Data;
  /// Type of the trailing slice item.
  type Tail: Copy + Data;
  /// Fields of `Self`. Non-empty only for `struct` types.
  type Repr: Repr<Self>;
  /// Layout of `Self`.
  const LAYOUT: Layout;

  /// Size in bytes of the sized part of `Self`.
  /// For [`Sized`](Sized) types, this is equals to [`mem::size_of`](mem::size_of)`::<Self>()`.
  #[doc(hidden)]
  const HEADER: usize;

  #[inline]
  fn transmute<T: Copy + Data>(self) -> T
  where
    Self: Copy,
  {
    transmute(self)
  }

  #[inline]
  fn transmute_ref<T: ?Sized + Data>(&self) -> &T {
    transmute_ref(self)
  }

  #[inline]
  fn try_transmute<T: Copy + Data>(self) -> Result<T, Error>
  where
    Self: Copy,
  {
    try_transmute(self)
  }

  #[inline]
  fn try_transmute_ref<T: ?Sized + Data>(&self) -> Result<&T, Error> {
    try_transmute_ref(self)
  }
}

pub unsafe trait RawData: Copy + Data {
  const RAW_LAYOUT: RawLayout;
}

// Macros ------------------------------------------------------------------------------------------

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

/// Safely reference unaligned data.
#[macro_export]
macro_rules! unaligned_ref {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_ref!($base.$field);
    $crate::unaligned_ref!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &_ = $base;
    let addr = core::ptr::addr_of!($base.$field);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
}

/// Safely mutably reference unaligned data.
#[macro_export]
macro_rules! unaligned_mut {
  ($base:tt.$field:tt.$($fields:tt).+) => {{
    let base = $crate::unaligned_mut!($base.$field);
    $crate::unaligned_mut!(base.$($fields).+)
  }};
  ($base:tt.$field:tt) => {{
    let _ref: &mut _ = $base;
    let addr = core::ptr::addr_of_mut!($base.$field);
    $crate::const_assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `const_assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
}

/// Reference data without interior mutability.
#[doc(hidden)]
macro_rules! data_ref {
  ($val:expr) => {{
    union Deref<T: Copy + Data> {
      val: T,
      void: (),
    }
    const unsafe fn cast<T: Copy + Data>(void: *const ()) -> *const T {
      void.cast()
    }
    let deref = Deref { val: $val };
    // SAFETY:
    // - `Data` types does contains any interior mutability.
    // - `union Deref<T>` has been initialized with a valid `T`.
    // - `&deref.void` has the same address as `deref` and `deref.val`.
    unsafe { &*cast(&deref.void) }
  }};
}

/// A pointer to an aligned allocation of size 2^20.
#[doc(hidden)]
pub const BIG_ALLOCATION: *const () = {
  #[repr(C, align(1048576))]
  struct Big([u8; 1 << 20]);

  const REF: &Big = &Big([0; 1 << 20]);
  let ptr: *const Big = REF;
  ptr.cast()
};

#[doc(hidden)]
#[rustfmt::skip]
pub const fn const_assert_unaligned<T: ?Sized + Data>(_: *const T) {
  struct Assert<T: ?Sized>(*const T);
  impl<T: ?Sized + Data> Assert<T> { const ASSERT_UNALIGNED: () = assert!(align_of::<T>() == 1); }
  let _ = Assert::<T>::ASSERT_UNALIGNED;
}

// Functions ---------------------------------------------------------------------------------------

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
pub const fn byte_layout<T: ?Sized + Data>(offset: usize) -> ByteLayout {
  #[inline(always)]
  const fn repr_byte_layout<R: Repr<T>, T: ?Sized + Data>(offset: usize) -> ByteLayout {
    match R::FIELDS {
      // Reached the end of the struct, returns an uninitialized layout.
      0 => ByteLayout::Uninit,
      // `offset` does offset in field, defer to its inner layout.
      _ if offset >= R::OFFSET
        && (is_unsized::<R::Type>() || offset < R::OFFSET + min_size_of::<R::Type>()) =>
      {
        byte_layout::<R::Type>(offset - R::OFFSET)
      }
      // `offset` does not offset in field, continue with next field.
      _ => repr_byte_layout::<R::Rest, T>(offset),
    }
  }

  // Check for `offset` to be in bounds.
  if is_sized::<T>() && offset >= min_size_of::<T>() {
    return ByteLayout::Uninit;
  }
  match T::LAYOUT {
    Layout::Void => unreachable!(),
    Layout::Slice | Layout::Array => byte_layout::<T::Item>(offset % mem::size_of::<T::Item>()),
    Layout::Struct => repr_byte_layout::<T::Repr, T>(offset),
    Layout::Raw { layout: spec } => ByteLayout::new(match spec.allowed_bits {
      Some(layout) => ((layout >> (offset * 8)) & 0xff) as u8,
      _ => 0xff,
    }),
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
pub const fn is_sized<T: ?Sized + Data>() -> bool {
  !is_unsized::<T>()
}

#[inline]
pub const fn is_unsized<T: ?Sized + Data>() -> bool {
  #[inline(always)]
  const fn repr_is_unsized<R: Repr<T>, T: ?Sized + Data>() -> bool {
    match R::FIELDS {
      0 => false,
      1 => is_unsized::<R::Type>(),
      _ => repr_is_unsized::<R::Rest, T>(),
    }
  }

  match T::LAYOUT {
    Layout::Slice => true,
    Layout::Struct => repr_is_unsized::<T::Repr, T>(),
    _ => false,
  }
}

#[inline]
pub const unsafe fn is_valid_bits<T: ?Sized + Data>(ptr: *const T) -> Result<(), Error> {
  #[inline(always)]
  const fn offset<R: Repr<T>, T: ?Sized + Data>(ptr: *const T) -> *const R::Type {
    assert!(R::FIELDS != 0);
    ptr::from_raw_parts(
      // SAFETY: safety is forwarded to the `unsafe` impl of the `Data` trait.
      unsafe { ptr.cast::<u8>().offset(R::OFFSET as isize) }.cast(),
      match is_sized::<R::Type>() {
        true => 0,
        _ => ptr::metadata(ptr),
      },
    )
  }

  #[inline(always)]
  const unsafe fn repr_is_valid_bits<R: Repr<T>, T: ?Sized + Data>(
    ptr: *const T,
  ) -> Result<(), Error> {
    match R::FIELDS {
      0 => Ok(()),
      _ => match is_valid_bits::<R::Type>(offset::<R, T>(ptr)) {
        Ok(_) => repr_is_valid_bits::<R::Rest, T>(ptr),
        Err(err) => Err(err),
      },
    }
  }

  match T::LAYOUT {
    Layout::Struct => return repr_is_valid_bits::<T::Repr, T>(ptr),
    Layout::Slice | Layout::Array => {
      let ptr = ptr.cast::<T::Item>();
      let len = match T::LAYOUT {
        Layout::Slice => ptr::metadata(ptr),
        _ => min_size_of::<T>() / mem::size_of::<T::Item>(),
      };
      let mut i = 0;
      while i < len {
        match is_valid_bits(ptr.offset(i as isize)) {
          Ok(()) => {}
          Err(err) => return Err(err),
        }
        i += 1;
      }
    }
    Layout::Raw { layout: spec } => match spec.allowed_bits {
      Some(allowed_bits) => {
        let uint = spec.load(slice::from_raw_parts(ptr.cast(), min_size_of::<T>()));
        if uint | allowed_bits != allowed_bits {
          return Err(Error::InvalidTransmute("TODO"));
        }
      }
      _ => {}
    },
    _ => {}
  }

  Ok(())
}

#[inline]
pub const fn min_size_of<T: ?Sized + Data>() -> usize {
  T::HEADER
}

#[inline]
pub const fn size_of_tail<T: ?Sized + Data>() -> usize {
  mem::size_of::<T::Tail>()
}

#[inline]
pub const fn size_of_tail_val<T: ?Sized + Data>(val: &T) -> usize {
  ptr::metadata(val) * size_of_tail::<T>()
}

#[inline]
pub const fn size_of_val<T: ?Sized + Data>(val: &T) -> usize {
  min_size_of::<T>() + size_of_tail_val(val)
}

#[inline]
pub const fn split_at<T: ?Sized + Data>(val: &T, mid: usize) -> (&T, &[T::Tail]) {
  match try_split_at(val, mid) {
    Some(result) => result,
    _ => panic!("`try_split_at` failed"),
  }
}

#[inline]
pub const fn try_split_at<T: ?Sized + Data>(val: &T, mid: usize) -> Option<(&T, &[T::Tail])> {
  if mid < min_size_of::<T>() || mid > size_of_val(val) {
    return None;
  }
  if mid == size_of_val(val) {
    return Some((val, &[]));
  }
  let tail = mid - min_size_of::<T>();
  if tail % mem::size_of::<T::Tail>() != 0 {
    return None;
  }
  let ptr: *const u8 = (val as *const T).cast();
  let val_ptr = ptr::from_raw_parts::<T>(ptr.cast(), tail / mem::size_of::<T::Tail>());
  let rest_ptr = ptr::from_raw_parts::<[T::Tail]>(
    unsafe { ptr.offset(mid as isize) }.cast(),
    ptr::metadata(val) - ptr::metadata(val_ptr),
  );
  Some(unsafe { (&*val_ptr, &*rest_ptr) })
}

#[inline]
pub const fn transmute<T: Copy + Data, U: Copy + Data>(src: T) -> U {
  *transmute_ref::<T, U>(data_ref!(src))
}

#[inline]
pub fn transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(src: &mut T) -> &mut U {
  match try_transmute_mut(src) {
    Ok(dst) => dst,
    Err(err) => match err {
      Error::AlignmentMismatch => panic!("alignment mismatch"),
      Error::SizeMismatch(_) => panic!("size mismatch"),
      Error::InvalidTransmute(err) => panic!("{}", err),
    },
  }
}

#[inline]
pub const fn transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> &U {
  match try_transmute_ref(src) {
    Ok(dst) => dst,
    Err(err) => match err {
      Error::AlignmentMismatch => panic!("alignment mismatch"),
      Error::SizeMismatch(_) => panic!("size mismatch"),
      Error::InvalidTransmute(err) => panic!("{}", err),
    },
  }
}

#[inline]
pub const fn try_transmute<T: Copy + Data, U: Copy + Data>(src: T) -> Result<U, Error> {
  match try_transmute_ref::<T, U>(data_ref!(src)) {
    Ok(dst) => Ok(*dst),
    Err(err) => Err(err),
  }
}

#[inline]
pub fn try_transmute_mut<T: ?Sized + Data, U: ?Sized + Data>(src: &mut T) -> Result<&mut U, Error> {
  if !is_aligned_to::<T>(src, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match layout::Transmute::<T, U, true>::apply(ptr::metadata(src)) {
    Ok(metadata) => metadata,
    Err(err) => return Err(err),
  };
  let ptr = ptr::from_raw_parts_mut::<U>(src as *mut T as *mut (), metadata);
  match unsafe { is_valid_bits(ptr) } {
    Ok(()) => Ok(unsafe { &mut *ptr }),
    Err(err) => Err(err),
  }
}

#[inline]
pub const fn try_transmute_ref<T: ?Sized + Data, U: ?Sized + Data>(src: &T) -> Result<&U, Error> {
  if !is_aligned_to::<T>(src, align_of::<U>()) {
    return Err(Error::AlignmentMismatch);
  }
  let metadata = match layout::Transmute::<T, U, false>::apply(ptr::metadata(src)) {
    Ok(metadata) => metadata,
    Err(err) => return Err(err),
  };
  let ptr = ptr::from_raw_parts::<U>(src as *const T as *const (), metadata);
  match unsafe { is_valid_bits(ptr) } {
    Ok(()) => Ok(unsafe { &*ptr }),
    Err(err) => Err(err),
  }
}

// ptr ---------------------------------------------------------------------------------------------

pub mod ptr {
  use core::mem;

  /// Extract the metadata component of a pointer.
  #[inline]
  pub const fn metadata<T: ?Sized>(ptr: *const T) -> usize {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline]
  pub const fn from_raw_parts<T: ?Sized>(address: *const (), metadata: usize) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline]
  pub const fn from_raw_parts_mut<T: ?Sized>(address: *mut (), metadata: usize) -> *mut T {
    unsafe { PtrRepr::with_components(address, metadata).mut_ptr }
  }

  /// Extract the address component of a non-dangling pointer.
  // #[inline]
  pub(super) const fn addr_of<T: ?Sized>(ptr: *const T) -> usize {
    // Compute `ptr` raw pointer `ptr`.
    let ptr = ptr.cast::<()>();
    // SAFETY: This no-op trick ensure that `ptr` has a provenance:
    // This will result in a compile-error if `ptr` is a dangling pointer (has no provenance).
    let _ = unsafe { core::ptr::addr_of!((*(ptr as *const (u8, ()))).1) };
    // SAFETY: This is just an inlined `p.addr()` (which is not a `const fn` so we cannot call it).
    // - This is safe at runtime as `ptr` point to a valid non-dangling reference, hence the pointer
    //   has a provenance.
    // - During const eval, it is unsound to perform this transmutation if the pointer has no
    //   provenance; the above trick ensure just that.
    unsafe { mem::transmute(ptr) }
  }

  #[repr(C)]
  union PtrRepr<T: ?Sized> {
    const_ptr: *const T,
    mut_ptr: *mut T,
    components: PtrComponents,
  }

  #[repr(C)]
  #[derive(Copy, Clone)]
  struct PtrComponents {
    address: *const (),
    metadata: usize,
  }

  impl<T: ?Sized> PtrRepr<T> {
    const fn empty() -> Self {
      Self::empty_with(core::ptr::null())
    }

    const fn empty_with(address: *const ()) -> Self {
      Self::with_components(address, 0)
    }

    const fn with_components(address: *const (), metadata: usize) -> Self {
      Self { components: PtrComponents { address, metadata } }
    }

    const fn set_const(mut self, const_ptr: *const T) -> Self {
      self.const_ptr = const_ptr;
      self
    }
  }
}

// layout ------------------------------------------------------------------------------------------

mod layout {
  use core::marker::PhantomData;
  use core::num::NonZeroUsize;

  use crate::*;

  /// Metadata requirements.
  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  enum Requirements {
    Is(usize),
    IsAtLeast(usize),
    IsMultipleOf(usize),
    IsAtLeastThenMultipleOf(usize, usize),
    IsEmpty,
    None,
  }

  impl Requirements {
    #[inline]
    const fn delta(self, metadata: usize) -> isize {
      match self {
        Self::Is(n) if metadata == n => 0,
        Self::IsAtLeast(n) if metadata >= n => 0,
        Self::Is(n) | Self::IsAtLeast(n) => n as isize - metadata as isize,
        Self::IsMultipleOf(m) if metadata % m == 0 => 0,
        Self::IsMultipleOf(m) => m as isize - (metadata % m) as isize,
        Self::IsAtLeastThenMultipleOf(n, m) if metadata >= n && metadata % m == 0 => 0,
        Self::IsAtLeastThenMultipleOf(n, _) if metadata < n => n as isize - metadata as isize,
        Self::IsAtLeastThenMultipleOf(_, m) => m as isize - (metadata % m) as isize,
        Self::IsEmpty if metadata == 0 => 0,
        Self::IsEmpty => -(metadata as isize),
        Self::None => 0,
      }
    }

    #[inline]
    const fn of<T: ?Sized + Data, U: ?Sized + Data>(mutable: bool) -> Result<Self, &'static str> {
      let (ah, at, bh, bt) = (
        min_size_of::<T>(),
        NonZeroUsize::new(size_of_tail::<T>()),
        min_size_of::<U>(),
        NonZeroUsize::new(size_of_tail::<U>()),
      );
      let mut min_at_count = 0; // Minimum number of `at` required to align `ah[at]` and `bh[bt]`.
      if ah < bh {
        let at = match at {
          Some(tail) => tail.get(),
          _ => return Err("cannot transmute between types of different sizes, source too small"),
        };
        let missing = bh - ah;
        if bt.is_none() {
          if missing % at != 0 {
            return Err(
              "cannot transmute between types of incompatible byte layouts, \
              missing bytes to fit the target is not a multiple of the source `DST` \
              trailing slice item size",
            );
          }
        }
        min_at_count = (missing + at - 1) / at; // Ceiling of division.
      } else if ah > bh {
        let bt = match bt {
          Some(tail) => tail.get(),
          _ => return Err("cannot transmute between types of different sizes, target too small"),
        };
        let remaining = ah - bh;
        if at.is_none() && remaining % bt != 0 {
          return Err(
            "cannot transmute between types of incompatible byte layouts, \
            remaining bytes to fit in the target is not a multiple of the target `DST` \
            trailing slice item size",
          );
        }
      }
      let requirements = match (at, bt) {
        (None, _) => Self::None,
        (Some(at), None) => {
          if ah < bh {
            Self::Is((bh - ah) / at.get())
          } else {
            Self::IsEmpty
          }
        }
        (Some(at), Some(bt)) => {
          #[inline]
          const fn gcd(a: usize, b: usize) -> usize {
            match b {
              0 => a,
              _ => gcd(b, a % b),
            }
          }

          let (at, bt) = (at.get(), bt.get());
          let gcd = gcd(at, bt);
          let lcm = (at * bt) / gcd;
          while (ah + min_at_count * at - bh) % bt != 0 {
            if min_at_count * at > lcm {
              return Err(
                "cannot transmute between `DST` types of incompatible byte layouts, \
                source cannot catchup on target",
              );
            }
            min_at_count += 1;
          }
          // Compute the minimum number of times `at` must be repeated to reach a size that is a
          // multiple of `bt`.
          match (min_at_count, bt / gcd) {
            (0, 1) => Self::None,
            (0, m) => Self::IsMultipleOf(m),
            (n, 1) => Self::IsAtLeast(n),
            (n, m) => Self::IsAtLeastThenMultipleOf(n, m),
          }
        }
      };
      // Check for no-uninit bytes.
      let at = size_of_tail::<T>();
      let minimum_size = match requirements {
        Self::Is(n) | Self::IsAtLeast(n) => ah + (n * at),
        Self::IsMultipleOf(m) => ah + (m * at),
        Self::IsAtLeastThenMultipleOf(n, _) => ah + (n * at),
        Self::IsEmpty | Self::None => ah,
      };
      let mut offset = 0;
      while offset < minimum_size {
        if !byte_layout::<T>(offset).is_transmutable_to(byte_layout::<U>(offset), mutable) {
          return Err(
            "cannot transmute between types of incompatible byte layouts, \
            source padding will left uninitialized byte(s) in target",
          );
        }
        offset += 1;
      }
      Ok(requirements)
    }
  }

  pub(crate) struct Transmute<T: ?Sized, U: ?Sized, const MUT: bool>(PhantomData<(*const T, U)>);

  impl<T: ?Sized + Data, U: ?Sized + Data, const MUT: bool> Transmute<T, U, MUT> {
    const REQUIREMENTS: Result<Requirements, &'static str> = Requirements::of::<T, U>(MUT);

    pub const fn apply(metadata: usize) -> Result<usize, Error> {
      let delta = match Self::REQUIREMENTS {
        Err(err) => return Err(Error::InvalidTransmute(err)),
        Ok(requirements) => requirements.delta(metadata),
      };
      if delta != 0 {
        return Err(Error::SizeMismatch(delta * size_of_tail::<T>() as isize));
      }
      if size_of_tail::<U>() == 0 {
        return Ok(0);
      }
      let size = min_size_of::<T>() + metadata * size_of_tail::<T>();
      Ok((size - min_size_of::<U>()) / size_of_tail::<U>())
    }
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn test_byte_layout() {
      use core::mem;
      use Requirements::*;

      #[derive(Debug, Copy, Clone, Data)]
      #[repr(C, packed)]
      #[data(crate)]
      pub struct BazPacked {
        a: u8,
        b: u32,
        c: [u8; 6],
      }

      for offset in 0..mem::size_of::<BazPacked>() {
        assert_eq!(ByteLayout::new(0xff), byte_layout::<BazPacked>(offset), "at {}", offset);
      }

      #[derive(Debug, Copy, Clone, Data)]
      #[repr(C)]
      #[data(crate)]
      pub struct Baz(u8, u32, [u8; 6]);

      assert_eq!(Ok(None), Requirements::of::<BazPacked, [u8]>(false));
      assert_eq!(
        Err(
          "cannot transmute between types of incompatible byte layouts, \
          source padding will left uninitialized byte(s) in target"
        ),
        Requirements::of::<Baz, [u8]>(false)
      );

      assert_eq!(Ok(None), Requirements::of::<u8, bool>(false));
      assert_eq!(Ok(None), Requirements::of::<bool, u8>(false));
      assert_eq!(Ok(None), Requirements::of::<u8, bool>(true));
      assert_eq!(
        Err(
          "cannot transmute between types of incompatible byte layouts, source padding will left \
          uninitialized byte(s) in target"
        ),
        Requirements::of::<bool, u8>(true)
      );
    }

    #[test]
    fn test_requirements() {
      use Requirements::*;

      #[derive(Data)]
      #[repr(C, packed)]
      #[data(crate)]
      struct Ty<const H: usize, const T: usize = 0>([u8; H], [[u8; T]]);

      // Basic tests for equal head and tail sizes
      assert_eq!(Ok(None), Requirements::of::<Ty<1>, Ty<1>>(false));
      assert_eq!(Ok(None), Requirements::of::<Ty<42>, Ty<42>>(false));
      assert_eq!(Ok(None), Requirements::of::<Ty<42, 1>, Ty<42, 1>>(false));

      // Test for equal head sizes with different tail sizes
      assert_eq!(Ok(None), Requirements::of::<Ty<42, 3>, Ty<42, 1>>(false));
      assert_eq!(Ok(IsMultipleOf(10)), Requirements::of::<Ty<50, 10>, Ty<50, 100>>(false));
      assert_eq!(Ok(IsMultipleOf(20)), Requirements::of::<Ty<50, 5>, Ty<50, 100>>(false));

      // Test cases for transmuting to and from empty tail
      assert_eq!(Ok(IsEmpty), Requirements::of::<Ty<42, 1>, Ty<42>>(false));
      assert_eq!(Ok(Is(7)), Requirements::of::<Ty<35, 1>, Ty<42>>(false));

      // Test cases for large difference in head sizes
      assert_eq!(Ok(Is(80)), Requirements::of::<Ty<20, 1>, Ty<100>>(false));

      // Test cases for tail sizes as multiples
      assert_eq!(Ok(IsMultipleOf(2)), Requirements::of::<Ty<42, 1>, Ty<42, 2>>(false));
      assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 1>, Ty<42, 3>>(false));
      assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 2>, Ty<42, 3>>(false));
      assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 4>, Ty<42, 3>>(false));
      assert_eq!(Ok(IsMultipleOf(8)), Requirements::of::<Ty<42, 5>, Ty<42, 8>>(false));
      assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 2>, Ty<42, 6>>(false));

      // Test zero heads
      assert_eq!(Ok(Is(42)), Requirements::of::<Ty<0, 1>, Ty<42>>(false));
      assert_eq!(Ok(IsAtLeast(42)), Requirements::of::<Ty<0, 1>, Ty<42, 1>>(false));

      // Test cases for alignment and complex transmutations
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(2, 3)),
        Requirements::of::<Ty<42, 1>, Ty<38, 3>>(false)
      );
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(1, 3)),
        Requirements::of::<Ty<42, 2>, Ty<38, 3>>(false)
      );
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(1, 3)),
        Requirements::of::<Ty<43, 2>, Ty<39, 3>>(false)
      );
      assert_eq!(Ok(IsMultipleOf(2)), Requirements::of::<Ty<43, 2>, Ty<39, 4>>(false));
      assert_eq!(Ok(IsAtLeast(2)), Requirements::of::<Ty<1, 2>, Ty<5, 2>>(false));
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(1, 3)),
        Requirements::of::<Ty<45, 2>, Ty<38, 3>>(false)
      );
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(2, 4)),
        Requirements::of::<Ty<40, 1>, Ty<38, 4>>(false)
      );
      assert_eq!(
        Ok(IsAtLeastThenMultipleOf(2, 4)),
        Requirements::of::<Ty<44, 5>, Ty<38, 4>>(false)
      );
      assert_eq!(Ok(IsAtLeast(1)), Requirements::of::<Ty<38, 3>, Ty<41, 3>>(false));

      // Panic test cases for incompatible transmutations
      assert_eq!(
        Err("cannot transmute between types of different sizes, target too small"),
        Requirements::of::<Ty<2>, Ty<1>>(false)
      );
      assert_eq!(
        Err("cannot transmute between types of different sizes, source too small"),
        Requirements::of::<Ty<1>, Ty<2>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between types of incompatible byte layouts, \
          missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
          item size"
        ),
        Requirements::of::<Ty<35, 2>, Ty<42>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between types of incompatible byte layouts, \
          missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
          item size"
        ),
        Requirements::of::<Ty<40, 3>, Ty<42>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between types of incompatible byte layouts, \
          remaining bytes to fit in the target is not a multiple of the target `DST` \
          trailing slice item size"
        ),
        Requirements::of::<Ty<42>, Ty<38, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<42, 3>, Ty<38, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<41, 3>, Ty<39, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<38, 3>, Ty<42, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<39, 3>, Ty<41, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<39, 3>, Ty<41, 6>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<39, 6>, Ty<41, 3>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<39, 6>, Ty<41, 9>>(false)
      );
      assert_eq!(
        Err(
          "cannot transmute between `DST` types of incompatible byte layouts, \
          source cannot catchup on target"
        ),
        Requirements::of::<Ty<39, 3>, Ty<41, 9>>(false)
      );
    }
  }
}

// internal ----------------------------------------------------------------------------------------

pub use internal::*;

mod internal {
  use core::fmt;
  use core::fmt::{Debug, Display, Formatter};
  use core::marker::PhantomData;
  use core::mem;
  use core::mem::ManuallyDrop;
  use core::num::{NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};
  use core::ops::{Deref, DerefMut};

  use crate::*;

  // impl ByteLayout -------------------------------------------------------------------------------

  impl ByteLayout {
    pub const fn new(allowed_bits: u8) -> Self {
      match NonZeroU8::new(allowed_bits) {
        Some(allowed_bits) => Self::AllowedBits(allowed_bits),
        _ => Self::Uninit,
      }
    }

    pub const fn is_transmutable_to(self, dst: Self, mutable: bool) -> bool {
      match (self, dst) {
        (_, Self::Uninit) => true,
        (Self::Uninit, _) => false,
        (Self::AllowedBits(src), Self::AllowedBits(dst)) => {
          !mutable || src.get() | dst.get() == src.get()
        }
      }
    }
  }

  // impl IntegerSpec ------------------------------------------------------------------------------

  impl RawLayout {
    #[inline(always)]
    pub const fn new() -> Self {
      Self { ordering: None, allowed_bits: None }
    }

    #[inline(always)]
    pub const fn with_ordering(self, ordering: Ordering) -> Self {
      Self { ordering: Some(ordering), allowed_bits: self.allowed_bits }
    }

    #[inline(always)]
    pub const fn with_allowed_bits(self, allowed_bits: u64) -> Self {
      Self { ordering: self.ordering, allowed_bits: Some(allowed_bits) }
    }

    #[inline(always)]
    pub const fn ordering(self) -> Ordering {
      match self.ordering {
        Some(ordering) => ordering,
        #[cfg(target_endian = "little")]
        _ => Ordering::Lsb,
        #[cfg(target_endian = "big")]
        _ => Ordering::Msb,
      }
    }

    #[inline(always)]
    pub const fn load(self, bytes: &[u8]) -> u64 {
      let (mut bit, mut uint) = (0, 0);
      assert!(bytes.len() <= 8);
      while bit < bytes.len() {
        let byte = bytes[bit];
        uint |= (byte as u64)
          << 8
            * match self.ordering() {
              Ordering::Lsb => bit,
              Ordering::Msb => bytes.len() - bit,
            };
        bit += 1;
      }
      uint
    }
  }

  // impl Dropless ---------------------------------------------------------------------------------

  #[doc(hidden)]
  pub struct DroplessRepr;

  unsafe impl<T: ?Sized + Data> Repr<Dropless<T>> for DroplessRepr {
    type Type = T;
    type Rest = ();
    const OFFSET: usize = 0;
    const FIELDS: usize = 1;
  }

  unsafe impl<T: ?Sized + Data> Data for Dropless<T> {
    type Item = ();
    type Tail = T::Tail;
    type Repr = DroplessRepr;
    const LAYOUT: Layout = Layout::Struct;
    const HEADER: usize = T::HEADER;
  }

  impl<T: ?Sized> Dropless<T> {
    const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
  }

  impl<T> Dropless<T> {
    #[inline(always)]
    pub const fn new(value: T) -> Self {
      let _ = Self::ASSERT_NOT_DROP;
      Self(ManuallyDrop::new(value))
    }

    #[inline(always)]
    pub const fn into_inner(self) -> T {
      let _ = Self::ASSERT_NOT_DROP;
      ManuallyDrop::into_inner(self.0)
    }
  }

  impl<T: ?Sized> Deref for Dropless<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
      self.0.deref()
    }
  }

  impl<T: ?Sized> DerefMut for Dropless<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
      self.0.deref_mut()
    }
  }

  impl<T: Display + ?Sized> Display for Dropless<T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Display::fmt(self.deref(), f)
    }
  }

  impl<T: Debug + ?Sized> Debug for Dropless<T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(self.deref(), f)
    }
  }

  // impl Unalign ----------------------------------------------------------------------------------

  impl<T: Copy> Unalign<T> {
    #[inline(always)]
    pub const fn new(value: T) -> Self {
      Self(value)
    }

    #[inline(always)]
    pub const fn into_inner(self) -> T {
      self.0
    }
  }

  impl<T: Copy + Data> Deref for Unalign<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
      unaligned_ref!(self.0)
    }
  }

  impl<T: Copy + Data> DerefMut for Unalign<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
      unaligned_mut!(self.0)
    }
  }

  impl<T: Display + Copy> Display for Unalign<T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Display::fmt(&self.into_inner(), f)
    }
  }

  impl<T: Debug + Copy> Debug for Unalign<T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      Debug::fmt(&self.into_inner(), f)
    }
  }

  // impl Repr -------------------------------------------------------------------------------------

  unsafe impl<T: ?Sized + Data> Repr<T> for () {
    type Type = ();
    type Rest = ();
    const OFFSET: usize = 0;
    const FIELDS: usize = 0;
  }

  // impl Data -------------------------------------------------------------------------------------

  unsafe impl Data for () {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Void;
    const HEADER: usize = 0;
  }

  unsafe impl<T: ?Sized + 'static> Data for PhantomData<T> {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Void;
    const HEADER: usize = 0;
  }

  unsafe impl<T: Copy + Data> Data for [T] {
    type Item = T;
    type Tail = T;
    type Repr = ();
    const LAYOUT: Layout = Layout::Slice;
    const HEADER: usize = 0;
  }

  unsafe impl<T: Copy + Data, const N: usize> Data for [T; N] {
    type Item = T;
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Array;
    const HEADER: usize = mem::size_of::<Self>();
  }

  unsafe impl<T: RawData> Data for T {
    type Item = ();
    type Tail = ();
    type Repr = ();
    const LAYOUT: Layout = Layout::Raw { layout: T::RAW_LAYOUT };
    const HEADER: usize = mem::size_of::<Self>();
  }

  // TODO:
  // unsafe impl<A: Copy + Data, B: Copy + Data> Data for (A, B) {
  //   type Item = ();
  //   type Tail = B::Tail;
  //   type Repr = (Field<Self, A>, Field<Self, B>);
  //   const LAYOUT: Layout = Layout::Struct {
  //     repr: unsafe {
  //       Repr::new((field::<Self, A>(offset_of!(Self, 0)), field::<Self, B>(offset_of!(Self, 1))))
  //     },
  //   };
  //   const HEADER: usize = mem::size_of::<Self>();
  // }

  // impl numbers ----------------------------------------------------------------------------------

  macro_rules! impl_num {
    ($ty:ident $raw:ident $bytes:literal $ordering:ident $a:ident $b:ident $c:ident) => {
      #[derive(Eq, PartialEq, Copy, Clone, Hash)]
      #[allow(non_camel_case_types)]
      #[repr(transparent)]
      pub struct $ty([u8; $bytes]);

      unsafe impl RawData for $ty {
        const RAW_LAYOUT: RawLayout = RawLayout::new().with_ordering(Ordering::$ordering);
      }

      impl $ty {
        pub const fn $a(bytes: [u8; $bytes]) -> Self {
          Self(bytes)
        }

        pub const fn $b(bytes: [u8; $bytes]) -> Self {
          Self::$a($raw::$b(bytes).$c())
        }

        pub const fn from_ne_bytes(bytes: [u8; $bytes]) -> Self {
          Self::$a($raw::from_ne_bytes(bytes).$c())
        }

        pub const fn from_raw(uint: $raw) -> Self {
          Self::$a(uint.$c())
        }

        pub const fn into_raw(self) -> $raw {
          $raw::$a(self.0)
        }
      }

      impl From<$raw> for $ty {
        fn from(value: $raw) -> Self {
          Self::from_raw(value)
        }
      }

      impl Into<$raw> for $ty {
        fn into(self) -> $raw {
          self.into_raw()
        }
      }

      impl Debug for $ty {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
          <$raw as Debug>::fmt(&self.into_raw(), f)
        }
      }

      impl Display for $ty {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
          <$raw as Display>::fmt(&self.into_raw(), f)
        }
      }
    };
  }

  impl_num!(u16le u16 2 Lsb from_le_bytes from_be_bytes to_le_bytes);
  impl_num!(u32le u32 4 Lsb from_le_bytes from_be_bytes to_le_bytes);
  impl_num!(u64le u64 8 Lsb from_le_bytes from_be_bytes to_le_bytes);
  impl_num!(u16be u16 2 Msb from_be_bytes from_le_bytes to_be_bytes);
  impl_num!(u32be u32 4 Msb from_be_bytes from_le_bytes to_be_bytes);
  impl_num!(u64be u64 8 Msb from_be_bytes from_le_bytes to_be_bytes);

  unsafe impl RawData for bool {
    const RAW_LAYOUT: RawLayout = RawLayout::new().with_allowed_bits(0b1);
  }

  unsafe impl RawData for u8 {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for u16 {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for u32 {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for u64 {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for usize {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for Option<NonZeroU8> {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for Option<NonZeroU16> {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for Option<NonZeroU32> {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for Option<NonZeroU64> {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }

  unsafe impl RawData for Option<NonZeroUsize> {
    const RAW_LAYOUT: RawLayout = RawLayout::new();
  }
}

// tests -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use crate::*;

  #[derive(Copy, Clone, Data)]
  #[repr(C)]
  #[data(crate)]
  pub struct Foo<T: ?Sized> {
    a: u8,
    b: u32,
    c: Dropless<T>,
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  #[data(crate)]
  pub struct FooPacked<T: ?Sized> {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: Dropless<T>,
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq, Data)]
  #[repr(C)]
  #[data(crate)]
  pub struct Bar<T: ?Sized> {
    a: [u8; 4],
    b: T,
  }

  #[test]
  fn test_byte_layout() {
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(0));
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(1));
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(2));
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(3));
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(4));
    assert_eq!(ByteLayout::new(0xff), byte_layout::<Bar<[u8]>>(5));

    assert_eq!(Ok(&true), try_transmute_ref::<u8, bool>(&mut 1u8));
    assert_eq!(Ok(&1u8), try_transmute_ref::<bool, u8>(&mut true));
    assert_eq!(Ok(&mut true), try_transmute_mut::<u8, bool>(&mut 1u8));
    assert_eq!(
      // TODO: error message could be better.
      Err(Error::InvalidTransmute(
        "cannot transmute between types of incompatible byte layouts, source padding will left \
        uninitialized byte(s) in target"
      )),
      try_transmute_mut::<bool, u8>(&mut true)
    );
  }

  #[test]
  fn test_transmute() {
    let mut foo = FooPacked::<[u8; 3]> {
      a: 0x42,
      b: u16le::from_le_bytes([0x42, 0x42]),
      c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
      d: Dropless::new([0xaf, 0xaf, 0xaa]),
    };

    let bar: &mut Bar<[u8]> = transmute_mut(&mut foo);

    assert_eq!([0x42; 4], bar.a);
    bar.a = [0xff; 4];

    assert_eq!(0xff, foo.a);
    assert_eq!(u16le::from_le_bytes([0xff, 0xff]), foo.b);
    assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

    pub fn pack<T: ?Sized + Data>(value: &T) -> &[u8] {
      transmute_ref(value)
    }

    pub fn pack_foo(foo: &FooPacked<[u8; 3]>) -> &[u8] {
      pack(foo)
    }

    println!("{:?}", pack_foo(&foo));

    let foo = Foo::<[u8; 3]> { a: 0x42, b: 0x42, c: Dropless::new([0xaf, 0xaf, 0xaa]) };

    let _: &Foo<[u8; 3]> = transmute_ref(&foo);
    let _: Result<&Foo<[u8; 3]>, Error> = try_transmute_ref(&foo);
  }

  #[test]
  fn test_try_transmute() {
    assert_eq!(Ok(&[1, 2]), try_transmute_ref::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(Err(Error::SizeMismatch(1)), try_transmute_ref::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(
      Ok(&Bar { a: [1, 2, 3, 4], b: () }),
      try_transmute_ref::<[u8], Bar<()>>(&[1, 2, 3, 4])
    );
    println!("{:?}", try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    println!(
      "{:?}",
      try_transmute_ref::<Bar<[u8]>, Bar<[u8; 2]>>(
        try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
      )
    );
    println!("{:?}", transmute_ref::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
  }

  #[test]
  fn test_split_at() {
    assert_eq!(([1u8, 2].as_slice(), [3u8, 4].as_slice()), split_at([1u8, 2, 3, 4].as_slice(), 2));

    #[derive(Data)]
    #[repr(C, packed)]
    #[data(crate)]
    struct Foo {
      bar: u8,
      qux_len: u16,
      qux: [u8],
    }

    let foo: &Foo = try_transmute_ref::<[u8], _>(&[1, 4, 0, 4, 5, 6, 7, 8, 9]).unwrap();
    let (foo, rest) = split_at(foo, (3 + foo.qux_len) as usize);
    let _ = (foo, rest);
  }
}
