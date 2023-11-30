use core::fmt::{Debug, Formatter};
use core::marker::{PhantomData, PhantomPinned};
use core::mem::{self, ManuallyDrop};
use core::num::{
  NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU128,
  NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize,
};
use core::slice;

// Re-export derive crate.
use crate::Error;
pub use mutant_derive::*;

pub use crate::Partition;
pub use crate::Unalign;

pub unsafe trait Zeroable: Sized {
  /// Calls [`zeroed`](mem::zeroed).
  ///
  /// This is a trait method so that you can write `MyType::zeroed()` in your
  /// code. It is a contract of this trait that if you implement it on your type
  /// you **must not** override this method.
  #[inline]
  fn zeroed() -> Self {
    unsafe { mem::zeroed() }
  }
}

pub trait Size: Sized {
  #[inline(always)]
  fn size(&self) -> usize {
    mem::size_of::<Self>()
  }
}

pub trait Swab: Sized {
  #[inline(always)]
  fn swab(bytes: &[u8]) -> Option<Partition<'_>> {
    Partition::from_split(bytes, mem::size_of::<Self>()).ok()
  }

  #[inline(always)]
  fn swab_mut(bytes: &mut [u8]) -> Option<ChunksMut<'_>> {
    unsafe { mem::transmute(Self::swab(bytes)) }
  }

  #[inline(always)]
  fn swab_size(bytes: &[u8]) -> usize {
    Self::swab(bytes).map(|x| x.validated.len()).unwrap_or(0)
  }
}

pub unsafe trait Pack: Size {
  #[inline(always)]
  fn pack(&self) -> &[u8] {
    if self.size() == 0 {
      &[]
    } else {
      unsafe { slice::from_raw_parts(self as *const Self as *const u8, self.size()) }
    }
  }
}

pub unsafe trait PackMut: Pack {
  #[inline(always)]
  fn pack_mut(&mut self) -> &mut [u8] {
    if self.size() == 0 {
      &mut []
    } else {
      unsafe { slice::from_raw_parts_mut(self as *mut Self as *mut u8, self.size()) }
    }
  }
}

pub unsafe trait UnpackUnaligned: Swab {
  #[inline(always)]
  fn unpack_unaligned(bytes: &[u8]) -> Option<&Self> {
    Self::unpack_unaligned_prefix(bytes).map(|(x, _)| x)
  }

  #[inline(always)]
  fn unpack_unaligned_prefix(bytes: &[u8]) -> Option<(&Self, &[u8])> {
    const { assert!(mem::align_of::<Self>() == 1) };
    Self::swab(bytes).map(|x| (unsafe { &*(x.validated.as_ptr() as *const Self) }, x.remaining))
  }
}

pub unsafe trait UnpackUnalignedMut: UnpackUnaligned {
  #[inline(always)]
  fn unpack_unaligned_mut(bytes: &mut [u8]) -> Option<&mut Self> {
    Self::unpack_unaligned_prefix_mut(bytes).map(|(x, _)| x)
  }

  #[inline(always)]
  fn unpack_unaligned_prefix_mut(bytes: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
    const { assert!(mem::align_of::<Self>() == 1) };
    Self::swab_mut(bytes).map(|x| (unsafe { &mut *(x.swab.as_mut_ptr() as *mut Self) }, x.rest))
  }
}

pub unsafe trait Unpack: Swab {
  #[inline(always)]
  fn unpack(bytes: &[u8]) -> Option<&Self> {
    Self::unpack_prefix(bytes).map(|(x, _)| x)
  }

  #[inline(always)]
  fn unpack_prefix(bytes: &[u8]) -> Option<(&Self, &[u8])> {
    if aligned::<Self>(bytes.as_ptr() as *const ()) {
      Self::swab(bytes).map(|x| (unsafe { &*(x.validated.as_ptr() as *const Self) }, x.remaining))
    } else {
      None
    }
  }
}

pub unsafe trait UnpackMut: Unpack {
  #[inline(always)]
  fn unpack_mut(bytes: &mut [u8]) -> Option<&mut Self> {
    Self::unpack_prefix_mut(bytes).map(|(x, _)| x)
  }

  #[inline(always)]
  fn unpack_prefix_mut(bytes: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
    unsafe { mem::transmute(Self::unpack_prefix(bytes)) }
  }
}

unsafe impl<T: UnpackUnaligned> Unpack for T {
  #[inline(always)]
  fn unpack(bytes: &[u8]) -> Option<&Self> {
    Self::unpack_unaligned(bytes)
  }

  #[inline(always)]
  fn unpack_prefix(bytes: &[u8]) -> Option<(&Self, &[u8])> {
    Self::unpack_unaligned_prefix(bytes)
  }
}

unsafe impl<T: UnpackUnalignedMut> UnpackMut for T {
  #[inline(always)]
  fn unpack_mut(bytes: &mut [u8]) -> Option<&mut Self> {
    Self::unpack_unaligned_mut(bytes)
  }

  #[inline(always)]
  fn unpack_prefix_mut(bytes: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
    Self::unpack_unaligned_prefix_mut(bytes)
  }
}

#[inline]
pub fn aligned<T>(ptr: *const ()) -> bool {
  mem::align_of::<T>() == 1 || ptr.align_offset(mem::align_of::<T>()) == 0
}

pub struct ChunksMut<'a> {
  swab: &'a mut [u8],
  rest: &'a mut [u8],
}

impl<'a> ChunksMut<'a> {
  #[inline(always)]
  pub fn from_prefix(buf: &'a mut [u8], size: usize) -> Option<Self> {
    if size <= buf.len() {
      let (swab, rest) = buf.split_at_mut(size);
      Some(Self { swab, rest })
    } else {
      None
    }
  }
}

/// No padding bytes.
pub unsafe trait Packed: Sized + 'static {}

/// Any bits pattern is valid, can contains padding.
pub unsafe trait AnyBitPattern: Zeroable + 'static {}

pub unsafe trait CheckedBitPattern {
  fn size_into(raw: &[u8]) -> Option<usize>;
  fn size(&self) -> usize;
}

pub trait IntoPacked {
  type Packed: Packed;

  fn into_packed(self) -> Self::Packed;
}

/// Re-interprets `&T` as `&[u8]`.
///
/// Any ZST becomes an empty slice, and in that case the pointer value of that
/// empty slice might not match the pointer value of the input reference.
#[inline(always)]
pub const fn pack<T: Packed>(t: &T) -> &[u8] {
  if mem::size_of::<T>() == 0 {
    &[]
  } else {
    unsafe { slice::from_raw_parts(t as *const T as *const u8, mem::size_of::<T>()) }
  }
}

/// Re-interprets `&mut T` as `&mut [u8]`.
///
/// Any ZST becomes an empty slice, and in that case the pointer value of that
/// empty slice might not match the pointer value of the input reference.
#[inline(always)]
pub fn pack_mut<T: Packed + AnyBitPattern>(t: &mut T) -> &mut [u8] {
  if mem::size_of::<T>() == 0 {
    &mut []
  } else {
    unsafe { slice::from_raw_parts_mut(t as *mut T as *mut u8, mem::size_of::<T>()) }
  }
}

/// Re-interprets `&[u8; N]` as `&T` and return the remaining bytes.
#[inline(always)]
pub const fn unpack<T: AnyBitPattern, const N: usize>(s: &[u8; N]) -> (&T, &[u8]) {
  const { assert!(mem::align_of::<T>() == 1) };
  const { assert!(mem::size_of::<T>() <= N) };

  let (s, rest) = s.split_at(mem::size_of::<T>());
  (unsafe { &*(s.as_ptr() as *const T) }, rest)
}

/// Re-interprets `&mut [u8; N]` as `&mut T` and return the remaining bytes.
#[inline(always)]
pub fn unpack_mut<T: Packed + AnyBitPattern, const N: usize>(
  s: &mut [u8; N],
) -> (&mut T, &mut [u8]) {
  const { assert!(mem::align_of::<T>() == 1) };
  const { assert!(mem::size_of::<T>() <= N) };

  let (s, rest) = s.split_at_mut(mem::size_of::<T>());
  (unsafe { &mut *(s.as_mut_ptr() as *mut T) }, rest)
}

/// Re-interprets `&[u8; N]` as `&T`.
#[inline(always)]
pub const fn unpack_exact<T: AnyBitPattern, const N: usize>(s: &[u8; N]) -> &T {
  const { assert!(mem::align_of::<T>() == 1) };
  const { assert!(mem::size_of::<T>() == N) };

  unsafe { &*(s.as_ptr() as *const T) }
}

/// Re-interprets `&mut [u8; N]` as `&mut T`.
#[inline(always)]
pub fn unpack_exact_mut<T: Packed + AnyBitPattern, const N: usize>(s: &mut [u8; N]) -> &mut T {
  const { assert!(mem::align_of::<T>() == 1) };
  const { assert!(mem::size_of::<T>() == N) };

  unsafe { &mut *(s.as_mut_ptr() as *mut T) }
}

/// Attempt to re-interprets `&[u8; N]` as `&T` and return the remaining bytes.
#[inline(always)]
pub fn try_unpack<T: CheckedBitPattern>(s: &[u8]) -> Option<(&T, &[u8])> {
  const { assert!(mem::align_of::<T>() == 1) };

  if let Some(width) = T::size_into(s) {
    if width > s.len() {
      return None;
    }
    let (s, rest) = s.split_at(width);
    return Some((unsafe { &*(s.as_ptr() as *const T) }, rest));
  }

  None
}

/// Attempt to re-interprets `&mut [u8; N]` as `&mut T` and return the remaining bytes.
#[inline(always)]
pub fn try_unpack_mut<T: Packed + CheckedBitPattern>(s: &mut [u8]) -> Option<(&mut T, &mut [u8])> {
  const { assert!(mem::align_of::<T>() == 1) };

  if let Some(width) = T::size_into(s) {
    if width > s.len() {
      return None;
    }
    let (s, rest) = s.split_at_mut(width);
    return Some((unsafe { &mut *(s.as_mut_ptr() as *mut T) }, rest));
  }

  None
}

/// Attempt to re-interprets `&[u8]` as `&T`.
#[inline(always)]
pub fn try_unpack_exact<T: CheckedBitPattern>(s: &[u8]) -> Option<&T> {
  const { assert!(mem::align_of::<T>() == 1) };

  if let Some(width) = T::size_into(s) {
    if width > s.len() {
      return None;
    }
    return Some(unsafe { &*(s[..width].as_ptr() as *const T) });
  }

  None
}

/// Attempt to re-interprets `&mut [u8]` as `&mut T`.
#[inline(always)]
pub fn try_unpack_exact_mut<T: Packed + CheckedBitPattern>(s: &mut [u8]) -> Option<&mut T> {
  const { assert!(mem::align_of::<T>() == 1) };

  if let Some(width) = T::size_into(s) {
    if width > s.len() {
      return None;
    }
    return Some(unsafe { &mut *(s[..width].as_mut_ptr() as *mut T) });
  }

  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::fmt::{Debug, Formatter};
  use core::ops::{Deref, DerefMut};

  trait Enum: CheckedBitPattern {
    type Tag: PartialEq = u8;
    fn tag(&self) -> Self::Tag;
  }

  trait Variant<E: Enum>: Packed {
    const TAG: E::Tag;

    fn as_ref(&self) -> &E {
      unsafe { mem::transmute(self) }
    }

    fn try_from_enumerable(e: &E) -> Option<&Self> {
      if e.tag() == Self::TAG {
        Some(unsafe { mem::transmute(e) })
      } else {
        None
      }
    }
  }

  trait Rack<T: CheckedBitPattern>: Packed {
    fn iter(&self) -> Iter<'_, T> {
      Iter { raw: pack(self), r#type: Default::default() }
    }

    fn iter_mut(&mut self) -> IterMut<'_, T>
    where
      T: Packed,
      Self: AnyBitPattern,
    {
      IterMut { raw: pack_mut(self), r#type: Default::default() }
    }
  }

  trait IntoRack<T: Enum> {
    type Rack: Rack<T>;
    fn rack(self) -> Self::Rack;
  }

  impl<T: Enum, A: Variant<T>, B: Variant<T>> IntoRack<T> for (A, B) {
    type Rack = Rack2<A, B>;

    fn rack(self) -> Self::Rack {
      Rack2(self.0, self.1)
    }
  }

  impl<T: Enum, A: Variant<T>, B: Variant<T>, C: Variant<T>> IntoRack<T> for (A, B, C) {
    type Rack = Rack3<A, B, C>;

    fn rack(self) -> Self::Rack {
      Rack3(self.0, self.1, self.2)
    }
  }

  impl<T: Enum, A: Variant<T>, B: Variant<T>, C: Variant<T>, D: Variant<T>> IntoRack<T>
    for (A, B, C, D)
  {
    type Rack = Rack4<A, B, C, D>;

    fn rack(self) -> Self::Rack {
      Rack4(self.0, self.1, self.2, self.3)
    }
  }

  #[repr(C, packed)]
  #[derive(Copy, Clone)]
  struct Rack2<A, B>(A, B);

  #[repr(C, packed)]
  #[derive(Copy, Clone)]
  struct Rack3<A, B, C>(A, B, C);

  #[repr(C, packed)]
  #[derive(Copy, Clone)]
  struct Rack4<A, B, C, D>(A, B, C, D);

  unsafe impl<A: Packed, B: Packed> Packed for Rack2<A, B> {}
  unsafe impl<A: Packed, B: Packed, C: Packed> Packed for Rack3<A, B, C> {}
  unsafe impl<A: Packed, B: Packed, C: Packed, D: Packed> Packed for Rack4<A, B, C, D> {}

  unsafe impl<A: Zeroable, B: Zeroable> Zeroable for Rack2<A, B> {}
  unsafe impl<A: Zeroable, B: Zeroable, C: Zeroable> Zeroable for Rack3<A, B, C> {}
  unsafe impl<A: Zeroable, B: Zeroable, C: Zeroable, D: Zeroable> Zeroable for Rack4<A, B, C, D> {}

  unsafe impl<A: AnyBitPattern, B: AnyBitPattern> AnyBitPattern for Rack2<A, B> {}
  unsafe impl<A: AnyBitPattern, B: AnyBitPattern, C: AnyBitPattern> AnyBitPattern for Rack3<A, B, C> {}
  unsafe impl<A: AnyBitPattern, B: AnyBitPattern, C: AnyBitPattern, D: AnyBitPattern> AnyBitPattern
    for Rack4<A, B, C, D>
  {
  }

  impl<E: Enum> Rack<E> for () {}
  impl<E: Enum, A: Variant<E>, B: Variant<E>> Rack<E> for Rack2<A, B> {}
  impl<E: Enum, A: Variant<E>, B: Variant<E>, C: Variant<E>> Rack<E> for Rack3<A, B, C> {}
  impl<E: Enum, A: Variant<E>, B: Variant<E>, C: Variant<E>, D: Variant<E>> Rack<E>
    for Rack4<A, B, C, D>
  {
  }

  impl<A: Packed + Copy + Debug, B: Packed + Copy + Debug> Debug for Rack2<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      let mut l = f.debug_list();
      l.entry(&{ self.0 });
      l.entry(&{ self.1 });
      l.finish()
    }
  }

  impl<A: Packed + Copy + Debug, B: Packed + Copy + Debug, C: Packed + Copy + Debug> Debug
    for Rack3<A, B, C>
  {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      let mut l = f.debug_list();
      l.entry(&{ self.0 });
      l.entry(&{ self.1 });
      l.entry(&{ self.2 });
      l.finish()
    }
  }

  impl<
      A: Packed + Copy + Debug,
      B: Packed + Copy + Debug,
      C: Packed + Copy + Debug,
      D: Packed + Copy + Debug,
    > Debug for Rack4<A, B, C, D>
  {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      let mut l = f.debug_list();
      l.entry(&{ self.0 });
      l.entry(&{ self.1 });
      l.entry(&{ self.2 });
      l.entry(&{ self.3 });
      l.finish()
    }
  }

  trait Size: Copy {
    fn into_usize(self) -> usize;
  }

  impl Size for u8 {
    fn into_usize(self) -> usize {
      self as usize
    }
  }

  impl Size for u16 {
    fn into_usize(self) -> usize {
      self as usize
    }
  }

  impl Size for u32 {
    fn into_usize(self) -> usize {
      self as usize
    }
  }

  impl Size for u64 {
    fn into_usize(self) -> usize {
      self as usize
    }
  }

  impl Size for usize {
    fn into_usize(self) -> usize {
      self
    }
  }

  #[repr(C, packed)]
  struct Payload<S: Size, T: ?Sized> {
    size: S,
    r#type: PhantomData<T>,
  }

  struct Iter<'a, T: CheckedBitPattern> {
    raw: &'a [u8],
    r#type: PhantomData<T>,
  }

  struct IterMut<'a, T: Packed + CheckedBitPattern> {
    raw: &'a mut [u8],
    r#type: PhantomData<T>,
  }

  impl<S: Size, T: ?Sized> Payload<S, T> {
    #[inline(always)]
    fn as_bytes(&self) -> &[u8] {
      let size = self.size.into_usize();
      let ptr = unsafe { (self as *const _ as *const u8).add(mem::size_of::<S>()) };
      unsafe { slice::from_raw_parts(ptr, size) }
    }
  }

  impl<S: Size, T: Packed + CheckedBitPattern> Payload<S, T> {
    #[inline(always)]
    fn as_mut_bytes(&mut self) -> &mut [u8] {
      let size = self.size.into_usize();
      let ptr = unsafe { (self as *mut _ as *mut u8).add(mem::size_of::<S>()) };
      unsafe { slice::from_raw_parts_mut(ptr, size) }
    }
  }

  impl<S: Size, T: CheckedBitPattern> Payload<S, [T]> {
    #[inline(always)]
    fn iter(&self) -> Iter<'_, T> {
      Iter { raw: self.as_bytes(), r#type: Default::default() }
    }
  }

  impl<S: Size, T: Packed + CheckedBitPattern> Payload<S, [T]> {
    #[inline(always)]
    fn as_mut_bytes(&mut self) -> &mut [u8] {
      let size = self.size.into_usize();
      let ptr = unsafe { (self as *mut _ as *mut u8).add(mem::size_of::<S>()) };
      unsafe { slice::from_raw_parts_mut(ptr, size) }
    }

    #[inline(always)]
    fn iter_mut(&mut self) -> IterMut<'_, T> {
      IterMut { raw: self.as_mut_bytes(), r#type: Default::default() }
    }
  }

  impl<S: Size, T: CheckedBitPattern> Deref for Payload<S, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
      match try_unpack_exact(self.as_bytes()) {
        Some(x) => x,
        _ => unreachable!(),
      }
    }
  }

  impl<S: Size, T: Packed + CheckedBitPattern> DerefMut for Payload<S, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
      match try_unpack_exact_mut(self.as_mut_bytes()) {
        Some(x) => x,
        _ => unreachable!(),
      }
    }
  }

  impl<S: Size, T: CheckedBitPattern + Debug> Debug for Payload<S, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      self.deref().fmt(f)
    }
  }

  impl<S: Size, T: CheckedBitPattern + Debug> Debug for Payload<S, [T]> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      self
        .iter()
        .fold(f.debug_list(), |mut d, i| {
          d.entry(i);
          d
        })
        .finish()
    }
  }

  impl<'a, T: CheckedBitPattern + 'a> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
      if let Some((x, rest)) = try_unpack::<T>(self.raw) {
        self.raw = rest;
        return Some(x);
      }

      None
    }
  }

  impl<'a, T: Packed + CheckedBitPattern + 'a> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
      if let Some((x, rest)) = try_unpack_mut::<T>(mem::take(&mut self.raw)) {
        self.raw = rest;
        return Some(x);
      }

      None
    }
  }

  #[repr(u8)]
  enum DeType {
    Nil = 0,  /* nil, the null type */
    Uint = 1, /* unsigned integer */
    Sint = 2, /* signed twos-complement integer */
    Uuid = 3, /* UUID, a universally unique identifier */
    Text = 4, /* Text string */
    Bool = 5, /* boolean */
    Seq = 6,  /* Data element sequence */
    Alt = 7,  /* Data element alternative */
    Url = 8,  /* URL, a uniform resource locator */
  }

  #[repr(u8)]
  enum DeSize {
    U8 = 0,
    U16 = 1,
    U32 = 2,
    U64 = 3,
    U128 = 4,
    V8 = 5,
    V16 = 6,
    V32 = 7,
  }

  const fn de_type(ty: DeType, sz: DeSize) -> u8 {
    ((ty as u8) << 3) | (sz as u8)
  }

  #[derive(Debug)]
  #[repr(C, u8)]
  enum DataElement {
    Uint8(Unalign<u8>) = de_type(DeType::Uint, DeSize::U8),
    Uint16(Unalign<u16>) = de_type(DeType::Uint, DeSize::U16),
    Uint32(Unalign<u32>) = de_type(DeType::Uint, DeSize::U32),
    Uuid16(Unalign<u16>) = de_type(DeType::Uuid, DeSize::U16),
    Seq8(Payload<u8, [Self]>) = de_type(DeType::Seq, DeSize::V8),
  }

  unsafe impl CheckedBitPattern for DataElement {
    fn size_into(raw: &[u8]) -> Option<usize> {
      if raw.is_empty() {
        return None;
      }
      let res = unsafe { &*(raw.as_ptr() as *const Self) };
      match raw[0] {
        Uint8::TAG => Some(2),
        Uint16::TAG => Some(3),
        Seq8::<()>::TAG if raw.len() >= 2 => Some(2 + raw[1].into_usize()),
        _ => None,
      }
    }

    fn size(&self) -> usize {
      match self {
        DataElement::Uint8(_) => 2,
        DataElement::Uint16(_) => 3,
        DataElement::Uint32(_) => 5,
        DataElement::Uuid16(_) => 3,
        DataElement::Seq8(payload) => 2 + payload.size.into_usize(),
      }
    }
  }

  impl Enum for DataElement {
    fn tag(&self) -> Self::Tag {
      match self {
        DataElement::Uint8(_) => de_type(DeType::Uint, DeSize::U8),
        DataElement::Uint16(_) => de_type(DeType::Uint, DeSize::U16),
        DataElement::Uint32(_) => de_type(DeType::Uint, DeSize::U32),
        DataElement::Uuid16(_) => de_type(DeType::Uuid, DeSize::U16),
        DataElement::Seq8(_) => de_type(DeType::Seq, DeSize::V8),
      }
    }
  }

  #[derive(Copy, Clone, Debug)]
  #[repr(C, packed)]
  struct Uint8 {
    tag: u8,
    value: u8,
  }

  #[derive(Copy, Clone, Debug)]
  #[repr(C, packed)]
  struct Uint16 {
    tag: u8,
    value: u16,
  }

  #[derive(Copy, Clone, Debug)]
  #[repr(C, packed)]
  struct Seq8<I> {
    tag: u8,
    size: u8,
    items: I,
  }

  impl Uint8 {
    fn new(value: u8) -> Self {
      Self { tag: de_type(DeType::Uint, DeSize::U8), value }
    }
  }

  impl Uint16 {
    fn new(value: u16) -> Self {
      Self { tag: de_type(DeType::Uint, DeSize::U16), value }
    }
  }

  impl<I: Packed> Seq8<I> {
    pub fn new<'a>(items: impl IntoRack<DataElement, Rack = I>) -> Self {
      let items = items.rack();
      Self { tag: de_type(DeType::Seq, DeSize::V8), size: mem::size_of_val(&items) as u8, items }
    }
  }

  unsafe impl Packed for Uint8 {}
  unsafe impl Packed for Uint16 {}
  unsafe impl<I: Packed> Packed for Seq8<I> {}

  impl Variant<DataElement> for Uint8 {
    const TAG: u8 = de_type(DeType::Uint, DeSize::U8);
  }

  impl Variant<DataElement> for Uint16 {
    const TAG: u8 = de_type(DeType::Uint, DeSize::U16);
  }

  impl<'a, I: Rack<DataElement>> Variant<DataElement> for Seq8<I> {
    const TAG: u8 = de_type(DeType::Seq, DeSize::V8);
  }

  #[test]
  fn it_works() {
    let de = Seq8::new((
      Uint8::new(0),
      Uint8::new(1),
      Seq8::new((Uint16::new(42), Uint16::new(44), Uint16::new(46), record())),
    ));

    for i in de.items.iter() {
      println!("{:?}", i);
    }

    fn record() -> impl Variant<DataElement> {
      Seq8::new((
        Uint8::new(0),
        Uint8::new(1),
        Seq8::new((Uint16::new(42), Uint16::new(44), Uint16::new(46))),
      ))
    }

    let r = record();

    foo(r.as_ref());
    foo(try_unpack::<DataElement>(pack(&r)).unwrap().0);

    fn foo(de: &DataElement) {
      dbg!(de);
    }

    let mut x = Rack2(0u16, 0u16);

    // match record().as_enumerable() {
    //   DataElement::Seq8(items) => items.iter().for_each(|x| println!("{:?}", x)),
    //   _ => {},
    // }
    //
    // let mut x = Seq8::new((Uint8::new(0), Uint8::new(1), Uint8::new(2)));
    // x.items().iter().for_each(|x| println!("{:?}", x));
    // x.payload().iter().for_each(|x| println!("{:?}", x));

    // TODO: lazy and payload `new` and `try_from`

    // println!("{:#?}", mem::align_of::<DataElement>());
    // println!("{:#?}", de);
    // println!("{:?}", mem::size_of_val(&de));
    // println!("{:?}", pack(&Uint8::new(1)));
    // println!("{:?}", pack(&de));
    // println!("{:?}", try_unpack::<DataElement>(pack(&de)).unwrap());
    // println!("{:#?}", mem::align_of::<DataElement>());
    // println!("{:#?}", mem::size_of::<DataElement>());
  }
}

impl Size for () {}
impl Swab for () {}
unsafe impl Pack for () {}
unsafe impl UnpackUnaligned for () {}
unsafe impl PackMut for () {}
unsafe impl UnpackUnalignedMut for () {}

impl Size for u8 {}
impl Swab for u8 {}
unsafe impl Pack for u8 {}
unsafe impl UnpackUnaligned for u8 {}
unsafe impl PackMut for u8 {}
unsafe impl UnpackUnalignedMut for u8 {}

impl Size for i8 {}
impl Swab for i8 {}
unsafe impl Pack for i8 {}
unsafe impl UnpackUnaligned for i8 {}
unsafe impl PackMut for i8 {}
unsafe impl UnpackUnalignedMut for i8 {}

impl Size for u16 {}
impl Swab for u16 {}
unsafe impl Pack for u16 {}
unsafe impl Unpack for u16 {}
unsafe impl PackMut for u16 {}
unsafe impl UnpackMut for u16 {}

impl Size for bool {}
impl Swab for bool {
  fn swab(bytes: &[u8]) -> Option<Partition<'_>> {
    Partition::from_split(bytes, 1)
      .and_then(|x| match x.validated[0] {
        0 | 1 => Ok(x),
        _ => Err(Error::InvalidBytePattern),
      })
      .ok()
  }
}
unsafe impl Pack for bool {}
unsafe impl UnpackUnaligned for bool {}
// Not `PackMut` as not all bit pattern are valid.
unsafe impl UnpackUnalignedMut for bool {}

impl Size for char {
  #[inline(always)]
  fn size(&self) -> usize {
    self.len_utf8()
  }
}
impl Swab for char {
  #[inline(always)]
  fn swab(bytes: &[u8]) -> Option<Partition<'_>> {
    if !bytes.is_empty() {
      Partition::from_split(bytes, utf8_width::get_width(bytes[0])).ok()
    } else {
      None
    }
  }
}
unsafe impl Pack for char {}
unsafe impl Unpack for char {}
// Not `PackMut` as not all bit pattern are valid.
unsafe impl UnpackMut for char {}

unsafe impl Packed for () {}
unsafe impl Packed for char {}
unsafe impl Packed for bool {}
unsafe impl Packed for u8 {}
unsafe impl Packed for i8 {}
unsafe impl Packed for u16 {}
unsafe impl Packed for i16 {}
unsafe impl Packed for u32 {}
unsafe impl Packed for i32 {}
unsafe impl Packed for u64 {}
unsafe impl Packed for i64 {}
unsafe impl Packed for usize {}
unsafe impl Packed for isize {}
unsafe impl Packed for u128 {}
unsafe impl Packed for i128 {}
unsafe impl Packed for f32 {}
unsafe impl Packed for f64 {}

unsafe impl Zeroable for () {}
unsafe impl Zeroable for u8 {}
unsafe impl Zeroable for i8 {}
unsafe impl Zeroable for u16 {}
unsafe impl Zeroable for i16 {}
unsafe impl Zeroable for u32 {}
unsafe impl Zeroable for i32 {}
unsafe impl Zeroable for u64 {}
unsafe impl Zeroable for i64 {}
unsafe impl Zeroable for usize {}
unsafe impl Zeroable for isize {}
unsafe impl Zeroable for u128 {}
unsafe impl Zeroable for i128 {}
unsafe impl Zeroable for f32 {}
unsafe impl Zeroable for f64 {}

unsafe impl AnyBitPattern for () {}
unsafe impl AnyBitPattern for u8 {}
unsafe impl AnyBitPattern for i8 {}
unsafe impl AnyBitPattern for u16 {}
unsafe impl AnyBitPattern for i16 {}
unsafe impl AnyBitPattern for u32 {}
unsafe impl AnyBitPattern for i32 {}
unsafe impl AnyBitPattern for u64 {}
unsafe impl AnyBitPattern for i64 {}
unsafe impl AnyBitPattern for usize {}
unsafe impl AnyBitPattern for isize {}
unsafe impl AnyBitPattern for u128 {}
unsafe impl AnyBitPattern for i128 {}
unsafe impl AnyBitPattern for f32 {}
unsafe impl AnyBitPattern for f64 {}

unsafe impl Packed for NonZeroU8 {}
unsafe impl Packed for NonZeroI8 {}
unsafe impl Packed for NonZeroU16 {}
unsafe impl Packed for NonZeroI16 {}
unsafe impl Packed for NonZeroU32 {}
unsafe impl Packed for NonZeroI32 {}
unsafe impl Packed for NonZeroU64 {}
unsafe impl Packed for NonZeroI64 {}
unsafe impl Packed for NonZeroU128 {}
unsafe impl Packed for NonZeroI128 {}
unsafe impl Packed for NonZeroUsize {}
unsafe impl Packed for NonZeroIsize {}

unsafe impl<T: ?Sized + 'static> Packed for PhantomData<T> {}
unsafe impl Packed for PhantomPinned {}
unsafe impl<T: Packed> Packed for ManuallyDrop<T> {}

unsafe impl<T: ?Sized> Zeroable for PhantomData<T> {}
unsafe impl Zeroable for PhantomPinned {}
unsafe impl<T: Zeroable> Zeroable for ManuallyDrop<T> {}

unsafe impl<T: ?Sized + 'static> AnyBitPattern for PhantomData<T> {}
unsafe impl AnyBitPattern for PhantomPinned {}
unsafe impl<T: AnyBitPattern> AnyBitPattern for ManuallyDrop<T> {}

unsafe impl<T: Packed, const N: usize> Packed for [T; N] {}
unsafe impl<T: Zeroable, const N: usize> Zeroable for [T; N] {}
unsafe impl<T: AnyBitPattern, const N: usize> AnyBitPattern for [T; N] {}

unsafe impl<T: AnyBitPattern> CheckedBitPattern for T {
  #[inline(always)]
  fn size_into(raw: &[u8]) -> Option<usize> {
    Some(raw.len())
  }

  fn size(&self) -> usize {
    mem::size_of::<Self>()
  }
}

unsafe impl CheckedBitPattern for char {
  #[inline(always)]
  fn size_into(raw: &[u8]) -> Option<usize> {
    if raw.is_empty() {
      return None;
    }
    let width = core::str::utf8_char_width(raw[0]);
    if raw.len() < width {
      return None;
    }
    core::str::from_utf8(&raw[..width]).ok().map(|_| width)
  }

  fn size(&self) -> usize {
    self.len_utf8()
  }
}

unsafe impl CheckedBitPattern for bool {
  #[inline(always)]
  fn size_into(raw: &[u8]) -> Option<usize> {
    if raw.is_empty() {
      return None;
    }
    match raw[0] {
      0 | 1 => Some(1),
      _ => None,
    }
  }

  fn size(&self) -> usize {
    1
  }
}
