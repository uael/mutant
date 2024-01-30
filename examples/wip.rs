use Ordering::*;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Ordering {
  Native,
  Lsb,
  Msb,
}

impl Ordering {
  pub const fn is_msb(self) -> bool {
    matches!(self, Msb) || (matches!(self, Native) && cfg!(target_endian = "big"))
  }
}

/// Number layout.
pub struct NumberLayout<P = Option<&'static [(u64, u64)]>> {
  // Number bit patterns:
  // * None: Number is "any bit pattern".
  // * Some(ranges): ranges of allowed values, where `.end` is included.
  //   The values in `ranges` never exceed the maximum value given the width (`max(width)`).
  //   `ranges` may contain non-contiguous values (e.g. `[(0, 1), (3, 4)]` does not contain `2`).
  bit_patterns: P,
  // Byte ordering.
  ordering: Ordering,
  // Number width in bytes, between 1 and 8.
  width: u8,
}

pub const fn min(a: u64, b: u64) -> u64 {
  if a < b {
    a
  } else {
    b
  }
}

pub const fn max(a: u64, b: u64) -> u64 {
  if a > b {
    a
  } else {
    b
  }
}

/// Maximum value for a number of `width` in bytes.
pub const fn mask(width: u8) -> u64 {
  assert!(width >= 1 && width <= 8);
  ((1u128 << (width * 8)) - 1) as u64
}

/// Counts the number of possible values in an inclusive range.
pub const fn size(range: (u64, u64)) -> u128 {
  assert!(range.0 <= range.1);
  (range.1 - range.0) as u128 + 1
}

/// Bit pattern of a number of 8 bytes maximum width.
#[derive(Copy, Clone)]
pub struct BitPattern {
  /// Allowed value ranges for each part, where the width of a single `ranges[n]` is `widths[n]`.
  /// Ranges are inclusive (i.e. in `(0, 0)` 0 is included).
  ranges: [(u64, u64); 8],
  /// Widths of `ranges`, where the sum of width is between 1 and 8.
  widths: [u8; 8],
}

impl BitPattern {
  pub const fn new() -> Self {
    Self { ranges: [(0, 0); 8], widths: [0; 8] }
  }

  pub const fn width(&self) -> u8 {
    let (mut i, mut total_width) = (0, 0);
    while i < self.ranges.len() {
      total_width += self.widths[i];
      i += 1;
    }
    assert!(total_width <= 8);
    total_width
  }

  pub const fn extend(mut self, range: (u64, u64), width: u8) -> Self {
    assert!(range.0 <= mask(width) && range.1 <= mask(width));
    let (mut i, mut total_width) = (0, 0);
    while i < self.ranges.len() {
      match self.widths[i] {
        0 => {
          self.ranges[i] = range;
          self.widths[i] = width;
          return self;
        }
        n => total_width += n,
      }
      assert!(total_width + width <= 8, "bit pattern is already maxed out");
      i += 1;
    }
    panic!("trying to `extend` an already complete bit pattern");
  }

  /// Returns the number of possible values allowed by this bit pattern.
  pub const fn variety(&self) -> u128 {
    assert!(self.width() >= 1);
    let (mut i, mut variety) = (0, 1);
    while i < self.ranges.len() {
      if self.widths[i] == 0 {
        break;
      }
      variety *= size(self.ranges[i]);
      i += 1;
    }
    variety
  }

  /// Returns the number of possible values in `range` that are also possible in this bit pattern.
  pub const fn overlaps(&self, range: (u64, u64), width: u8, ordering: Ordering) -> u128 {
    assert!(width <= self.width());
    let mut total_overlaps = 1u128; // Start with 1 to multiply overlaps for each segment

    let (mut i, mut byte) = (0, 0);
    while i < self.ranges.len() && self.widths[i] != 0 {
      // Compute segment shift.
      let shift = if ordering.is_msb() { (width - byte - self.widths[i]) * 8 } else { byte * 8 };

      // Calculate the segment-specific mask and shift the input range
      let segment_start = (range.0 >> shift) & mask(self.widths[i]);
      let segment_end = (range.1 >> shift) & mask(self.widths[i]);

      // Determine the overlap with the allowed range for this segment
      let overlap_start = max(segment_start, self.ranges[i].0);
      let overlap_end = min(segment_end, self.ranges[i].1);

      // Multiply the total overlap by this segment's overlap
      total_overlaps *= if overlap_start <= overlap_end {
        size((overlap_start, overlap_end))
      } else {
        0 // No overlap
      };

      // Move to the next segment
      byte += self.widths[i];
      i += 1;
    }

    total_overlaps
  }
}

impl NumberLayout {
  pub const fn new(
    bit_patterns: Option<&'static [(u64, u64)]>,
    ordering: Ordering,
    width: u8,
  ) -> Self {
    let max = mask(width);
    if let Some(ranges) = bit_patterns {
      let mut i = 0;
      while i < ranges.len() {
        assert!(ranges[i].0 <= max);
        assert!(ranges[i].1 <= max);
        assert!(ranges[i].0 <= ranges[i].1);
        i += 1;
      }
    }
    Self { bit_patterns, ordering, width }
  }
}

pub const fn any(width: u8, ordering: Ordering) -> NumberLayout {
  assert!(width >= 1 && width <= 8);
  NumberLayout::new(None, ordering, width)
}

pub const fn ne1(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Native, 1)
}
pub const fn ne2(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Native, 2)
}
pub const fn ne4(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Native, 4)
}
pub const fn ne8(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Native, 8)
}
pub const fn le1(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Lsb, 1)
}
pub const fn le2(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Lsb, 2)
}
pub const fn le4(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Lsb, 4)
}
pub const fn le8(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Lsb, 8)
}
pub const fn be1(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Msb, 1)
}
pub const fn be2(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Msb, 2)
}
pub const fn be4(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Msb, 4)
}
pub const fn be8(ranges: &'static [(u64, u64)]) -> NumberLayout {
  NumberLayout::new(Some(ranges), Msb, 8)
}

/// Count the number of possible values in `src`.
pub const fn count(src: &[NumberLayout], width: u8) -> u128 {
  let (mut i, mut count) = (0, 1);
  while i < src.len() {
    count *= match src[i].bit_patterns {
      Some(ranges) => {
        let (mut j, mut count) = (0, 0);
        while j < ranges.len() {
          count += size((ranges[j].0, min(ranges[j].1, mask(width))));
          j += 1;
        }
        count
      }
      None => mask(src[i].width) as u128 + 1,
    };
    i += 1;
  }
  count
}

/// Count the number of overlapping possible values between `dst` and `src`.
/// `dst.width` is always the sum of all `src` widths.
/// This function assume little-endian ordering.
pub const fn overlaps(dst: NumberLayout, src: &[NumberLayout]) -> u128 {
  const fn rec(
    dst: &NumberLayout<&[(u64, u64)]>,
    src: &[NumberLayout],
    i: usize,
    pat: BitPattern,
  ) -> u128 {
    if i == src.len() {
      let (mut j, mut total_overlaps) = (0, 0);
      while j < dst.bit_patterns.len() {
        total_overlaps += pat.overlaps(dst.bit_patterns[j], dst.width, dst.ordering);
        j += 1;
      }
      return total_overlaps;
    }

    if let Some(bit_patterns) = src[i].bit_patterns {
      let (mut j, mut total_overlaps) = (0, 0);
      while j < bit_patterns.len() {
        total_overlaps += rec(dst, src, i + 1, pat.extend(bit_patterns[j], src[i].width));
        j += 1;
      }
      return total_overlaps;
    }

    rec(dst, src, i + 1, pat.extend((0, mask(src[i].width)), src[i].width))
  }

  if let Some(bit_patterns) = dst.bit_patterns {
    let dst = NumberLayout { bit_patterns, ordering: dst.ordering, width: dst.width };
    return rec(&dst, src, 0, BitPattern::new());
  }

  count(src, dst.width)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_utilities() {
    // Test min and max functions
    assert_eq!(min(1, 2), 1);
    assert_eq!(max(1, 2), 2);

    // Test mask function for various widths
    for width in 1..=7 {
      assert_eq!(mask(width), (1u64 << (width * 8)) - 1);
    }

    // Test size function with different ranges
    assert_eq!(size((0, 0)), 1);
    assert_eq!(size((0, 10)), 11);
  }

  #[test]
  fn test_bit_pattern() {
    let mut pattern = BitPattern::new();

    // Test extending the bit pattern and calculating width
    pattern = pattern.extend((0, 1), 1);
    assert_eq!(pattern.width(), 1);
    assert_eq!(pattern.variety(), 2); // For range (0, 1) with width 1

    // More complex pattern
    pattern = pattern.extend((0, 3), 2); // Adding 2 more bytes, allowing values 0 to 3
    assert_eq!(pattern.variety(), 8); // 2 * 4 = 8 possible combinations

    // Test overlaps function
    assert_eq!(2, pattern.overlaps((0, 0xFF), 3, Lsb)); // Only 2 values (0-1)
    assert_eq!(8, pattern.overlaps((0, 0xFFFF), 3, Lsb)); // Only 2 values (0-1)
  }

  #[test]
  fn test_count() {
    assert_eq!(4, count(&[le1(&[(0, 1)]), le1(&[(0, 1)])], 2));
    assert_eq!(25, count(&[le1(&[(5, 9)]), le1(&[(0, 4)])], 2));
    assert_eq!(100, count(&[le1(&[(0, 99)])], 1));
    assert_eq!(200, count(&[le2(&[(0, 99), (200, 299)])], 2));
    assert_eq!(6, count(&[le1(&[(0, 1), (3, 4), (6, 7)])], 1));
    assert_eq!(18, count(&[le1(&[(0, 2)]), le1(&[(0, 1), (3, 4), (6, 7)])], 2));
    assert_eq!(256, count(&[any(1, Lsb)], 1));
    assert_eq!(65536, count(&[any(2, Lsb)], 2));
    assert_eq!(2, count(&[le8(&[(0, 1)])], 8));
    assert_eq!(4, count(&[le8(&[(0, 1), (10, 11)])], 8));
    assert_eq!(2, count(&[le8(&[(u64::MAX - 1, u64::MAX)])], 8));
    assert_eq!(18446744073709551616, count(&[le8(&[(0, u64::MAX)])], 8));
  }

  #[test]
  fn test_overlaps_lsb() {
    assert_eq!(4, overlaps(any(2, Lsb), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
    assert_eq!(4, overlaps(le2(&[(0, 1), (256, 257)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
    assert_eq!(2, overlaps(le2(&[(0, 1)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
    assert_eq!(2, overlaps(le2(&[(256, 257)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
    assert_eq!(4, overlaps(le2(&[(512, 513), (768, 769)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
    assert_eq!(2, overlaps(le2(&[(512, 513)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
    assert_eq!(2, overlaps(le2(&[(768, 769)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
    assert_eq!(0, overlaps(le2(&[(100, 200)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
    assert_eq!(50, overlaps(le2(&[(50, 150)]), &[le1(&[(0, 99)]), le1(&[(0, 1)])]));
    assert_eq!(65536, overlaps(any(2, Lsb), &[any(1, Lsb), any(1, Lsb)]));
    assert_eq!(256, overlaps(le2(&[(0, 255)]), &[any(1, Lsb), le1(&[(0, 1)])]));
    assert_eq!(
      6,
      overlaps(le2(&[(0, 1), (3, 4), (6, 7)]), &[le1(&[(0, 1), (3, 4), (6, 7)]), le1(&[(0, 2)])])
    );
    assert_eq!(
      4,
      overlaps(
        le4(&[(0, u32::MAX as u64)]),
        &[le2(&[(0, 1)]), le2(&[(u16::MAX as u64 - 1, u16::MAX as u64)])]
      )
    );
    assert_eq!(u64::MAX as u128 + 1, overlaps(any(8, Lsb), &[any(4, Lsb), any(4, Lsb)]));
    assert_eq!(11, overlaps(le2(&[(245, 255)]), &[le1(&[(240, 255)]), le1(&[(0, 255)])]));
    assert_eq!(
      16,
      overlaps(
        le2(&[(0, 3), (256, 259), (512, 515), (768, 771)]),
        &[le1(&[(0, 3)]), le1(&[(0, 3)])]
      )
    );
    assert_eq!(2, overlaps(ne2(&[(0, 1), (4, 5), (8, 9)]), &[ne1(&[(0, 3)]), ne1(&[(0, 3)])]));
    assert_eq!(1, overlaps(ne4(&[(255, 255)]), &[ne2(&[(255, 255)]), ne2(&[(0, 255)])]));
    assert_eq!(256, overlaps(ne2(&[(0, 255)]), &[any(1, Lsb), ne1(&[(0, 0)])]));
  }

  #[test]
  fn test_overlaps_msb() {
    assert_eq!(4, overlaps(any(2, Msb), &[be1(&[(0, 1)]), be1(&[(0, 1)])]));
    assert_eq!(4, overlaps(be2(&[(0, 1), (256, 257)]), &[be1(&[(0, 1)]), be1(&[(0, 1)])]));
    assert_eq!(2, overlaps(be2(&[(0, 1)]), &[be1(&[(0, 1)]), be1(&[(0, 1)])]));
    assert_eq!(2, overlaps(be2(&[(256, 257)]), &[be1(&[(0, 1)]), be1(&[(0, 1)])]));
    assert_eq!(4, overlaps(be2(&[(512, 513), (768, 769)]), &[be1(&[(2, 3)]), be1(&[(0, 1)])]));
    assert_eq!(2, overlaps(be2(&[(512, 513)]), &[be1(&[(2, 3)]), be1(&[(0, 1)])]));
    assert_eq!(2, overlaps(be2(&[(768, 769)]), &[be1(&[(2, 3)]), be1(&[(0, 1)])]));
    assert_eq!(0, overlaps(be2(&[(100, 200)]), &[be1(&[(2, 3)]), be1(&[(0, 1)])]));
    assert_eq!(50, overlaps(be2(&[(50, 150)]), &[be1(&[(0, 1)]), be1(&[(0, 99)])]));
    assert_eq!(65536, overlaps(any(2, Msb), &[any(1, Msb), any(1, Msb)]));
    assert_eq!(256, overlaps(be2(&[(0, 255)]), &[be1(&[(0, 1)]), any(1, Msb)]));
    assert_eq!(
      6,
      overlaps(be2(&[(0, 1), (3, 4), (6, 7)]), &[be1(&[(0, 2)]), be1(&[(0, 1), (3, 4), (6, 7)])])
    );
    assert_eq!(
      4,
      overlaps(
        be4(&[(0, u32::MAX as u64)]),
        &[be2(&[(0, 1)]), be2(&[(u16::MAX as u64 - 1, u16::MAX as u64)])]
      )
    );
    assert_eq!(u64::MAX as u128 + 1, overlaps(any(8, Msb), &[any(4, Msb), any(4, Msb)]));
    assert_eq!(11, overlaps(be2(&[(245, 255)]), &[be1(&[(0, 255)]), be1(&[(240, 255)])]));
    assert_eq!(
      16,
      overlaps(
        be2(&[(0, 3), (256, 259), (512, 515), (768, 771)]),
        &[be1(&[(0, 3)]), be1(&[(0, 3)])]
      )
    );
    assert_eq!(2, overlaps(ne2(&[(0, 1), (4, 5), (8, 9)]), &[ne1(&[(0, 3)]), ne1(&[(0, 3)])]));
    assert_eq!(1, overlaps(ne4(&[(255, 255)]), &[ne2(&[(255, 255)]), ne2(&[(0, 255)])]));
    assert_eq!(256, overlaps(ne2(&[(0, 255)]), &[any(1, Msb), ne1(&[(0, 0)])]));
  }
}

// tests
fn main() {}
