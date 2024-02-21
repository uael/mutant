// use std::fmt::{Debug, Formatter};
// use mutant::bail;
//
// use ByteLayout::*;
// use BytePattern::*;
//
// #[derive(Copy, Clone)]
// enum BytePattern { Mask(u8), Allow(u8), Discard(u8) }
//
// #[derive(Copy, Clone)]
// pub struct BitPattern {
//   width: usize, // Pattern width in bytes, between 1 and 8.
//   pattern: [BytePattern; 8],
// }
//
// #[derive(Debug, Copy, Clone)]
// pub struct RawLayout {
//   // Number width in bytes, between 1 and 8.
//   width: usize,
//   // Number bit patterns.
//   bit_patterns: &'static [[BytePattern; 8]],
// }
//
// impl BytePattern {
//   pub const fn contains(self, other: Self) -> bool {
//     match (self, other) {
//       (Mask(a), Mask(b)) => a | b == a, // All bits in 'b' are allowed by 'a'.
//       (Mask(a), Allow(b)) => a & b == b, // 'b' is allowed by 'a'.
//       (Mask(a), Discard(b)) => a & b == 0, // No disallowed bits in 'b' by 'a'.
//       (Allow(a), Allow(b)) => a == b, // 'a' is exactly 'b'.
//       (Allow(_), _) => false, // 'Allow' can't contain 'Mask' or 'Discard'.
//       (Discard(a), Discard(b)) => a == b, // 'a' discards exactly what 'b' discards.
//       (Discard(a), Allow(b)) => a != b, // 'b' isn't discarded by 'a'.
//       (Discard(a), Mask(b)) => a & b != a, // None of the bits in 'a' can be produced by 'b'.
//     }
//   }
// }
//
// impl BitPattern {
//   pub const fn new(width: usize, pattern: [BytePattern; 8]) -> Self {
//     assert!(width >= 1 && width <= 8);
//     BitPattern { width, pattern }
//   }
//
//   pub const fn any(width: usize) -> Self {
//     assert!(width >= 1 && width <= 8);
//     let (mut i, mut pattern) = (0, [Mask(0); 8]);
//     while i < width {
//       pattern[i] = Mask(0xff);
//       i += 1;
//     }
//     BitPattern { width, pattern }
//   }
//
//   pub const fn contains(&self, other: &Self) -> bool {
//     assert!(self.width == other.width);
//     let mut i = 0;
//     while i < self.width {
//       if !self.pattern[i].contains(other.pattern[i]) {
//         return false;
//       }
//       i += 1;
//     }
//     true
//   }
//
//   pub const fn static_analyze(&self, src: &Self, mut sa: StaticAnalyze) -> StaticAnalyze {
//     let dst_in_src = src.contains(&self);
//     sa.any_dst_in_src |= dst_in_src;
//     sa.all_dst_in_src &= dst_in_src;
//     sa.all_src_in_dst &= self.contains(&src);
//     sa
//   }
// }
//
// impl Debug for BitPattern {
//   fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//     self.pattern.iter().take(self.width).fold(&mut f.debug_list(), |d, p| d.entry(&p)).finish()
//   }
// }
//
// #[derive(Copy, Clone)]
// pub enum ByteLayout {
//   Uninit,
//   OutOfBounds,
//   Init { offset: usize, width: usize, bit_patterns: &'static [[BytePattern; 8]] }
// }
//
// impl Debug for BytePattern {
//   fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//     match self {
//       Mask(mask) if *mask == 0 => write!(f, "none"),
//       Mask(mask) if *mask == 0xff => write!(f, "any"),
//       Mask(mask) => write!(f, "{:#b}", mask),
//       Allow(byte) => write!(f, "{:#02x}", byte),
//       Discard(byte) => write!(f, "!{:#02x}", byte),
//     }
//   }
// }
//
// #[derive(Debug, Copy, Clone)]
// struct Source {
//   pattern: [BytePattern; 16],
//   start: usize,
//   end: usize,
// }
//
// #[derive(Debug, Copy, Clone)]
// struct Target {
//   width: usize,
//   bit_patterns: &'static [[BytePattern; 8]],
// }
//
// impl Source {
//   pub const fn empty(off: usize) -> Self {
//     Self {
//       pattern: [Mask(0); 16],
//       start: off,
//       end: off,
//     }
//   }
//
//   pub const fn len(&self) -> usize {
//     self.end - self.start
//   }
//
//   pub const fn seek(mut self, off: usize) -> Self {
//     assert!(self.end >= off);
//     let mut i = off;
//     while i < self.end {
//       self.pattern[i - off] = self.pattern[i - self.start];
//       i += 1;
//     }
//     self.start = off;
//     self
//   }
//
//   pub const fn feed(mut self, pattern: &[BytePattern; 8], width: usize, rewind: usize) -> Self {
//     if rewind > 0 {
//       assert!(self.len() == 0);
//       assert!(self.start >= rewind);
//       self.start -= rewind;
//       self.end -= rewind;
//     }
//
//     let len = self.len();
//     assert!(len + width <= self.pattern.len());
//     let mut i = 0;
//     while i < width {
//       self.pattern[len + i] = pattern[i];
//       i += 1;
//     }
//     self.end += width;
//     self
//   }
//
//   pub const fn to_pattern(&self, off: usize, width: usize) -> BitPattern {
//     assert!(width >= 1 && width <= 8);
//     assert!(off >= self.start);
//     assert!(off + width <= self.end);
//     let (mut i, mut pattern) = (0, [Mask(0); 8]);
//     while i < width {
//       pattern[i] = self.pattern[off - self.start + i];
//       i += 1;
//     }
//     BitPattern { width, pattern }
//   }
// }
//
// struct Cx<'a> {
//   src: &'a [Option<(usize, &'static [[BytePattern; 8]])>],
//   dst: &'a [Option<(usize, &'static [[BytePattern; 8]])>],
// }
//
// pub const fn byte_layout_at(layouts: &[Option<(usize, &'static [[BytePattern; 8]])>], offset: usize) -> ByteLayout {
//   let (mut i, mut cumulative_offset) = (0, 0);
//   while i < layouts.len() {
//     let next_offset = cumulative_offset + match &layouts[i] {
//       Some(layout) => layout.0,
//       _ => 1,
//     };
//     // Check if the global offset falls within the current layout's range.
//     if offset >= cumulative_offset && offset < next_offset {
//       return match layouts[i] {
//         Some((width, bit_patterns)) => {
//           // Calculate local offset within the current layout.
//           let offset = offset - cumulative_offset;
//           // Return the current layout and its local offset.
//           Init { offset, width, bit_patterns }
//         },
//         _ => Uninit,
//       }
//     }
//     // Update cumulative_offset for the next layout.
//     cumulative_offset = next_offset;
//     i += 1;
//   }
//   OutOfBounds
// }
//
// #[derive(Debug, Copy, Clone)]
// struct StaticAnalyze {
//   any_dst_in_src: bool,
//   all_dst_in_src: bool,
//   all_src_in_dst: bool,
// }
//
// impl<'a> Cx<'a> {
//   pub const fn new(src: &'a [Option<(usize, &'static [[BytePattern; 8]])>], dst: &'a [Option<(usize, &'static [[BytePattern; 8]])>]) -> Self {
//     Self { src, dst }
//   }
//
//   const fn next(&self, mut off: usize) -> Option<(usize, Target)> {
//     loop {
//       match byte_layout_at(self.dst, off) {
//         Uninit => off += 1,
//         OutOfBounds => break None,
//         Init { offset, width, bit_patterns } => {
//           assert!(offset == 0);
//           break Some((off, Target { width, bit_patterns }))
//         }
//       }
//     }
//   }
//
//   fn sa(
//     &self,
//     mut src: Source,
//     dst: Target,
//     off: usize,
//     mut sa: StaticAnalyze,
//   ) -> Result<StaticAnalyze, &'static str> {
//     // Catch up `src` on `off`
//     if off >= src.end {
//       src = Source::empty(off);
//     }
//
//     // Make sure `src` pattern is big enough to analyze `dst`.
//     while src.end < off + dst.width {
//       let (offset, width, bit_patterns) = match byte_layout_at(self.src, src.end) {
//         Uninit | OutOfBounds => return Err("error"),
//         Init { offset, width, bit_patterns } => (offset, width, bit_patterns)
//       };
//       if bit_patterns.len() == 1 {
//         src = src.feed(&bit_patterns[0], width, offset);
//         continue
//       }
//       let mut i = 0;
//       while i < bit_patterns.len() {
//         let src = src.feed(&bit_patterns[i], width, offset);
//         sa = bail!(self.sa(src, dst, off, sa));
//         i += 1;
//       }
//       return Ok(sa);
//     }
//
//     let (mut i, srcp) = (0, src.to_pattern(off, dst.width));
//     while i < dst.bit_patterns.len() {
//       sa = BitPattern::new(dst.width, dst.bit_patterns[i]).static_analyze(&srcp, sa);
//       i += 1;
//     }
//
//     let end = off + dst.width;
//     match self.next(end) {
//       None => Ok(sa),
//       Some((off, dst)) => self.sa(src.seek(end), dst, off, sa)
//     }
//   }
// }
//
// impl RawLayout {
//   pub const fn new(width: usize, bit_patterns: &'static [[BytePattern; 8]]) -> Self {
//     assert!(bit_patterns.len() >= 1);
//     Self { width, bit_patterns }
//   }
// }
//
// pub fn sa(src: &[Option<RawLayout>], dst: &[Option<RawLayout>]) -> Result<StaticAnalyze, &'static str> {
//   let cx = Cx::new(src, dst);
//   let (dst, off) = match cx.next(0) {
//     None => return Err("invalid layout"),
//     Some((dst, off)) => (dst, off),
//   };
//   cx.sa(Source::empty(0), dst, off, StaticAnalyze { any_dst_in_src: false, all_dst_in_src: true, all_src_in_dst: true })
// }
//
// pub const fn raw1(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(1, bit_patterns)) }
// pub const fn raw2(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(2, bit_patterns)) }
// pub const fn raw3(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(3, bit_patterns)) }
// pub const fn raw4(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(4, bit_patterns)) }
// pub const fn raw5(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(5, bit_patterns)) }
// pub const fn raw6(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(6, bit_patterns)) }
// pub const fn raw7(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(7, bit_patterns)) }
// pub const fn raw8(bit_patterns: &'static [[BytePattern; 8]]) -> Option<RawLayout> { Some(RawLayout::new(8, bit_patterns)) }
//
// pub const fn ne1(a: u8)  -> [u8; 8] { [a, 0, 0, 0, 0, 0, 0, 0] }
// pub const fn ne2(v: u16) -> [u8; 8] { let [a, b] = v.to_ne_bytes(); [a, b, 0, 0, 0, 0, 0, 0] }
// pub const fn ne4(v: u32) -> [u8; 8] { let [a, b, c, d] = v.to_ne_bytes(); [a, b, c, d, 0, 0, 0, 0] }
// pub const fn ne8(v: u64) -> [u8; 8] { v.to_ne_bytes() }
//
// pub const fn le1(a: u8)  -> [u8; 8] { [a, 0, 0, 0, 0, 0, 0, 0] }
// pub const fn le2(v: u16) -> [u8; 8] { let [a, b] = v.to_le_bytes(); [a, b, 0, 0, 0, 0, 0, 0] }
// pub const fn le4(v: u32) -> [u8; 8] { let [a, b, c, d] = v.to_le_bytes(); [a, b, c, d, 0, 0, 0, 0] }
// pub const fn le8(v: u64) -> [u8; 8] { v.to_le_bytes() }
//
// pub const fn be1(a: u8)  -> [u8; 8] { [a, 0, 0, 0, 0, 0, 0, 0] }
// pub const fn be2(v: u16) -> [u8; 8] { let [a, b] = v.to_be_bytes(); [a, b, 0, 0, 0, 0, 0, 0] }
// pub const fn be4(v: u32) -> [u8; 8] { let [a, b, c, d] = v.to_be_bytes(); [a, b, c, d, 0, 0, 0, 0] }
// pub const fn be8(v: u64) -> [u8; 8] { v.to_be_bytes() }
//
// pub const fn pat<const N: usize>(b: [BytePattern; N]) -> [BytePattern; 8] {
//   assert!(N <= 8);
//   let (mut i, mut pattern) = (0, [Mask(0); 8]);
//   while i < N {
//     pattern[i] = b[i];
//     i += 1;
//   }
//   pattern
// }
//
// #[cfg(test)]
// mod tests {
//   use super::*;
//
//   #[allow(non_upper_case_globals)]
//   #[test]
//   fn test_sa() {
//     const u8: Option<RawLayout> = raw1(&[[Mask(0xff); 8]]);
//     const u16: Option<RawLayout> = raw2(&[[Mask(0xff); 8]]);
//     const bool: Option<RawLayout> = raw1(&[pat([Mask(0x01)])]);
//     const u8nz: Option<RawLayout> = raw1(&[pat([Discard(0)])]);
//     const u16nz: Option<RawLayout> = raw2(&[pat([Discard(0), Mask(0xff)])]);
//     const u32nz: Option<RawLayout> = raw4(&[pat([Discard(0), Mask(0xff), Mask(0xff), Mask(0xff)])]);
//
//     let _ = dbg!(sa(&[u8], &[u8]));
//     let _ = dbg!(sa(&[bool], &[bool]));
//     let _ = dbg!(sa(&[bool], &[u8]));
//     let _ = dbg!(sa(&[u8], &[bool]));
//     let _ = dbg!(sa(&[bool, bool], &[u16]));
//     let _ = dbg!(sa(&[bool, bool], &[bool, bool]));
//     let _ = dbg!(sa(&[bool, bool], &[u16nz]));
//     let _ = dbg!(sa(&[u16nz, bool, u16nz], &[bool, u16nz, bool]));
//
//     // assert_eq!(4, overlaps(any(2, Lsb), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
//     // assert_eq!(4, overlaps(le2(&[(0, 1), (256, 257)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
//     // assert_eq!(2, overlaps(le2(&[(0, 1)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
//     // assert_eq!(2, overlaps(le2(&[(256, 257)]), &[le1(&[(0, 1)]), le1(&[(0, 1)])]));
//     // assert_eq!(4, overlaps(le2(&[(512, 513), (768, 769)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
//     // assert_eq!(2, overlaps(le2(&[(512, 513)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
//     // assert_eq!(2, overlaps(le2(&[(768, 769)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
//     // assert_eq!(0, overlaps(le2(&[(100, 200)]), &[le1(&[(0, 1)]), le1(&[(2, 3)])]));
//     // assert_eq!(50, overlaps(le2(&[(50, 150)]), &[le1(&[(0, 99)]), le1(&[(0, 1)])]));
//     // assert_eq!(65536, overlaps(any(2, Lsb), &[any(1, Lsb), any(1, Lsb)]));
//     // assert_eq!(256, overlaps(le2(&[(0, 255)]), &[any(1, Lsb), le1(&[(0, 1)])]));
//     // assert_eq!(
//     //   6,
//     //   overlaps(le2(&[(0, 1), (3, 4), (6, 7)]), &[le1(&[(0, 1), (3, 4), (6, 7)]), le1(&[(0, 2)])])
//     // );
//     // assert_eq!(
//     //   4,
//     //   overlaps(
//     //     le4(&[(0, u32::MAX as u64)]),
//     //     &[le2(&[(0, 1)]), le2(&[(u16::MAX as u64 - 1, u16::MAX as u64)])]
//     //   )
//     // );
//     // assert_eq!(u64::MAX as u128 + 1, overlaps(any(8, Lsb), &[any(4, Lsb), any(4, Lsb)]));
//     // assert_eq!(11, overlaps(le2(&[(245, 255)]), &[le1(&[(240, 255)]), le1(&[(0, 255)])]));
//     // assert_eq!(
//     //   16,
//     //   overlaps(
//     //     le2(&[(0, 3), (256, 259), (512, 515), (768, 771)]),
//     //     &[le1(&[(0, 3)]), le1(&[(0, 3)])]
//     //   )
//     // );
//   }
// }

// tests
fn main() {}
