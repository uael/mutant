use core::fmt::{Debug, Display, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::{fmt, mem};
use std::ops::{Deref, DerefMut};

pub use mutant_derive::ensure;
pub use mutant_derive::Mutant;

pub unsafe trait Mutant: 'static {
  #[doc(hidden)]
  type Head: Copy;
  #[doc(hidden)]
  type Tail: ?Sized + Mutant;
  #[doc(hidden)]
  type Item: Copy;

  #[doc(hidden)]
  const HEAD: TypeLayout<Self::Head>;
  #[doc(hidden)]
  const TAIL: TypeLayout<Self::Item>;

  const TAG: Option<u64> = None;
  const TAG_SPEC: Option<IntSpecifier> = None;
  const SIZE_SPEC: Option<IntSpecifier> = None;

  #[inline]
  fn cast<T: ?Sized + Mutant>(&self) -> &T {
    cast(self)
  }

  #[inline]
  fn try_cast<T: ?Sized + Mutant>(&self) -> Result<&T, isize> {
    try_cast(self)
  }
}

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Undrop<T: ?Sized>(ManuallyDrop<T>);

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct TypeLayout<T> {
  header: (),
  layout: MaybeUninit<T>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering {
  LittleEndian,
  BigEndian,
}

#[derive(Debug, Copy, Clone)]
pub struct IntSpecifier {
  ordering: Ordering,
  offset: usize,
  bits: u8,
}

impl IntSpecifier {
  const fn new(ordering: Ordering, offset: usize, bits: u8) -> Self {
    Self { ordering, offset, bits }
  }

  pub const fn u8(offset: usize) -> Self {
    Self::le(offset, 8)
  }

  pub const fn le(offset: usize, bits: u8) -> Self {
    assert!(bits > 0 && bits <= 64);
    Self::new(Ordering::LittleEndian, offset, bits)
  }

  pub const fn be(offset: usize, bits: u8) -> Self {
    assert!(bits > 0 && bits <= 64);
    Self::new(Ordering::BigEndian, offset, bits)
  }

  pub const unsafe fn load(&self, ptr: *const u8) -> u64 {
    let ptr = ptr.offset(self.offset as isize);
    assert!(self.bits > 0 && self.bits <= 64);
    let val = match self.bits.next_power_of_two() / 8 {
      1 => *ptr.cast::<u8>() as u64,
      2 => *ptr.cast::<u16>() as u64,
      4 => *ptr.cast::<u32>() as u64,
      8 => *ptr.cast::<u64>(),
      _ => unreachable!(),
    };
    let val = match self.ordering {
      #[cfg(target_endian = "big")]
      Ordering::LittleEndian => val.swap_bytes(),
      #[cfg(target_endian = "little")]
      Ordering::BigEndian => val.swap_bytes(),
      _ => val,
    };
    let mask = u64::MAX >> (64 - self.bits);
    val & mask
  }
}

// fn ----------------------------------------------------------------------------------------------

#[inline]
pub const fn align_of<T: ?Sized + Mutant>() -> usize {
  mem::align_of::<T::Head>()
}

#[inline]
pub const fn min_size_of<T: ?Sized + Mutant>() -> usize {
  mem::size_of::<T::Head>()
}

#[inline]
pub const fn size_of_tail_item<T: ?Sized + Mutant>() -> usize {
  mem::size_of::<T::Item>()
}

#[inline]
pub const fn is_sized<T: ?Sized + Mutant>() -> bool {
  size_of_tail_item::<T>() == 0
}

#[inline]
pub const fn is_unsized<T: ?Sized + Mutant>() -> bool {
  size_of_tail_item::<T>() > 0
}

#[inline]
pub const fn maybe_uninit<T: ?Sized + Mutant>(offset: usize) -> bool {
  // Handle head offset first (ie. not an offset within the trailing slice for `DST`).
  if offset < min_size_of::<T>() {
    // SAFETY: `offset` has been checked above to be in bounds.
    return unsafe { T::HEAD.maybe_uninit_unchecked(offset) };
  }
  // Ensure type is a dynamically sized type `DST`.
  assert!(size_of_tail_item::<T>() > 0);
  // The offset point to the trailing slice of this `DST`,
  // compute the new offset withing the tail layout.
  let offset = (offset - min_size_of::<T>()) % size_of_tail_item::<T>();
  // SAFETY: `offset` has been computed above to be in bounds.
  unsafe { T::TAIL.maybe_uninit_unchecked(offset) }
}

#[doc(hidden)]
#[inline]
pub const fn tail_of<T: ?Sized + Mutant>(val: &T) -> &T::Tail {
  let tail_offset = min_size_of::<T>() - min_size_of::<T::Tail>();
  let ptr = unsafe { (val as *const T).cast::<u8>().offset(tail_offset as isize) };
  unsafe { &*ptr::from_raw_parts(ptr.cast(), ptr::metadata(val)) }
}

#[doc(hidden)]
#[inline]
pub const unsafe fn layout_of<T: ?Sized + Mutant>() -> T::Head {
  T::HEAD.layout.assume_init()
}

#[inline]
pub const fn is_aligned_to<T: ?Sized + Mutant>(value: &T, align: usize) -> bool {
  assert!(align.is_power_of_two(), "alignment must be power of 2");
  // `value` is aligned for `U` if `T` alignment is already big enough.
  align <= align_of::<T>()
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(value) & (align - 1) == 0
}

#[inline]
pub const fn is_valid<T: ?Sized + Mutant>(val: &T) -> Result<(), isize> {
  if let (Some(tag), Some(tag_spec)) = (<T::Tail as Mutant>::TAG, T::TAG_SPEC) {
    if unsafe { tag_spec.load((val as *const T).cast()) } != tag {
      return Err(0);
    }
  }
  if let Some(size_spec) = T::SIZE_SPEC {
    let metadata = ptr::metadata(val);
    let actual_size = min_size_of::<T::Tail>() + metadata * size_of_tail_item::<T>();
    let expected_size = unsafe { size_spec.load((val as *const T).cast()) } as usize;
    if expected_size != actual_size {
      return Err(actual_size as isize - expected_size as isize);
    }
  }
  if min_size_of::<T::Tail>() == 0 {
    return Ok(());
  }
  is_valid(tail_of::<T>(val))
}

#[inline]
pub const fn cast<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &T) -> &U {
  ensure!(is_aligned_to::<T>(value, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
  unsafe { &*ptr }
}

#[inline]
pub fn cast_mut<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &mut T) -> &mut U {
  ensure!(is_aligned_to::<T>(value, align_of::<U>()), "alignment mismatch");
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts_mut::<U>(value as *mut T as *mut (), metadata);
  unsafe { &mut *ptr }
}

#[inline]
pub const fn try_cast<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &T) -> Result<&U, isize> {
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
    const fn assert_unaligned<T: ?Sized + Mutant>(_: *const T) {
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
    const fn assert_unaligned<T: ?Sized + Mutant>(_: *mut T) {
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

// impl Mutant -------------------------------------------------------------------------------------

unsafe impl Mutant for () {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(()) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

unsafe impl Mutant for u8 {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(0xff) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

unsafe impl Mutant for u16 {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(0xffff) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

unsafe impl Mutant for u32 {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(0xffffffff) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

unsafe impl<T: Copy + Mutant> Mutant for [T] {
  type Head = ();
  type Tail = ();
  type Item = T::Head;

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(()) };
  const TAIL: TypeLayout<Self::Item> = T::HEAD;
}

unsafe impl<T: Copy + Mutant, const N: usize> Mutant for [T; N] {
  type Head = [T::Head; N];
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new([layout_of::<T>(); N]) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u16le([u8; 2]);

unsafe impl Mutant for u16le {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(Self([1; 2])) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u32le([u8; 4]);

unsafe impl Mutant for u32le {
  type Head = Self;
  type Tail = ();
  type Item = ();

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(Self([1; 4])) };
  const TAIL: TypeLayout<Self::Item> = <()>::HEAD;
}

// impl Unalign ------------------------------------------------------------------------------------

unsafe impl<T: Mutant + Copy> Mutant for Unalign<T> {
  type Head = Unalign<T::Head>;
  type Tail = T::Tail;
  type Item = T::Item;

  const HEAD: TypeLayout<Self::Head> = unsafe { TypeLayout::new(Unalign(layout_of::<T>())) };
  const TAIL: TypeLayout<Self::Item> = T::TAIL;

  const TAG: Option<u64> = T::TAG;
  const TAG_SPEC: Option<IntSpecifier> = T::TAG_SPEC;
  const SIZE_SPEC: Option<IntSpecifier> = T::SIZE_SPEC;
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

impl<T: Copy + Mutant> Deref for Unalign<T> {
  type Target = T;

  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unaligned_ref!(&self.0)
  }
}

impl<T: Copy + Mutant> DerefMut for Unalign<T> {
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

unsafe impl<T: ?Sized + Mutant> Mutant for Undrop<T> {
  type Head = T::Head;
  type Tail = T::Tail;
  type Item = T::Item;

  const HEAD: TypeLayout<Self::Head> = T::HEAD;
  const TAIL: TypeLayout<Self::Item> = T::TAIL;

  const TAG: Option<u64> = T::TAG;
  const TAG_SPEC: Option<IntSpecifier> = T::TAG_SPEC;
  const SIZE_SPEC: Option<IntSpecifier> = T::SIZE_SPEC;
}

impl<T: ?Sized> Undrop<T> {
  const ASSERT_NOT_DROP: () = assert!(!mem::needs_drop::<T>());
}

impl<T: Sized> Undrop<T> {
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
  pub const fn transmute<T: ?Sized + Mutant, U: ?Sized + Mutant>(metadata: usize) -> usize {
    let _ = Transmute::<T, U>::ASSERT_INFALLIBLE;
    unsafe { transmute_unchecked::<T, U>(metadata) }
  }

  #[inline]
  pub const fn try_transmute<T: ?Sized + Mutant, U: ?Sized + Mutant>(
    metadata: usize,
  ) -> Result<usize, isize> {
    match Transmute::<T, U>::REQUIREMENTS.delta(metadata) {
      0 => Ok(unsafe { transmute_unchecked::<T, U>(metadata) }),
      d => Err(d),
    }
  }

  #[inline]
  pub const unsafe fn transmute_unchecked<T: ?Sized + Mutant, U: ?Sized + Mutant>(
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

  impl<T> TypeLayout<T> {
    #[inline(always)]
    pub const unsafe fn new(invalid: T) -> Self {
      Self { header: (), layout: MaybeUninit::new(invalid) }
    }

    pub(super) const unsafe fn maybe_uninit_unchecked(self, offset: usize) -> bool {
      (&self.header as *const ()).cast::<u8>().offset(offset as isize).read() == 0
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
    const fn of<T: ?Sized + Mutant, U: ?Sized + Mutant>() -> Self {
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
        if !maybe_uninit::<U>(offset) && maybe_uninit::<T>(offset) {
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

  impl<T: ?Sized + Mutant, U: ?Sized + Mutant> Transmute<T, U> {
    pub const REQUIREMENTS: Requirements = {
      if let Some(spec) = U::TAG_SPEC {
        assert!(spec.bits > 0 && spec.bits <= 64);
        assert!(min_size_of::<U>() >= spec.offset + (spec.bits as usize / 8))
      }
      if let Some(spec) = U::SIZE_SPEC {
        assert!(spec.bits > 0 && spec.bits <= 64);
        assert!(min_size_of::<U>() >= spec.offset + (spec.bits as usize / 8))
      }
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
      Requirements::None => assert!(
        U::TAG_SPEC.is_none() && U::SIZE_SPEC.is_none(),
        "transmute is fallible; use `try_transmute` instead"
      ),
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

    #[derive(Debug, Copy, Clone, Mutant)]
    #[repr(C, packed)]
    #[mutant(crate = "crate")]
    pub struct BazPacked {
      a: u8,
      b: u32,
      c: [u8; 6],
    }

    #[derive(Debug, Copy, Clone, Mutant)]
    #[mutant(crate = "crate")]
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

    #[derive(Mutant)]
    #[repr(packed)]
    #[mutant(crate = "crate")]
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
  #[inline]
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

  #[derive(Copy, Clone, Mutant)]
  #[repr(C, packed)]
  #[mutant(crate = "crate")]
  pub struct Foo<T: ?Sized> {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: Undrop<T>,
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq, Mutant)]
  #[repr(C)]
  #[mutant(crate = "crate")]
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

    pub fn pack<T: ?Sized + Mutant>(value: &T) -> &[u8] {
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
    println!(
      "{:?}",
      try_cast::<Bar<[u8]>, Bar<[u8; 2]>>(
        try_cast::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
      )
    );
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
      async fn send<T: ?Sized + Mutant>(&self, tx: &Packet<T>);
    }

    #[derive(Copy, Clone, Mutant)]
    #[mutant(crate = "crate", tag_spec = IntSpecifier::u8(0))]
    #[repr(C, packed)]
    pub struct Packet<T: ?Sized> {
      pub r#type: u8,
      pub data: Undrop<T>,
    }

    impl<T: ?Sized + Debug + Mutant> Debug for Packet<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Packet")
          .field("type", &self.r#type)
          .field("data", &unaligned_ref!(&self.data))
          .finish()
      }
    }

    #[derive(Copy, Clone, Mutant)]
    #[mutant(crate = "crate", tag = 0x01, size_spec = IntSpecifier::le(2, 16))]
    #[repr(C, packed)]
    pub struct Acl<T: ?Sized> {
      pub handle_and_flags: Unalign<u16>,
      pub data_total_len: Unalign<u16>,
      pub parameters: Undrop<T>,
    }

    impl<T: ?Sized + Debug + Mutant> Debug for Acl<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Acl")
          .field("handle_and_flags", &self.handle_and_flags)
          .field("data_total_len", &self.data_total_len)
          .field("completed_packets", &unaligned_ref!(&self.parameters))
          .finish()
      }
    }

    #[derive(Copy, Clone, Mutant)]
    #[mutant(crate = "crate", tag = 0x02, tag_spec = IntSpecifier::u8(0), size_spec = IntSpecifier::u8(1))]
    #[repr(C, packed)]
    pub struct Event<T: ?Sized + EventParameters> {
      pub event_code: u8,
      pub parameters_len: u8,
      pub parameters: Undrop<T>,
    }

    pub trait EventParameters: Mutant {
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

    #[derive(Debug, Copy, Clone, Mutant)]
    #[mutant(crate = "crate", tag = 21)]
    #[repr(C, packed)]
    pub struct ConnectionComplete {
      pub address: [u8; 6],
      pub handle: u16,
      pub link_type: u8,
    }

    #[derive(Debug, Copy, Clone, Mutant)]
    #[mutant(crate = "crate", tag = 22)]
    #[repr(C, packed)]
    pub struct DisconnectionComplete {
      pub address: [u8; 6],
      pub handle: u16,
    }

    #[derive(Debug, Copy, Clone, Mutant)]
    #[mutant(crate = "crate")]
    #[repr(C, packed)]
    pub struct CompletedPackets {
      pub handle: u16,
      pub number_of_completed_packets: u16,
    }

    #[derive(Mutant)]
    #[mutant(crate = "crate", tag = 23)]
    #[repr(C, packed)]
    pub struct NumberOfCompletedPackets {
      pub num_handles: u8,
      pub completed_packets: [CompletedPackets],
    }

    #[derive(Copy, Clone, Mutant)]
    #[mutant(crate = "crate")]
    #[repr(C, packed)]
    pub struct Command<T: ?Sized + Mutant> {
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

      async fn send<T: ?Sized + Mutant>(&self, tx: &hci::Packet<T>) {
        // TODO: flow-control
        self.lower.send(tx).await
      }
    }

    impl<'rx, L: hci::Socket> Host<'rx, L> {
      pub async fn get<T: ?Sized + Mutant>(&self) -> &T {
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
