use core::fmt::Debug;
use core::marker::PhantomData;
use core::{fmt, mem};
use core::mem::ManuallyDrop;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::fmt::{Display, Formatter};

// mod unalign;
// mod unaligned;
// mod undrop;
//
// pub use unalign::*;
// pub use unaligned::*;
// pub use undrop::*;

pub unsafe trait Mutant: 'static {
  #[doc(hidden)]
  type Head: Copy;
  #[doc(hidden)]
  type Tail: Copy;

  #[doc(hidden)]
  const HEAD: Layout<Self::Head>;
  #[doc(hidden)]
  const TAIL: Layout<Self::Tail>;
}

#[repr(packed)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Unalign<T: Copy>(pub T);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct Undrop<T: ?Sized>(ManuallyDrop<T>);

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Layout<T> {
  header: (),
  layout: MaybeUninit<T>,
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
pub const fn size_of_tail<T: ?Sized + Mutant>() -> usize {
  mem::size_of::<T::Tail>()
}

#[inline]
pub const fn maybe_uninit<T: ?Sized + Mutant>(offset: usize) -> bool {
  // Handle head offset first (ie. not an offset within the trailing slice for `DST`).
  if offset < min_size_of::<T>() {
    // SAFETY: `offset` has been checked above to be in bounds.
    return unsafe { T::HEAD.maybe_uninit_unchecked(offset) };
  }
  // Ensure type is a dynamically sized type `DST`.
  assert!(size_of_tail::<T>() > 0);
  // The offset point to the trailing slice of this `DST`,
  // compute the new offset withing the tail layout.
  let offset = (offset - min_size_of::<T>()) % size_of_tail::<T>();
  // SAFETY: `offset` has been computed above to be in bounds.
  unsafe { T::TAIL.maybe_uninit_unchecked(offset) }
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
  align_of::<T>() >= align
    // or if the value address is a multiple of `U` alignment.
    || ptr::addr_of(value) & (align - 1) == 0
}

#[inline]
pub const fn transmute<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &T) -> &U {
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
  unsafe { &*ptr }
}

#[inline]
pub fn transmute_mut<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &mut T) -> &mut U {
  let metadata = layout::transmute::<T, U>(ptr::metadata(value));
  let ptr = ptr::from_raw_parts_mut::<U>(value as *mut T as *mut (), metadata);
  unsafe { &mut *ptr }
}

#[inline]
pub const fn try_transmute<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &T) -> Option<&U> {
  match layout::try_transmute::<T, U>(ptr::metadata(value)) {
    None => None,
    Some(metadata) => {
      if is_aligned_to::<T>(value, align_of::<U>()) {
        let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
        Some(unsafe { &*ptr })
      } else {
        None
      }
    }
  }
}

#[inline]
pub fn try_transmute_mut<T: ?Sized + Mutant, U: ?Sized + Mutant>(value: &mut T) -> Option<&mut U> {
  match layout::try_transmute::<T, U>(ptr::metadata(value)) {
    None => None,
    Some(metadata) => {
      if is_aligned_to::<T>(value, align_of::<U>()) {
        let ptr = ptr::from_raw_parts_mut::<U>(value as *mut T as *mut (), metadata);
        Some(unsafe { &mut *ptr })
      } else {
        None
      }
    }
  }
}

// impl Mutant -------------------------------------------------------------------------------------

unsafe impl Mutant for () {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(()) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

unsafe impl Mutant for u8 {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(0xff) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

unsafe impl Mutant for u16 {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(0xffff) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

unsafe impl Mutant for u32 {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(0xffffffff) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

unsafe impl<T: Copy + Mutant<Head = T, Tail = ()>> Mutant for [T] {
  type Head = ();
  type Tail = T;

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(()) };
  const TAIL: Layout<Self::Tail> = T::HEAD;
}

unsafe impl<T: Copy + Mutant<Head = T, Tail = ()>, const N: usize> Mutant for [T; N] {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new([layout_of::<T>(); N]) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u16le([u8; 2]);

unsafe impl Mutant for u16le {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(Self([1; 2])) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u32le([u8; 4]);

unsafe impl Mutant for u32le {
  type Head = Self;
  type Tail = ();

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(Self([1; 4])) };
  const TAIL: Layout<Self::Tail> = <()>::HEAD;
}

// impl Unalign ------------------------------------------------------------------------------------

unsafe impl<T: Mutant + Copy> Mutant for Unalign<T> {
  type Head = Unalign<T::Head>;
  type Tail = T::Tail;

  const HEAD: Layout<Self::Head> = unsafe { Layout::new(Unalign(layout_of::<T>())) };
  const TAIL: Layout<Self::Tail> = T::TAIL;
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

  const HEAD: Layout<Self::Head> = T::HEAD;
  const TAIL: Layout<Self::Tail> = T::TAIL;
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
  ) -> Option<usize> {
    if Transmute::<T, U>::REQUIREMENTS.is_transmutable(metadata) {
      Some(unsafe { transmute_unchecked::<T, U>(metadata) })
    } else {
      None
    }
  }

  #[inline]
  pub const unsafe fn transmute_unchecked<T: ?Sized + Mutant, U: ?Sized + Mutant>(
    metadata: usize,
  ) -> usize {
    match size_of_tail::<U>() {
      0 => 0,
      bt => {
        let size = min_size_of::<T>() + metadata * size_of_tail::<T>();
        (size - min_size_of::<U>()) / bt
      }
    }
  }

  impl<T> Layout<T> {
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
    const fn is_transmutable(self, metadata: usize) -> bool {
      match self {
        Self::Is(n) => metadata == n,
        Self::IsAtLeast(n) => metadata >= n,
        Self::IsMultipleOf(m) => metadata % m == 0,
        Self::IsAtLeastThenMultipleOf(n, m) => metadata >= n && metadata % m == 0,
        Self::IsEmpty => metadata == 0,
        Self::None => true,
      }
    }

    #[inline]
    const fn of<T: ?Sized + Mutant, U: ?Sized + Mutant>() -> Self {
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
      let at = size_of_tail::<T>();
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
    pub const REQUIREMENTS: Requirements = Requirements::of::<T, U>();
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
        align_of::<T>() >= align_of::<U>(),
        "cannot transmute between types of incompatible byte layouts, \
        alignment mismatch; \
        use `try_transmute` instead"
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

    #[derive(Debug, Copy, Clone)]
    #[repr(C, packed)]
    pub struct BazPacked {
      a: u8,
      b: u32,
      c: [u8; 6],
    }

    #[derive(Debug, Copy, Clone)]
    pub struct Baz {
      a: u8,
      b: u32,
      c: [u8; 6],
    }

    unsafe impl Mutant for BazPacked {
      type Head = Self;
      type Tail = ();

      const HEAD: Layout<Self::Head> = unsafe { layout!(Self::Head { a: u8, b: u32, c: [u8; 6] }) };
      const TAIL: Layout<Self::Tail> = <()>::HEAD;
    }

    unsafe impl Mutant for Baz {
      type Head = Self;
      type Tail = ();

      const HEAD: Layout<Self::Head> = unsafe { layout!(Self::Head { a: u8, b: u32, c: [u8; 6] }) };
      const TAIL: Layout<Self::Tail> = <()>::HEAD;
    }

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

    #[repr(packed)]
    struct Layout<const H: usize, const T: usize = 0>([u8; H], [[u8; T]]);

    unsafe impl<const H: usize, const T: usize> Mutant for Layout<H, T> {
      type Head = [u8; H];
      type Tail = [u8; T];

      const HEAD: crate::Layout<Self::Head> = <[u8; H]>::HEAD;
      const TAIL: crate::Layout<Self::Tail> = <[u8; T]>::HEAD;
    }

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
  pub use core::ptr;

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
    let _ = unsafe { ptr::addr_of!((*(ptr as *const (u8, ()))).1) };
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
      Self::empty_with(ptr::null())
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
      $crate::Layout::new(layout)
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

  #[derive(Copy, Clone)]
  #[repr(C, packed)]
  pub struct Foo<T: ?Sized> {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: Undrop<T>,
  }

  #[derive(Debug, Copy, Clone, Eq, PartialEq)]
  #[repr(C)]
  pub struct Bar<T: ?Sized> {
    a: [u8; 4],
    b: T,
  }

  // Approximately what `Mutant` will derive
  #[repr(C, packed)]
  pub struct __FooHead<T: ?Sized>
  where
    u8: Mutant,
    u16le: Mutant,
    [u8; 6]: Mutant,
    Undrop<T>: Mutant,
  {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: <Undrop<T> as Mutant>::Head,
    d_tail: [<Undrop<T> as Mutant>::Tail; 0],
  }

  // Approximately what `Mutant` will derive
  impl<T: ?Sized> Copy for __FooHead<T>
  where
    u8: Mutant,
    u16le: Mutant,
    [u8; 6]: Mutant,
    Undrop<T>: Mutant,
  {
  }

  // Approximately what `Mutant` will derive
  impl<T: ?Sized> Clone for __FooHead<T>
  where
    u8: Mutant,
    u16le: Mutant,
    [u8; 6]: Mutant,
    Undrop<T>: Mutant,
  {
    fn clone(&self) -> Self {
      *self
    }
  }

  // Approximately what `Mutant` will derive
  unsafe impl<T: ?Sized> Mutant for Foo<T>
  where
    u8: Mutant,
    u16le: Mutant,
    [u8; 6]: Mutant,
    Undrop<T>: Mutant,
  {
    type Head = __FooHead<T>;
    type Tail = <Undrop<T> as Mutant>::Tail;

    const HEAD: Layout<Self::Head> =
      unsafe { layout!(Self::Head { a: u8, b: u16le, c: [u8; 6], d: Undrop<T> }) };
    const TAIL: Layout<Self::Tail> = <Undrop<T> as Mutant>::TAIL;
  }

  unsafe impl<T: ?Sized + Mutant> Mutant for Bar<T> {
    type Head = Bar<T::Head>;
    type Tail = T::Tail;

    const HEAD: Layout<Self::Head> = unsafe { layout!(Self::Head { a: [u8; 4], b: T }) };
    const TAIL: Layout<Self::Tail> = T::TAIL;
  }

  #[test]
  fn test_transmute() {
    let mut foo = Foo::<[u8; 3]> {
      a: 0x42,
      b: u16le([0x42, 0x42]),
      c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
      d: Undrop::new([0xaf, 0xaf, 0xaa]),
    };

    let bar: &mut Bar<[u8]> = transmute_mut(&mut foo);

    assert_eq!([0x42; 4], bar.a);
    bar.a = [0xff; 4];

    assert_eq!(0xff, foo.a);
    assert_eq!(u16le([0xff, 0xff]), foo.b);
    assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

    pub fn pack<T: ?Sized + Mutant>(value: &T) -> &[u8] {
      transmute(value)
    }

    pub fn pack_foo(foo: &Foo<[u8; 3]>) -> &[u8] {
      pack(foo)
    }

    println!("{:?}", pack_foo(&foo));
  }

  #[test]
  fn test_try_transmute() {
    assert_eq!(Some(&[1, 2]), try_transmute::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(None, try_transmute::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(
      Some(&Bar { a: [1, 2, 3, 4], b: () }),
      try_transmute::<[u8], Bar<()>>(&[1, 2, 3, 4])
    );
    println!("{:?}", try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    println!(
      "{:?}",
      try_transmute::<Bar<[u8]>, Bar<[u8; 2]>>(
        try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
      )
    );
    println!("{:?}", transmute::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
  }
}

//   #[derive(Copy, Clone)]
//   #[repr(C, packed)]
//   struct Dst<T: ?Sized> {
//     len: u8,
//     payload: Undrop<T>,
//   }
//
//   impl<T: ?Sized + Debug + Unaligned> Debug for Dst<T> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//       f.debug_struct("Dst")
//         .field("len", &self.len)
//         .field("payload", &read_aligned!(self.payload))
//         .finish()
//     }
//   }
//
//   unsafe impl<T: ?Sized> Unaligned for Dst<T> {}
//
//   #[test]
//   fn test() {
//     println!("{:?}", unpack::<()>(&[1, 0, 0, 0]));
//     println!("{:?}", unpack::<[u8]>(&[0, 1, 2, 3, 4, 5]));
//     println!("{:?}", unpack::<[bool]>(&[0, 1, 0, 1]));
//     println!("{:?}", unpack::<[Unalign<u16>]>(&[0, 1, 0, 2]));
//     println!("{:?}", unpack::<u8>(&[0]));
//     println!("{:?}", unpack::<bool>(&[0]));
//     println!("{:?}", unpack::<bool>(&[1]));
//     println!("{:?}", unpack::<bool>(&[2]));
//     println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
//     println!("{:?}", unpack::<Dst<[u8]>>(&[3, 1, 2, 3, 4]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[2, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[1, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[3, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[4, 1, 0, 2, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[5, 1, 0, 2, 0]));
//     //
//     // println!("{:?}", unpack::<A>(&[0]));
//     // println!("{:?}", unpack::<B>(&[1, 69]));
//     // println!("{:?}", unpack::<C>(&[2, 3, 1, 2, 3]));
//     //
//     // println!("{:?}", unpack_x(&[0]));
//     // println!("{:?}", unpack_x(&[1, 69]));
//     // println!("{:?}", unpack_x(&[2, 3, 1, 2, 3]));
//     //
//     // println!("{:?}", unpack::<Command<[u8]>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
//     // println!("{:?}", unpack::<Command<SetEventMask>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
//     //
//     // println!("{:?}", unpack::<Command<[u8]>>(&[0x42, 0, 0]));
//     // println!("{:?}", unpack::<Command<Reset>>(&[0x42, 0, 0]));
//   }
//
//   #[test]
//   fn test_hci() {
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Packet<T: ?Sized> {
//       r#type: u8,
//       data: Undrop<T>,
//     }
//
//     trait PacketVariant: Pointee {
//       const PACKET_TYPE: u8;
//     }
//
//     impl<T: ?Sized + EventVariant> PacketVariant for Event<T> {
//       const PACKET_TYPE: u8 = 1;
//     }
//
//     impl PacketVariant for Acl<[u8]> {
//       const PACKET_TYPE: u8 = 2;
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Packet<T> {}
//
//     impl Pointee for Packet<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl<T: ?Sized + PacketVariant> Pointee for Packet<T> {
//       type Metadata = T::Metadata;
//       type Head = Packet<()>;
//       type Tail = T;
//
//       fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
//         head.r#type == T::PACKET_TYPE
//       }
//     }
//
//     impl<T: ?Sized + Unaligned + Debug> Debug for Packet<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Packet")
//           .field("type", &self.r#type)
//           .field("data", &read_aligned!(self.data))
//           .finish()
//       }
//     }
//
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Acl<T: ?Sized> {
//       handle_and_flags: Unalign<u16>,
//       data_total_len: Unalign<u16>,
//       parameters: Undrop<T>,
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Acl<T> {}
//
//     impl Pointee for Acl<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for Acl<[u8]> {
//       type Metadata = usize;
//       type Head = Acl<()>;
//       type Tail = [u8];
//
//       #[inline]
//       fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
//         head.data_total_len.into_inner() as usize
//       }
//     }
//
//     impl<T: ?Sized + Unaligned + Debug> Debug for Acl<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Acl")
//           .field("handle_and_flags", &self.handle_and_flags)
//           .field("data_total_len", &self.data_total_len)
//           .field("completed_packets", &read_aligned!(self.parameters))
//           .finish()
//       }
//     }
//
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Event<T: ?Sized> {
//       event_code: u8,
//       parameters_len: u8,
//       parameters: Undrop<T>,
//     }
//
//     #[derive(Debug, Copy, Clone)]
//     #[repr(C, packed)]
//     struct ConnectionComplete {
//       address: [u8; 6],
//       handle: u16,
//       link_type: u8,
//     }
//
//     #[derive(Debug, Copy, Clone)]
//     #[repr(C, packed)]
//     struct CompletedPackets {
//       handle: u16,
//       number_of_completed_packets: u16,
//     }
//
//     #[repr(C, packed)]
//     struct NumberOfCompletedPackets {
//       num_handles: u8,
//       completed_packets: [CompletedPackets],
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Event<T> {}
//     unsafe impl Unaligned for ConnectionComplete {}
//     unsafe impl Unaligned for CompletedPackets {}
//     unsafe impl Unaligned for NumberOfCompletedPackets {}
//
//     trait EventVariant: Pointee {
//       const EVENT_CODE: Option<u8>;
//     }
//
//     impl EventVariant for [u8] {
//       const EVENT_CODE: Option<u8> = None;
//     }
//
//     impl EventVariant for ConnectionComplete {
//       const EVENT_CODE: Option<u8> = Some(21);
//     }
//
//     impl EventVariant for NumberOfCompletedPackets {
//       const EVENT_CODE: Option<u8> = Some(42);
//     }
//
//     impl Pointee for ConnectionComplete {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for NumberOfCompletedPackets {
//       type Metadata = usize;
//       type Head = u8;
//       type Tail = [CompletedPackets];
//     }
//
//     impl Pointee for CompletedPackets {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for Event<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl<T: ?Sized + EventVariant> Pointee for Event<T> {
//       type Metadata = T::Metadata;
//       type Head = Event<()>;
//       type Tail = T;
//
//       #[inline]
//       fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
//         head.parameters_len as usize
//       }
//
//       #[inline]
//       fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
//         match T::EVENT_CODE {
//           Some(event_code) => event_code == head.event_code,
//           _ => true,
//         }
//       }
//     }
//
//     impl<T: ?Sized + Debug + Unaligned> Debug for Event<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Event")
//           .field("event_code", &self.event_code)
//           .field("parameters_len", &self.parameters_len)
//           .field("parameters", &read_aligned!(self.parameters))
//           .finish()
//       }
//     }
//
//     impl Debug for NumberOfCompletedPackets {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("NumberOfCompletedPackets")
//           .field("num_handles", &self.num_handles)
//           .field("completed_packets", &read_aligned!(self.completed_packets))
//           .finish()
//       }
//     }
//
//     println!("{:?}", unpack::<Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
//     println!(
//       "{:?}",
//       unpack::<Event<ConnectionComplete>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Event<ConnectionComplete>>(&[22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Packet<Event<[u8]>>>(&[1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Packet<Event<ConnectionComplete>>>(&[
//         1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3
//       ])
//     );
//   }
// }
