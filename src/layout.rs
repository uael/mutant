use core::marker::PhantomData;
use core::num::NonZeroUsize;

use crate::{DataType, min_size_of, size_of_tail_item, maybe_uninit, Repr, IntegerSpec};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Layout<T: ?Sized + DataType> {
  Void,
  Slice, Array,
  Struct { repr: Repr<T, T::Fields> },
  Integer { spec: IntegerSpec },
}

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::assert_panic;
  use crate::tests::*;

  #[test]
  fn test_infallible() {
    let _ = Transmute::<[u8], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[u8; 2], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 5]], [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<[[u8; 4]], [[u8; 2]]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<Foo<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<Bar<[u8]>, [u8]>::ASSERT_INFALLIBLE;
    let _ = Transmute::<Foo<[u8]>, Bar<[u8]>>::ASSERT_INFALLIBLE;
  }

  #[test]
  fn test_nouninit() {
    use core::mem;
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
