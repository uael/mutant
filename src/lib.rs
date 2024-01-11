#![feature(offset_of)]
use core::fmt::{Debug, Display, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::num::NonZeroUsize;
use core::{fmt, mem, slice};
use core::ops::{Deref, DerefMut};
use core::mem::offset_of;

pub use mutant_derive::ensure;
pub use mutant_derive::DataType;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Layout<T: ?Sized + DataType> {
  Void,
  Slice, Array,
  Struct { repr: Repr<T, T::Fields> },
  Integer { spec: IntegerSpec },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering {
  Lsb = 1, Msb
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct IntegerSpec {
  ordering: Option<Ordering>,
  bitsmask: Option<u64>,
}

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Repr<T: ?Sized + DataType, F: FieldList<T>> {
  r#type: PhantomData<T>,
  fields: F,
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Field<T: ?Sized + DataType, F: Copy + DataType> {
  r#type: PhantomData<(F, T)>,
  offset: usize,
  factor: Option<NonZeroUsize>,
  expect: Option<u64>,
}

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Undrop<T: ?Sized>(ManuallyDrop<T>);

pub trait Integer: Copy + DataType {
  const SPEC: IntegerSpec;
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

impl IntegerSpec {
  #[inline(always)]
  pub const fn new() -> Self {
    Self { ordering: None, bitsmask: None }
  }

  #[inline(always)]
  pub const fn with_ordering(self, ordering: Ordering) -> Self {
    Self { ordering: Some(ordering), bitsmask: self.bitsmask }
  }

  #[inline(always)]
  pub const fn with_bitsmask(self, bitsmask: u64) -> Self {
    Self { ordering: self.ordering, bitsmask: Some(bitsmask) }
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
      uint |= (byte as u64) << 8 * match self.ordering() {
        Ordering::Lsb => bit,
        Ordering::Msb => bytes.len() - bit,
      };
      bit += 1;
    }
    uint
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

impl<T: ?Sized + DataType, F: Copy + DataType> Clone for Field<T, F> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: ?Sized + DataType, F: Copy + DataType> Copy for Field<T, F> {

}

impl<T: ?Sized + DataType, F: FieldList<T>> Clone for Repr<T, F> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: ?Sized + DataType, F: FieldList<T>> Copy for Repr<T, F> {

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
  pub const fn remaining(self, val: &T) -> usize {
    size_of_val(val) - self.offset - self.size()
  }

  #[inline(always)]
  const fn try_load(self, val: &T) -> Option<u64> {
    let spec = match self.layout() {
      Layout::Integer { spec } => spec,
      _ => return None,
    };

    let bytes = unsafe { slice::from_raw_parts(self.as_ptr(val).cast::<u8>(), self.size()) };
    Some(spec.load(bytes))
  }
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
  Field { r#type: PhantomData, offset, factor, expect }
}

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

#[macro_export]
macro_rules! unaligned_ref {
  (&$($expr:tt).+) => {{
    const fn assert_unaligned<T: ?Sized + DataType>(_: *const T) {
      assert!(align_of::<T>() == 1);
    }

    let _ref: &_ = $crate::unaligned_ref!(__impl@base_of $($expr).+);
    let addr = core::ptr::addr_of!($($expr).+);
    assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &*addr }
  }};
  (&mut $($expr:tt).+) => {{
    const fn assert_unaligned<T: ?Sized + DataType>(_: *mut T) {
      assert!(align_of::<T>() == 1);
    }

    let _ref: &mut _ = $crate::unaligned_ref!(__impl@base_of $($expr).+);
    let addr = core::ptr::addr_of_mut!($($expr).+);
    assert_unaligned(addr);
    // SAFETY:
    // - `_ref` has been checked to be a valid reference hence not a dangling pointer.
    // - The call to `assert_unaligned` ensure field un-alignment.
    unsafe { &mut *addr }
  }};
  (__impl@base_of $base:tt $(.$ex:tt)+) => { $base };
}

// impl Integer ------------------------------------------------------------------------------------

impl Integer for u8 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u16 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u32 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u64 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct u16le([u8; 2]);

impl Integer for u16le {
  const SPEC: IntegerSpec = IntegerSpec::new().with_ordering(Ordering::Lsb);
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct u32le([u8; 4]);

impl Integer for u32le {
  const SPEC: IntegerSpec = IntegerSpec::new().with_ordering(Ordering::Lsb);
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

// impl Unalign ------------------------------------------------------------------------------------

impl<T: Copy + DataType> DataType for Unalign<T> {
  type Item = ();
  type Head = T;
  type Tail = T::Tail;
  type Fields = Field<Self, T>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T>(0, None, None) }) };
}

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

impl<T: Copy + DataType> Deref for Unalign<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unaligned_ref!(&self.0)
  }
}

impl<T: Copy + DataType> DerefMut for Unalign<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unaligned_ref!(&mut self.0)
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

// impl Undrop -------------------------------------------------------------------------------------

impl<T: ?Sized + DataType> DataType for Undrop<T> {
  type Item = ();
  type Head = T::Head;
  type Tail = T::Tail;
  type Fields = Field<Self, T::Head>;
  const LAYOUT: Layout<Self> = Layout::Struct { repr: Repr::new(unsafe { field::<Self, T::Head>(0, None, None) }) };
}

impl<T: ?Sized> Undrop<T> {
  const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
}

impl<T> Undrop<T> {
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

impl<T: ?Sized> Deref for Undrop<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    self.0.deref()
  }
}

impl<T: ?Sized> DerefMut for Undrop<T> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.0.deref_mut()
  }
}

impl<T: Display + ?Sized> Display for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Self::ASSERT_NOT_DROP;
    Display::fmt(unsafe { mem::transmute::<_, &T>(self) }, f)
  }
}

impl<T: Debug + ?Sized> Debug for Undrop<T> {
  #[inline(always)]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let _ = Self::ASSERT_NOT_DROP;
    Debug::fmt(unsafe { mem::transmute::<_, &T>(self) }, f)
  }
}

// mod layout --------------------------------------------------------------------------------------

pub mod layout {
  use super::*;

  #[inline]
  pub const fn transmute<T: ?Sized + DataType, U: ?Sized + DataType>(metadata: usize) -> usize {
    let _ = Transmute::<T, U>::ASSERT_INFALLIBLE;
    unsafe { transmute_unchecked::<T, U>(metadata) }
  }

  #[inline]
  pub const fn try_transmute<T: ?Sized + DataType, U: ?Sized + DataType>(
    metadata: usize,
  ) -> Result<usize, isize> {
    match Transmute::<T, U>::REQUIREMENTS.delta(metadata) {
      0 => Ok(unsafe { transmute_unchecked::<T, U>(metadata) }),
      d => Err(d),
    }
  }

  #[inline]
  pub const unsafe fn transmute_unchecked<T: ?Sized + DataType, U: ?Sized + DataType>(
    metadata: usize,
  ) -> usize {
    match size_of_tail_item::<U>() {
      0 => 0,
      bt => {
        let size = min_size_of::<T>() + metadata * size_of_tail_item::<T>();
        (size - min_size_of::<U>()) / bt
      }
    }
  }

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
    const fn of<T: ?Sized + DataType, U: ?Sized + DataType>() -> Self {
      let (ah, at, bh, bt) = (
        min_size_of::<T>(),
        NonZeroUsize::new(size_of_tail_item::<T>()),
        min_size_of::<U>(),
        NonZeroUsize::new(size_of_tail_item::<U>()),
      );
      let mut min_at_count = 0; // Minimum number of `at` required to align `ah[at]` and `bh[bt]`.
      if ah < bh {
        let at = match at {
          Some(tail) => tail.get(),
          _ => panic!("cannot transmute between types of different sizes, source too small"),
        };
        let missing = bh - ah;
        if bt.is_none() {
          assert!(
            missing % at == 0,
            "cannot transmute between types of incompatible byte layouts, \
            missing bytes to fit the target is not a multiple of the source `DST` \
            trailing slice item size"
          );
        }
        min_at_count = (missing + at - 1) / at; // Ceiling of division.
      } else if ah > bh {
        let bt = match bt {
          Some(tail) => tail.get(),
          _ => panic!("cannot transmute between types of different sizes, target too small"),
        };
        let remaining = ah - bh;
        if at.is_none() {
          assert!(
            remaining % bt == 0,
            "cannot transmute between types of incompatible byte layouts, \
            remaining bytes to fit in the target is not a multiple of the target `DST` \
            trailing slice item size"
          )
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
              panic!(
                "cannot transmute between `DST` types of incompatible byte layouts, \
                source cannot catchup on target"
              )
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
      let at = size_of_tail_item::<T>();
      let minimum_size = match requirements {
        Self::Is(n) | Self::IsAtLeast(n) => ah + (n * at),
        Self::IsMultipleOf(m) => ah + (m * at),
        Self::IsAtLeastThenMultipleOf(n, _) => ah + (n * at),
        Self::IsEmpty | Self::None => ah,
      };
      let mut offset = 0;
      while offset < minimum_size {
        if !maybe_uninit::<U>(offset) &&
          maybe_uninit::<T>(offset) {
          panic!(
            "cannot transmute between types of incompatible byte layouts, \
            source padding will left uninitialized byte(s) in target"
          );
        }
        offset += 1;
      }
      requirements
    }
  }

  struct Transmute<T: ?Sized, U: ?Sized>(PhantomData<T>, PhantomData<U>);

  impl<T: ?Sized + DataType, U: ?Sized + DataType> Transmute<T, U> {
    pub const REQUIREMENTS: Requirements = {
      // TODO:
      // if let Some(spec) = U::TAG_SPEC {
      //   assert!(spec.bits > 0 && spec.bits <= 64);
      //   assert!(min_size_of::<U>() >= spec.offset + (spec.bits as usize / 8))
      // }
      // if let Some(spec) = U::SIZE_SPEC {
      //   assert!(spec.bits > 0 && spec.bits <= 64);
      //   assert!(min_size_of::<U>() >= spec.offset + (spec.bits as usize / 8))
      // }
      Requirements::of::<T, U>()
    };
    pub const ASSERT_INFALLIBLE: () = match Self::REQUIREMENTS {
      Requirements::Is(_) => panic!(
        "cannot transmute between types of incompatible byte layouts, \
        source too small; \
        use `try_transmute` instead"
      ),
      Requirements::IsAtLeast(_) => panic!(
        "cannot transmute between types of incompatible byte layouts, \
        target too small; \
        use `try_transmute` instead"
      ),
      Requirements::IsMultipleOf(_) => panic!(
        "cannot transmute between types of incompatible byte layouts, \
        tail size multiple of mismatch; \
        use `try_transmute` instead"
      ),
      Requirements::IsAtLeastThenMultipleOf(_, _) => panic!(
        "cannot transmute between types of incompatible byte layouts, \
        target too small or tail size multiple of mismatch; \
        use `try_transmute` instead"
      ),
      Requirements::IsEmpty => panic!(
        "cannot transmute between types of incompatible byte layouts, \
        source tail might not be empty; \
        use `try_transmute` instead"
      ),
      // TODO:
      // Requirements::None => assert!(
      //   U::TAG_SPEC.is_none() && U::SIZE_SPEC.is_none(),
      //   "transmute is fallible; use `try_transmute` instead"
      // ),
      Requirements::None => (),
    };
  }

  #[test]
  fn test_infallible() {
    let _ = Transmute::<[u8], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[u8; 2], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 5]], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 4]], [[u8; 2]]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<tests::Foo<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<tests::Bar<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<tests::Foo<[u8]>, tests::Bar<[u8]>>::ASSERT_INFALLIBLE;
  }

  #[test]
  fn test_nouninit() {
    use Requirements::*;

    #[derive(Debug, Copy, Clone, DataType)]
    #[repr(C, packed)]
    #[mutant(crate)]
    pub struct BazPacked {
      a: u8,
      b: u32,
      c: [u8; 6],
    }

    for offset in 0..mem::size_of::<BazPacked>() {
      assert_eq!(false, maybe_uninit::<BazPacked>(offset), "at {}", offset);
    }

    #[derive(Debug, Copy, Clone, DataType)]
    #[mutant(crate)]
    pub struct Baz(u8, u32, [u8; 6]);

    assert_eq!(None, Requirements::of::<BazPacked, [u8]>());
    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      source padding will left uninitialized byte(s) in target",
      Requirements::of::<Baz, [u8]>()
    );
  }

  #[test]
  fn test_requirements() {
    use Requirements::*;

    #[derive(DataType)]
    #[repr(packed)]
    #[mutant(crate)]
    struct Layout<const H: usize, const T: usize = 0>([u8; H], [[u8; T]]);

    // Basic tests for equal head and tail sizes
    assert_eq!(None, Requirements::of::<Layout<1>, Layout<1>>());
    assert_eq!(None, Requirements::of::<Layout<42>, Layout<42>>());
    assert_eq!(None, Requirements::of::<Layout<42, 1>, Layout<42, 1>>());

    // Test for equal head sizes with different tail sizes
    assert_eq!(None, Requirements::of::<Layout<42, 3>, Layout<42, 1>>());
    assert_eq!(IsMultipleOf(10), Requirements::of::<Layout<50, 10>, Layout<50, 100>>());
    assert_eq!(IsMultipleOf(20), Requirements::of::<Layout<50, 5>, Layout<50, 100>>());

    // Test cases for transmuting to and from empty tail
    assert_eq!(IsEmpty, Requirements::of::<Layout<42, 1>, Layout<42>>());
    assert_eq!(Is(7), Requirements::of::<Layout<35, 1>, Layout<42>>());

    // Test cases for large difference in head sizes
    assert_eq!(Is(80), Requirements::of::<Layout<20, 1>, Layout<100>>());

    // Test cases for tail sizes as multiples
    assert_eq!(IsMultipleOf(2), Requirements::of::<Layout<42, 1>, Layout<42, 2>>());
    assert_eq!(IsMultipleOf(3), Requirements::of::<Layout<42, 1>, Layout<42, 3>>());
    assert_eq!(IsMultipleOf(3), Requirements::of::<Layout<42, 2>, Layout<42, 3>>());
    assert_eq!(IsMultipleOf(3), Requirements::of::<Layout<42, 4>, Layout<42, 3>>());
    assert_eq!(IsMultipleOf(8), Requirements::of::<Layout<42, 5>, Layout<42, 8>>());
    assert_eq!(IsMultipleOf(3), Requirements::of::<Layout<42, 2>, Layout<42, 6>>());

    // Test zero heads
    assert_eq!(Is(42), Requirements::of::<Layout<0, 1>, Layout<42>>());
    assert_eq!(IsAtLeast(42), Requirements::of::<Layout<0, 1>, Layout<42, 1>>());

    // Test cases for alignment and complex transmutations
    assert_eq!(IsAtLeastThenMultipleOf(2, 3), Requirements::of::<Layout<42, 1>, Layout<38, 3>>());
    assert_eq!(IsAtLeastThenMultipleOf(1, 3), Requirements::of::<Layout<42, 2>, Layout<38, 3>>());
    assert_eq!(IsAtLeastThenMultipleOf(1, 3), Requirements::of::<Layout<43, 2>, Layout<39, 3>>());
    assert_eq!(IsMultipleOf(2), Requirements::of::<Layout<43, 2>, Layout<39, 4>>());
    assert_eq!(IsAtLeast(2), Requirements::of::<Layout<1, 2>, Layout<5, 2>>());
    assert_eq!(IsAtLeastThenMultipleOf(1, 3), Requirements::of::<Layout<45, 2>, Layout<38, 3>>());
    assert_eq!(IsAtLeastThenMultipleOf(2, 4), Requirements::of::<Layout<40, 1>, Layout<38, 4>>());
    assert_eq!(IsAtLeastThenMultipleOf(2, 4), Requirements::of::<Layout<44, 5>, Layout<38, 4>>());
    assert_eq!(IsAtLeast(1), Requirements::of::<Layout<38, 3>, Layout<41, 3>>());

    // Panic test cases for incompatible transmutations
    assert_panic!(
      "cannot transmute between types of different sizes, target too small",
      Requirements::of::<Layout<2>, Layout<1>>()
    );
    assert_panic!(
      "cannot transmute between types of different sizes, source too small",
      Requirements::of::<Layout<1>, Layout<2>>()
    );
    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
      item size",
      Requirements::of::<Layout<35, 2>, Layout<42>>()
    );
    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
      item size",
      Requirements::of::<Layout<40, 3>, Layout<42>>()
    );
    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      remaining bytes to fit in the target is not a multiple of the target `DST` \
      trailing slice item size",
      Requirements::of::<Layout<42>, Layout<38, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<42, 3>, Layout<38, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<41, 3>, Layout<39, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<38, 3>, Layout<42, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<39, 3>, Layout<41, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<39, 3>, Layout<41, 6>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<39, 6>, Layout<41, 3>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<39, 6>, Layout<41, 9>>()
    );
    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      Requirements::of::<Layout<39, 3>, Layout<41, 9>>()
    );
  }
}

// mod ptr -----------------------------------------------------------------------------------------

mod ptr {
  use super::*;

  /// Extract the metadata component of a pointer.
  #[inline]
  pub(super) const fn metadata<T: ?Sized>(ptr: *const T) -> usize {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline]
  pub(super) const fn from_raw_parts<T: ?Sized>(address: *const (), metadata: usize) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline]
  pub(super) const fn from_raw_parts_mut<T: ?Sized>(address: *mut (), metadata: usize) -> *mut T {
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

// tests -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  #[macro_export]
  macro_rules! layout {
    ($T:ty { $($field:ident: $field_ty:ty),+ $(,)? }) => {{
      let mut layout: $T = ::core::mem::zeroed();
      $(layout.$field = $crate::layout_of::<$field_ty>();)+
      $crate:Layoutt::new(layout)
    }};
  }

  #[macro_export]
  macro_rules! assert_panic {
    ($msg:expr, $e:expr) => {
      assert_eq!(
        Some($msg),
        std::panic::catch_unwind(|| { $e })
          .err()
          .and_then(|a| a.downcast_ref::<&'static str>().cloned())
      )
    };
  }

  #[derive(Copy, Clone, DataType)]
  #[repr(C, packed)]
  #[mutant(crate)]
  pub struct Foo<T: ?Sized> {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: Undrop<T>,
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq, DataType)]
  #[repr(C)]
  #[mutant(crate)]
  pub struct Bar<T: ?Sized> {
    a: [u8; 4],
    b: T,
  }

  #[test]
  fn test_transmute() {
    let mut foo = Foo::<[u8; 3]> {
      a: 0x42,
      b: u16le([0x42, 0x42]),
      c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
      d: Undrop::new([0xaf, 0xaf, 0xaa]),
    };

    let bar: &mut Bar<[u8]> = cast_mut(&mut foo);

    assert_eq!([0x42; 4], bar.a);
    bar.a = [0xff; 4];

    assert_eq!(0xff, foo.a);
    assert_eq!(u16le([0xff, 0xff]), foo.b);
    assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

    pub fn pack<T: ?Sized + DataType>(value: &T) -> &[u8] {
      cast(value)
    }

    pub fn pack_foo(foo: &Foo<[u8; 3]>) -> &[u8] {
      pack(foo)
    }

    println!("{:?}", pack_foo(&foo));
  }

  #[test]
  fn test_try_transmute() {
    assert_eq!(Ok(&[1, 2]), try_cast::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(Err(1), try_cast::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(Ok(&Bar { a: [1, 2, 3, 4], b: () }), try_cast::<[u8], Bar<()>>(&[1, 2, 3, 4]));
    println!("{:?}", try_cast::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    // println!(
    //   "{:?}",
    //   try_cast::<Bar<[u8]>, Bar<[u8; 2]>>(
    //     try_cast::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
    //   )
    // );
    println!("{:?}", cast::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
  }

  pub mod unsync {
    use core::cell::Cell;
    use core::future::{poll_fn, Future};
    use core::pin::pin;
    use core::task::Poll;
    use mutant_derive::ensure;

    pub async fn yield_once() {
      let mut pend = Some(());
      poll_fn(move |_| match pend.take() {
        None => Poll::Ready(()),
        Some(_) => Poll::Pending,
      })
      .await
    }

    pub async fn poll_once<F: Future>(f: F) -> Option<F::Output> {
      let mut f = pin!(f);
      poll_fn(move |cx| match f.as_mut().poll(cx) {
        Poll::Ready(value) => Poll::Ready(Some(value)),
        _ => Poll::Ready(None),
      })
      .await
    }

    #[repr(transparent)]
    pub struct Slot<T>(Cell<Option<T>>);

    impl<T> Slot<T> {
      pub const fn new(value: T) -> Self {
        Self(Cell::new(Some(value)))
      }

      pub const fn uninit() -> Self {
        Self(Cell::new(None))
      }

      pub fn is_set(&self) -> bool {
        match self.take_nowait() {
          Some(value) => {
            self.set_nowait(value);
            true
          }
          _ => false,
        }
      }

      pub fn set_nowait(&self, value: T) -> Option<T> {
        self.0.replace(Some(value))
      }

      pub fn take_nowait(&self) -> Option<T> {
        self.0.take()
      }

      pub async fn set(&self, value: T) {
        while self.is_set() {
          yield_once().await;
        }
        let none = self.set_nowait(value);
        ensure!(none.is_none());
      }

      pub async fn take(&self) -> T {
        loop {
          yield_once().await;
          if let Some(value) = self.take_nowait() {
            break value;
          }
        }
      }
    }

    impl<T: Copy> Slot<T> {
      pub fn get_nowait(&self) -> Option<T> {
        self.0.get()
      }

      pub async fn get(&self) -> T {
        loop {
          yield_once().await;
          if let Some(value) = self.get_nowait() {
            break value;
          }
        }
      }
    }

    impl<T> Default for Slot<T> {
      fn default() -> Self {
        Self(Cell::default())
      }
    }
  }

  pub mod hci {
    use super::*;

    pub trait Socket {
      async fn recv(&self) -> &Packet<[u8]>;
      async fn send<T: ?Sized + DataType>(&self, tx: &Packet<T>);
    }

    #[derive(Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct Packet<T: ?Sized> {
      pub r#type: u8,
      pub data: Undrop<T>,
    }

    impl<T: ?Sized + Debug + DataType> Debug for Packet<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Packet")
          .field("type", &self.r#type)
          .field("data", &unaligned_ref!(&self.data))
          .finish()
      }
    }

    #[derive(Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct Acl<T: ?Sized> {
      pub handle_and_flags: Unalign<u16>,
      pub data_total_len: Unalign<u16>,
      pub parameters: Undrop<T>,
    }

    impl<T: ?Sized + Debug + DataType> Debug for Acl<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Acl")
          .field("handle_and_flags", &self.handle_and_flags)
          .field("data_total_len", &self.data_total_len)
          .field("completed_packets", &unaligned_ref!(&self.parameters))
          .finish()
      }
    }

    #[derive(Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct Event<T: ?Sized + EventParameters> {
      #[mutant(expect(T::EVENT_CODE))]
      pub event_code: u8,
      #[mutant(size)]
      pub parameters_len: u8,
      pub parameters: Undrop<T>,
    }

    pub trait EventParameters: DataType {
      const EVENT_CODE: Option<u8>;
    }

    impl EventParameters for [u8] {
      const EVENT_CODE: Option<u8> = None;
    }

    impl EventParameters for ConnectionComplete {
      const EVENT_CODE: Option<u8> = Some(21);
    }

    impl EventParameters for DisconnectionComplete {
      const EVENT_CODE: Option<u8> = Some(22);
    }

    impl EventParameters for NumberOfCompletedPackets {
      const EVENT_CODE: Option<u8> = Some(23);
    }

    #[derive(Debug, Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct ConnectionComplete {
      pub address: [u8; 6],
      pub handle: u16,
      pub link_type: u8,
    }

    #[derive(Debug, Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct DisconnectionComplete {
      pub address: [u8; 6],
      pub handle: u16,
    }

    #[derive(Debug, Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct CompletedPackets {
      pub handle: u16,
      pub number_of_completed_packets: u16,
    }

    #[derive(DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct NumberOfCompletedPackets {
      pub num_handles: u8,
      pub completed_packets: [CompletedPackets],
    }

    #[derive(Copy, Clone, DataType)]
    #[mutant(crate)]
    #[repr(C, packed)]
    pub struct Command<T: ?Sized + DataType> {
      pub op_code: u16,
      pub parameters_len: u8,
      pub parameters: Undrop<T>,
    }

    impl<T: ?Sized + Debug + EventParameters> Debug for Event<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
          .field("event_code", &self.event_code)
          .field("parameters_len", &self.parameters_len)
          .field("parameters", &unaligned_ref!(&self.parameters))
          .finish()
      }
    }

    impl Debug for NumberOfCompletedPackets {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("NumberOfCompletedPackets")
          .field("num_handles", &self.num_handles)
          .field("completed_packets", &unaligned_ref!(&self.completed_packets))
          .finish()
      }
    }
  }

  pub mod host {
    use super::*;
    use core::future::Future;
    use core::pin::pin;

    pub struct Host<'rx, L: hci::Socket>
    where
      Self: 'rx,
    {
      lower: L,
      rx: unsync::Slot<&'rx hci::Packet<[u8]>>,
    }

    impl<'rx, L: hci::Socket> hci::Socket for Host<'rx, L> {
      async fn recv(&self) -> &hci::Packet<[u8]> {
        self.rx.take().await
      }

      async fn send<T: ?Sized + DataType>(&self, tx: &hci::Packet<T>) {
        // TODO: flow-control
        self.lower.send(tx).await
      }
    }

    impl<'rx, L: hci::Socket> Host<'rx, L> {
      pub async fn get<T: ?Sized + DataType>(&self) -> &T {
        loop {
          match self.rx.get().await.try_cast::<hci::Packet<T>>() {
            Ok(value) => break unaligned_ref!(&value.data),
            _ => {}
          }
        }
      }

      pub async fn get_event<T: ?Sized + hci::EventParameters>(&self) -> &T {
        loop {
          match self.rx.get().await.try_cast::<hci::Packet<hci::Event<T>>>() {
            Ok(value) => {
              break {
                let data = unaligned_ref!(&value.data);
                unaligned_ref!(&data.parameters)
              }
            }
            _ => {}
          }
        }
      }
    }

    pub async fn host<F: Future, H: hci::Socket>(
      hci: H,
      f: impl FnOnce(&Host<'_, H>) -> F,
    ) -> F::Output {
      let host = Host { lower: hci, rx: Default::default() };
      let mut f = pin!(f(&host));
      loop {
        // Poll `f` once.
        if let Some(value) = unsync::poll_once(f.as_mut()).await {
          break value;
        }
        host.rx.set(host.lower.recv().await).await;
      }
    }

    pub async fn accept<T: hci::Socket, F: Future>(
      host: &Host<'_, T>,
      f: impl FnOnce(u16) -> F,
    ) -> Option<F::Output> {
      let connection_handle = host.get_event::<hci::ConnectionComplete>().await.handle;
      let mut f = pin!(f(connection_handle));
      loop {
        // Poll `f` once.
        if let Some(value) = unsync::poll_once(f.as_mut()).await {
          break Some(value);
        }
        let disconnection = host.get_event::<hci::DisconnectionComplete>().await;
        if disconnection.handle == connection_handle {
          break None;
        }
      }
    }
  }

  #[test]
  fn test_hci() {
    println!(
      "{:?}",
      try_cast::<[u8], hci::Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
    );
    println!(
      "{:?}",
      try_cast::<[u8], hci::Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3])
    );
    println!(
      "{:?}",
      try_cast::<[u8], hci::Event<hci::ConnectionComplete>>(&[
        21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
      ])
    );
    println!(
      "{:?}",
      try_cast::<[u8], hci::Event<hci::DisconnectionComplete>>(&[
        22, 9, 69, 69, 69, 69, 69, 69, 69, 69
      ])
    );
    println!(
      "{:?}",
      try_cast::<[u8], hci::Event<hci::ConnectionComplete>>(&[
        22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
      ])
    );
    println!(
      "{:?}",
      try_cast::<[u8], hci::Packet<hci::Event<[u8]>>>(&[
        2, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
      ])
    );
    println!(
      "{:#?}",
      try_cast::<[u8], hci::Packet<hci::Event<hci::NumberOfCompletedPackets>>>(&[
        2, 23, 9, 2, 1, 0, 2, 0, 3, 0, 4, 0
      ])
    );
  }
}
