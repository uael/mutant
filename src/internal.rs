use crate::layout::{intersect, wildcard_size, Intersection};
use crate::num::{cast, Numeric};
use crate::{min_size_of, Default, Type};

/// Unwrap a `Result` and return the error if it is `Err`.
#[rustfmt::skip]
macro_rules! bail {
  ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => return Err(e) });
  ($e:expr, $x:ident => $map:expr $(,)?) => (match $e { Ok(r) => r, Err($x) => return Err($map) });
}

/// Check a condition and return an error if it is `false`.
#[rustfmt::skip]
macro_rules! check { ($c:expr, $e:expr $(,)?) => { if (!$c) { return Err($e); } } }

/// Internal type layout representation.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Layout {
  min_size: usize,
  dst_slice_item_size: usize,
}

impl Layout {
  #[inline]
  const fn size_of_with<M: Numeric>(&self, metadata: M) -> usize {
    self.min_size + cast::<_, usize>(metadata) * self.dst_slice_item_size
  }
}

#[inline]
pub const fn layout_of<T: ?Sized + Type>() -> Layout {
  const {
    let min_size = min_size_of::<T>();
    let dst_slice_item_size = wildcard_size::<T::Layout>();
    Layout { min_size, dst_slice_item_size }
  }
}

#[inline]
pub const fn transmute_metadata<M: Numeric>(a: &Layout, b: &Layout, metadata: M) -> M {
  match b.dst_slice_item_size {
    0 => M::DEFAULT,
    n => cast((a.size_of_with(metadata) - b.min_size) / n),
  }
}

#[inline]
pub const fn transmute_metadata_checked<M: Numeric>(
  a: &Layout, b: &Layout, metadata: M, checks: &Option<MetadataChecks>,
) -> Result<M, isize> {
  if let Some(checks) = checks {
    bail!(checks.validate(cast(metadata)), n => n * a.dst_slice_item_size as isize);
  }
  Ok(transmute_metadata(a, b, metadata))
}

/// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) enum MetadataChecks {
  Is(usize),                             // Is equals to `n`
  IsAtLeast(usize),                      // Is at least `n`
  IsMultipleOf(usize),                   // Is a multiple of `m`.
  IsAtLeastThenMultipleOf(usize, usize), // Is at least `n` and a multiple of `m`.
  IsEmpty,                               // Implies no data, i.e. should be zero.
}

impl MetadataChecks {
  /// Validates `metadata` against constraints.
  /// In case of an error `Err(e)`, a positive `e` indicates an overflow of `metadata` by `e`,
  /// while a negative value signifies an underflow by `-e`.
  #[inline]
  pub(super) const fn validate(self, metadata: usize) -> Result<(), isize> {
    use MetadataChecks::*;
    match self {
      Is(n) if metadata == n => Ok(()),
      IsAtLeast(n) if metadata >= n => Ok(()),
      Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
      IsMultipleOf(m) if metadata % m == 0 => Ok(()),
      IsMultipleOf(m) => Err(m as isize - (metadata % m) as isize),
      IsAtLeastThenMultipleOf(n, m) if metadata >= n && (metadata - n) % m == 0 => Ok(()),
      IsAtLeastThenMultipleOf(n, _) if metadata < n => Err(n as isize - metadata as isize),
      IsAtLeastThenMultipleOf(_, m) => Err(m as isize - (metadata % m) as isize),
      IsEmpty if metadata == 0 => Ok(()),
      IsEmpty => Err(-(metadata as isize)),
    }
  }
}

/// Try to resolve `xa[ya] => xb[yb]`, where:
/// - `a` and `b` are source and target respectively,
/// - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
/// - `ya` and `yb` are the DST slice item sizes of `a` and `b` respectively.
pub const fn maybe_size_mismatch(
  a: &Layout, b: &Layout,
) -> Result<Option<MetadataChecks>, &'static str> {
  use MetadataChecks::*;
  // Where:
  // - `n` is the minimum number of the source DST slice items needed to align `a` and `b`,
  // - `m` is the source DST slice items multiplier.
  // Compute if source and target are DSTs, i.e. had a DST slice with a non-zero item size.
  let (src_is_dst, dst_is_dst) = (a.dst_slice_item_size != 0, b.dst_slice_item_size != 0);
  // Compute the minimum source DST slice items needed to align source and target.
  // 1. Source is smaller than target:
  //  - Check for source to be a DST.
  //  - Compute the missing bytes to fit the target.
  //  - In case the target isn't a DST, check if the missing bytes are a multiple of
  //    the source DST slice item size.
  //  - The minimum source DST slice items is the missing bytes ceil divided by the source DST
  //    slice item size.
  let mut n = if a.min_size < b.min_size {
    // Check for source to be a DST.
    check!(src_is_dst, "source too small to fit target");
    // Compute the missing bytes to fit the target.
    let missing = b.min_size - a.min_size;
    // In case the target isn't a DST:
    if !dst_is_dst {
      // Check if the missing bytes are a multiple of the source DST slice item size.
      check!(
        missing % a.dst_slice_item_size == 0,
        "missing bytes to fit the target is not a multiple of the source DST trailing slice item \
         size"
      );
    }
    // The minimum source DST slice items is the missing bytes ceil divided by the source DST
    // slice item size.
    (missing + a.dst_slice_item_size - 1) / a.dst_slice_item_size
  }
  // 2. Source is larger or equal to target:
  //  - The minimum source DST slice items is 0.
  else {
    // In case the target is a DST:
    // - Compute the remaining bytes to fit in the target.
    // - In case the source isn't a DST, check if the remaining bytes is a multiple
    //   of the target DST slice item size.
    if dst_is_dst {
      // Compute the remaining bytes to fit in the target.
      let remaining = a.min_size - b.min_size;
      // In case the source isn't a DST:
      if !src_is_dst {
        // Check if the remaining bytes is a multiple of the target DST slice item size.
        check!(
          remaining % b.dst_slice_item_size == 0,
          "remaining bytes to fit in the target is not a multiple of the target DST trailing \
           slice item size",
        );
      }
    }
    // The minimum source DST slice items is 0.
    0
  };
  // Compute the possible size mismatch between source and target.
  // 1. Source isn't a DST:
  let maybe_size_mismatch = if !src_is_dst {
    // No size mismatch.
    None
  }
  // 2. Source is a DST and target isn't a DST:
  else if !dst_is_dst {
    // In case the source is smaller than the target, the source metadata must be the minimum
    // source DST slice items count.
    // Otherwise, the source metadata must be empty.
    Some(if a.min_size < b.min_size { Is(n) } else { IsEmpty })
  }
  // 3. Source and target are DSTs:
  else {
    /// Greatest common divisor and least common multiple calculations to determine the alignment
    /// and size compatibility.
    const fn gcd(a: usize, b: usize) -> usize {
      match b {
        0 => a,
        _ => gcd(b, a % b),
      }
    }
    // Compute the GCD of the source and target DST trailing slice item sizes.
    let gcd = gcd(a.dst_slice_item_size, b.dst_slice_item_size);
    // Compute the absolute difference between the source and target DST minimum sizes.
    let absolute_miss =
      if a.min_size < b.min_size { b.min_size - a.min_size } else { a.min_size - b.min_size };
    // Make sure the source can catch up on the target.
    check!(absolute_miss % gcd == 0, "DST source cannot catch up on DST target");
    // Compute the minimum number of source DST slice items needed to align source and target.
    // We need to find `n` in a way that `xa + n*ya - yb` is a multiple of `yb`. Although this
    // could be resolved using complex equations, for the sake of readability, we can simplify it
    // by using the following loop:
    // - `n * ya` is the total size in bytes of the source DST slice for `n` source items.
    // - `xa + n * ya` is the total size in bytes of the source for `n` source items.
    // - `(xa + n * ya - xb)` is the total size of the target DST slice for `n` source items.
    // If the latter is a multiple of `yb`, then we found the minimum number of source DST slice
    // items needed to align source and target.
    while (a.min_size + n * a.dst_slice_item_size - b.min_size) % b.dst_slice_item_size != 0 {
      n += 1; // Increment the minimum number of source DST slice items by 1.
    }
    // Compute the metadata checks for the source DST slice.
    match (n, b.dst_slice_item_size / gcd) {
      // No size mismatch.
      (0, 1) => None,
      // Source metadata must be a multiple of the GCD.
      (0, m) => Some(IsMultipleOf(m)),
      // Source metadata must be at least `n` and a multiple of `m`.
      (n, 1) => Some(IsAtLeast(n)),
      // Source metadata must be at least `n` and a multiple of `m`.
      (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
    }
  };
  Ok(maybe_size_mismatch)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) struct Transmutation {
  pub(super) maybe_size_mismatch: Option<MetadataChecks>,
  pub(super) source_to_target_intersection: Intersection,
  pub(super) target_to_source_intersection: Intersection,
}

/// Try to resolve `Src => Dst` transmutation.
#[allow(long_running_const_eval)]
pub(super) const fn transmutation<Src: ?Sized + Type, Dst: ?Sized + Type>(
) -> Result<Transmutation, &'static str> {
  // const {
  let (a, b) = (&layout_of::<Src>(), &layout_of::<Dst>());
  // Compute the possible size mismatch from `Src` and `Dst` layouts.
  match maybe_size_mismatch(a, b) {
    Err(e) => Err(e),
    Ok(maybe_size_mismatch) => {
      use MetadataChecks::*;
      // Compute the minimum size in bytes need to fully intersect `Src` and `Dst`.
      let bound = match maybe_size_mismatch {
        None if a.dst_slice_item_size != 0 => a.min_size + a.dst_slice_item_size,
        None => b.min_size,
        Some(IsEmpty) => a.min_size,
        Some(IsMultipleOf(m)) => a.min_size + (m * a.dst_slice_item_size),
        Some(Is(n)) => a.min_size + (n * a.dst_slice_item_size),
        Some(IsAtLeast(n)) => a.min_size + (n * a.dst_slice_item_size) + a.dst_slice_item_size,
        Some(IsAtLeastThenMultipleOf(n, m)) => a.min_size + ((n + m) * a.dst_slice_item_size),
      };
      // Finalize by intersecting `Src` and `Dst` in both direction.
      let source_to_target_intersection = intersect::<Src::Layout, Dst::Layout>(bound);
      // let source_to_target_intersection = Intersection::Subset;
      if let Intersection::Disjoint = source_to_target_intersection {
        Err("none of the bit patterns in source are valid in target")
      } else {
        Ok(Transmutation {
          maybe_size_mismatch,
          source_to_target_intersection,
          // target_to_source_intersection: Intersection::Subset,
          target_to_source_intersection: intersect::<Dst::Layout, Src::Layout>(bound),
        })
      }
    } // }
  }
}

#[inline]
pub(super) const fn convert_metadata<Src: ?Sized + Type, Dst: ?Sized + Type>(
  metadata: usize,
) -> usize {
  let (a, b) = const { (&layout_of::<Src>(), &layout_of::<Dst>()) };
  match b.dst_slice_item_size {
    0 => 0,
    n => ((a.min_size + metadata * a.dst_slice_item_size) - b.min_size) / n,
  }
}

#[cfg(test)]
impl Transmutation {
  #[inline]
  const fn new(
    maybe_size_mismatch: Option<MetadataChecks>, source_to_target_intersection: Intersection,
    target_to_source_intersection: Intersection,
  ) -> Self {
    Self { maybe_size_mismatch, source_to_target_intersection, target_to_source_intersection }
  }

  #[inline]
  pub const fn unreachable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
    Self::new(maybe_size_mismatch, Intersection::Disjoint, Intersection::Disjoint)
  }

  #[inline]
  pub const fn fallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
    Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Overlap)
  }

  #[inline]
  pub const fn fallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
    Self::new(maybe_size_mismatch, Intersection::Overlap, Intersection::Subset)
  }

  #[inline]
  pub const fn infallible(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
    Self::new(maybe_size_mismatch, Intersection::Subset, Intersection::Overlap)
  }

  #[cfg(test)]
  #[inline]
  pub const fn infallible_mutable(maybe_size_mismatch: Option<MetadataChecks>) -> Self {
    Self::new(maybe_size_mismatch, Intersection::Subset, Intersection::Subset)
  }
}
