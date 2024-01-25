use core::ops::Range;

pub const fn shift(val: u64, width: usize, off: usize, end: usize) -> u64 {
  assert!(width <= 8);
  assert!(off < width);
  assert!(end <= width);
  assert!(end > off);
  // Create the mask to isolate the desired bits
  let mask = (1u128 << ((end - off) * 8)) - 1;

  // Shift the value to the right by the offset, then apply the mask
  (val >> (off * 8)) & mask as u64
}

pub const fn pat_shift(pat: Range<u64>, width: usize, off: usize, end: usize) -> Range<u64> {
  shift(pat.start, width, off, end)..shift(pat.end, width, off, end)
}

// tests
fn main() {
  assert_eq!(0xafbfcf, shift(0xafbfcf, 3, 0, 3));
  assert_eq!(0xafbf, shift(0xafbfcf, 3, 1, 3));
  assert_eq!(0xaf, shift(0xafbfcf, 3, 2, 3));
  assert_eq!(0xbfcf, shift(0xafbfcf, 3, 0, 2));
  assert_eq!(0xcf, shift(0xafbfcf, 3, 0, 1));
  assert_eq!(0xbf, shift(0xafbfcf, 3, 1, 2));

  assert_eq!(0xca..0xafbfcf, pat_shift(0xca..0xafbfcf, 3, 0, 3));
  assert_eq!(0x00..0xafbf, pat_shift(0xca..0xafbfcf, 3, 1, 3));
  assert_eq!(0x00..0xaf, pat_shift(0xca..0xafbfcf, 3, 2, 3));
  assert_eq!(0xca..0xbfcf, pat_shift(0xca..0xafbfcf, 3, 0, 2));
  assert_eq!(0xca..0xcf, pat_shift(0xca..0xafbfcf, 3, 0, 1));
  assert_eq!(0x00..0xbf, pat_shift(0xca..0xafbfcf, 3, 1, 2));
}
