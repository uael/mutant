use core::ops::Range;

/// Counts the number of values in one range (`of`) that also exist in another range (`into`).
/// Both `of` and `into` are inclusive ranges.
///
/// # Parameters
/// - `of`: A `Range<u64>` representing the first range.
/// - `into`: A `Range<u64>` representing the second range.
///
/// # Returns
/// `u128`: The number of overlapping elements between the two ranges.
///
/// # Examples
/// ```
/// assert_eq!(2, overlaps(0..1, 0..1));
/// assert_eq!(1, overlaps(0..0, 0..1));
/// assert_eq!(1, overlaps(0..1, 0..0));
/// assert_eq!(0, overlaps(5..10, 11..15));
/// assert_eq!(32, overlaps(0..63, 32..63));
/// ```
pub const fn overlaps(of: Range<u64>, into: Range<u64>) -> u128 {
  let start = if of.start > into.start { of.start } else { into.start };
  let end = if of.end < into.end { of.end } else { into.end };

  // Check if there is an overlap
  if start > end {
    0
  } else {
    // Since ranges are inclusive, add 1 to include the end value
    (end - start + 1) as u128
  }
}

// tests
fn main() {
  assert_eq!(2, overlaps(0..1, 0..1));
  assert_eq!(1, overlaps(0..0, 0..1));
  assert_eq!(1, overlaps(0..1, 0..0));
  assert_eq!(32, overlaps(0..63, 32..63));
}