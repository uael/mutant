use core::marker::PhantomData;
use core::num::NonZeroUsize;

use crate::{is_fallible, layout_of, min_size_of, size_of_tail, Data, IntegerSpec, Repr};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Layout<T: ?Sized + Data> {
  Void,
  Slice,
  Array,
  Struct { repr: Repr<T, T::Fields> },
  Integer { spec: IntegerSpec },
}

#[inline]
pub const fn transmute<T: ?Sized + Data, U: ?Sized + Data>(metadata: usize) -> usize {
  let _ = Transmute::<T, U>::ASSERT_INFALLIBLE;
  unsafe { transmute_unchecked::<T, U>(metadata) }
}

#[inline]
pub const fn try_transmute<T: ?Sized + Data, U: ?Sized + Data>(
  metadata: usize,
) -> Result<usize, isize> {
  match Transmute::<T, U>::REQUIREMENTS.delta(metadata) {
    0 => Ok(unsafe { transmute_unchecked::<T, U>(metadata) }),
    d => Err(d),
  }
}

#[inline]
pub const unsafe fn transmute_unchecked<T: ?Sized + Data, U: ?Sized + Data>(
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

#[inline]
const fn is_zst_slice<T: ?Sized + Data>() -> bool {
  matches!(T::LAYOUT, Layout::Slice) && size_of_tail::<T>() == 0
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
  const fn of<T: ?Sized + Data, U: ?Sized + Data>() -> Result<Self, &'static str> {
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
        _ if is_zst_slice::<U>() => return Ok(Self::None),
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
        } else if is_zst_slice::<U>() {
          Self::None
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
      let (al, bl) = (layout_of::<T>(offset), layout_of::<U>(offset));
      if bl != 0 && al == 0 {
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

struct Transmute<T: ?Sized, U: ?Sized>(PhantomData<T>, PhantomData<U>);

impl<T: ?Sized + Data, U: ?Sized + Data> Transmute<T, U> {
  pub const REQUIREMENTS: Requirements = match Requirements::of::<T, U>() {
    Ok(requirements) => requirements,
    Err(err) => panic!("{}", err),
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
    Requirements::None => {
      assert!(!is_fallible::<U>(), "target is fallible");
      if !is_zst_slice::<U>() {
        let mut offset = 0;
        while offset < min_size_of::<T>() {
          let (al, bl) = (layout_of::<T>(offset), layout_of::<U>(offset));
          if bl != 0 && al == 0 || al & bl != al {
            panic!(
              "cannot transmute between types of incompatible byte layouts, \
              source padding will left uninitialized byte(s) in target"
            );
          }
          offset += 1;
        }
      }
    }
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::tests::*;

  #[test]
  fn test_infallible() {
    let _ = Transmute::<[u8], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[u8; 2], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 5]], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 4]], [[u8; 2]]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<FooPacked<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<Bar<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<FooPacked<[u8]>, Bar<[u8]>>::ASSERT_INFALLIBLE;
    let _ = Transmute::<bool, u8>::ASSERT_INFALLIBLE;
  }

  #[test]
  fn test_layout_of() {
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
      assert_eq!(0xff, layout_of::<BazPacked>(offset), "at {}", offset);
    }

    #[derive(Debug, Copy, Clone, Data)]
    #[repr(C)]
    #[data(crate)]
    pub struct Baz(u8, u32, [u8; 6]);

    assert_eq!(Ok(None), Requirements::of::<BazPacked, [u8]>());
    assert_eq!(
      Err(
        "cannot transmute between types of incompatible byte layouts, \
        source padding will left uninitialized byte(s) in target"
      ),
      Requirements::of::<Baz, [u8]>()
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
    assert_eq!(Ok(None), Requirements::of::<Ty<1>, Ty<1>>());
    assert_eq!(Ok(None), Requirements::of::<Ty<42>, Ty<42>>());
    assert_eq!(Ok(None), Requirements::of::<Ty<42, 1>, Ty<42, 1>>());

    // Test for equal head sizes with different tail sizes
    assert_eq!(Ok(None), Requirements::of::<Ty<42, 3>, Ty<42, 1>>());
    assert_eq!(Ok(IsMultipleOf(10)), Requirements::of::<Ty<50, 10>, Ty<50, 100>>());
    assert_eq!(Ok(IsMultipleOf(20)), Requirements::of::<Ty<50, 5>, Ty<50, 100>>());

    // Test cases for transmuting to and from empty tail
    assert_eq!(Ok(IsEmpty), Requirements::of::<Ty<42, 1>, Ty<42>>());
    assert_eq!(Ok(Is(7)), Requirements::of::<Ty<35, 1>, Ty<42>>());

    // Test cases for large difference in head sizes
    assert_eq!(Ok(Is(80)), Requirements::of::<Ty<20, 1>, Ty<100>>());

    // Test cases for tail sizes as multiples
    assert_eq!(Ok(IsMultipleOf(2)), Requirements::of::<Ty<42, 1>, Ty<42, 2>>());
    assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 1>, Ty<42, 3>>());
    assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 2>, Ty<42, 3>>());
    assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 4>, Ty<42, 3>>());
    assert_eq!(Ok(IsMultipleOf(8)), Requirements::of::<Ty<42, 5>, Ty<42, 8>>());
    assert_eq!(Ok(IsMultipleOf(3)), Requirements::of::<Ty<42, 2>, Ty<42, 6>>());

    // Test zero heads
    assert_eq!(Ok(Is(42)), Requirements::of::<Ty<0, 1>, Ty<42>>());
    assert_eq!(Ok(IsAtLeast(42)), Requirements::of::<Ty<0, 1>, Ty<42, 1>>());

    // Test cases for alignment and complex transmutations
    assert_eq!(Ok(IsAtLeastThenMultipleOf(2, 3)), Requirements::of::<Ty<42, 1>, Ty<38, 3>>());
    assert_eq!(Ok(IsAtLeastThenMultipleOf(1, 3)), Requirements::of::<Ty<42, 2>, Ty<38, 3>>());
    assert_eq!(Ok(IsAtLeastThenMultipleOf(1, 3)), Requirements::of::<Ty<43, 2>, Ty<39, 3>>());
    assert_eq!(Ok(IsMultipleOf(2)), Requirements::of::<Ty<43, 2>, Ty<39, 4>>());
    assert_eq!(Ok(IsAtLeast(2)), Requirements::of::<Ty<1, 2>, Ty<5, 2>>());
    assert_eq!(Ok(IsAtLeastThenMultipleOf(1, 3)), Requirements::of::<Ty<45, 2>, Ty<38, 3>>());
    assert_eq!(Ok(IsAtLeastThenMultipleOf(2, 4)), Requirements::of::<Ty<40, 1>, Ty<38, 4>>());
    assert_eq!(Ok(IsAtLeastThenMultipleOf(2, 4)), Requirements::of::<Ty<44, 5>, Ty<38, 4>>());
    assert_eq!(Ok(IsAtLeast(1)), Requirements::of::<Ty<38, 3>, Ty<41, 3>>());

    // Panic test cases for incompatible transmutations
    assert_eq!(
      Err("cannot transmute between types of different sizes, target too small"),
      Requirements::of::<Ty<2>, Ty<1>>()
    );
    assert_eq!(
      Err("cannot transmute between types of different sizes, source too small"),
      Requirements::of::<Ty<1>, Ty<2>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between types of incompatible byte layouts, \
        missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
        item size"
      ),
      Requirements::of::<Ty<35, 2>, Ty<42>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between types of incompatible byte layouts, \
        missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
        item size"
      ),
      Requirements::of::<Ty<40, 3>, Ty<42>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between types of incompatible byte layouts, \
        remaining bytes to fit in the target is not a multiple of the target `DST` \
        trailing slice item size"
      ),
      Requirements::of::<Ty<42>, Ty<38, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<42, 3>, Ty<38, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<41, 3>, Ty<39, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<38, 3>, Ty<42, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<39, 3>, Ty<41, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<39, 3>, Ty<41, 6>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<39, 6>, Ty<41, 3>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<39, 6>, Ty<41, 9>>()
    );
    assert_eq!(
      Err(
        "cannot transmute between `DST` types of incompatible byte layouts, \
        source cannot catchup on target"
      ),
      Requirements::of::<Ty<39, 3>, Ty<41, 9>>()
    );
  }
}
