use core::hint;
use core::mem;
use core::num::NonZeroU16;
use core::ptr::read_unaligned;

use crate::ptr;

use Branch::*;

/// `check_bit_patterns` transmutability context.
#[derive(Debug, Copy, Clone, PartialEq)]
struct Transmutability {
  reachable: bool,  // At least one source bit pattern exists in target.
  infallible: bool, // All source bit patterns exists in target.
  mutable: bool,    // All target bit patterns exists in source.
}

#[derive(Debug, PartialEq, Hash)]
pub struct WordLayout {
  width: u8,
  flags: u8,
  bytes: [u8],
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct InitLayout<'a> {
  offset: usize,
  layout: &'a WordLayout,
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum ByteLayout<'a> {
  Uninit,
  Init(InitLayout<'a>),
}

#[derive(Debug, Copy, Clone)]
struct BytesLayout<'a> {
  width: usize,
  offset: usize,
  layouts_count: usize,
  layouts: [&'a WordLayout; 16],
  branches: [BranchIter; 16],
  transmutability: Transmutability,
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
enum Branch {
  Any,
  Allow(u8),
  Discard(u8),
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
struct BranchIter(u64);

/// Marker trait for plain word types (e.g. integers or enum discriminants).
///
/// # Safety
/// - Word types are fully initialized types, i.e. contains no uninitialized bytes.
/// - Implementor is responsible for `BITS_PATTERN` to represents only valid values for `Self`.
pub unsafe trait Word: Copy + 'static {
  type Raw: Word<Raw = Self::Raw>;
  /// Valid bits pattern for this type.
  const WORD_LAYOUT: &'static WordLayout;
}

#[doc(hidden)]
pub mod bits {
  pub const ANY: u8 = 0;
  pub const ALLOW: u8 = 0b10;
  pub const DISCARD: u8 = 0b100;
}

#[inline]
pub const fn matches<T: Word>(raw: &T::Raw) -> bool {
  T::WORD_LAYOUT.matches(raw)
}

#[doc(hidden)]
#[inline(always)]
pub const fn raw<T: Word>(raw: T::Raw) -> T::Raw {
  assert!(mem::size_of::<T>() == mem::size_of::<T::Raw>());
  assert!(<T::Raw as Word>::WORD_LAYOUT.is_any());
  raw
}

macro_rules! layout {
  (any) => (WordLayout::from_ref::<0, 0>(&(::core::mem::size_of::<Self>() as u8, 0, [])));
  (!$discard:expr) => (layout!(__impl DISCARD $discard));
  ($($allow:tt)|+) => (layout!(__impl ALLOW $($allow),+));
  (__impl $flags:ident $($values:expr),+) => (WordLayout::from_ref(&(
    ::core::mem::size_of::<Self>() as u8,
    bits::$flags,
    [$(raw::<Self>($values).to_ne_bytes()),+])
  ));
}

impl Branch {
  #[inline(always)]
  pub const fn contains_any(self, other: Self) -> bool {
    match (self, other) {
      (_, Any) | (Any, _) => true,
      (Allow(a), Allow(b)) | (Discard(a), Discard(b)) => a == b,
      (Allow(a), Discard(b)) | (Discard(a), Allow(b)) => a != b,
    }
  }

  #[inline(always)]
  pub const fn contains_all(self, other: Self) -> bool {
    match (self, other) {
      (Any, _) => true,  // All exists in any.
      (_, Any) => false, // All any cant exists in not any.

      (Allow(b), Allow(a)) => a == b,
      (Discard(b), Allow(a)) => a != b,

      (Allow(_), Discard(_)) => false,
      (Discard(b), Discard(a)) => a == b,
    }
  }
}

impl Transmutability {
  #[inline(always)]
  pub const fn reachable_infallible_mutable() -> Self {
    Self { reachable: true, infallible: true, mutable: true }
  }

  #[inline(always)]
  pub const fn reachable_infallible() -> Self {
    Self { reachable: true, infallible: true, mutable: false }
  }

  #[inline(always)]
  pub const fn reachable_mutable() -> Self {
    Self { reachable: true, infallible: false, mutable: true }
  }

  #[inline(always)]
  pub const fn reachable() -> Self {
    Self { reachable: true, infallible: false, mutable: false }
  }

  #[inline(always)]
  pub const fn unreachable() -> Self {
    Self { reachable: false, infallible: true, mutable: true }
  }

  #[inline(always)]
  pub const fn and(mut self, other: Self) -> Self {
    self.reachable &= other.reachable;
    self.mutable &= other.mutable;
    self.infallible &= other.infallible;
    self
  }

  #[inline(always)]
  pub const fn or(mut self, other: Self) -> Self {
    self.reachable |= other.reachable;
    if other.reachable {
      self.mutable &= other.mutable;
      self.infallible &= other.infallible;
    }
    self
  }
}

impl BranchIter {
  #[inline(always)]
  pub const fn empty() -> Self {
    Self(0)
  }

  #[inline(always)]
  pub const fn new(branches: usize) -> Self {
    Self(u64::MAX >> (64 - branches))
  }

  #[inline(always)]
  pub const fn branches(self) -> usize {
    self.0.count_ones() as usize
  }

  #[inline(always)]
  pub const fn next(self) -> (Option<usize>, Self) {
    if self.0 == 0 {
      return (None, self);
    }
    let index = self.0.trailing_zeros() as usize;
    (Some(index), self.invalidate(index))
  }

  #[inline(always)]
  pub const fn invalidate(self, index: usize) -> Self {
    Self(self.0 & !(1 << index))
  }
}

#[doc(hidden)]
#[inline(always)]
pub const fn mask(bits: usize) -> u128 {
  u128::MAX >> (128 - bits)
}

impl WordLayout {
  const ANY: &'static Self = Self::from_ref::<0, 0>(&(0, 0, []));

  #[inline(always)]
  pub const fn from_bytes(bytes: &[u8]) -> &Self {
    assert!(bytes.len() >= 2);
    assert!(bytes[0] == 0 || (bytes[0].is_power_of_two() && bytes[0] <= 16));
    let values_bytes = bytes.len() - 2;
    let values = match bytes[0] {
      0 => values_bytes,
      _ => {
        assert!(values_bytes % bytes[0] as usize == 0);
        values_bytes / bytes[0] as usize
      }
    };
    match bytes[1] {
      bits::ANY => assert!(values == 0),
      bits::ALLOW => assert!(values != 0 && values < 64),
      bits::DISCARD => assert!(values == 1),
      _ => panic!("invalid bits pattern type"),
    }
    // SAFETY: above checks ensure a valid layout.
    unsafe { &*ptr::from_raw_parts(bytes.as_ptr().cast(), values_bytes) }
  }

  #[inline(always)]
  pub const fn from_ref<const W: usize, const N: usize>(x: &(u8, u8, [[u8; W]; N])) -> &Self {
    Self::from_bytes(unsafe { &*ptr::from_raw_parts(ptr::from_ref(x).cast(), 2 + W * N) })
  }

  #[inline(always)]
  pub const fn is_any(&self) -> bool {
    self.flags == bits::ANY
  }

  #[inline(always)]
  pub const fn width(&self) -> usize {
    self.width as usize
  }

  #[inline(always)]
  pub const fn branches(&self) -> usize {
    match self.flags {
      bits::ALLOW => self.bytes.len() / self.width as usize,
      _ => 1,
    }
  }

  #[inline(always)]
  pub const fn matches<T: Word>(&self, word: &T) -> bool {
    if self.width as usize != mem::size_of::<T>() {
      return false;
    }
    // SAFETY: `word` is a valid reference and the above check
    // make sure `word` is valid up to `self.width`.
    unsafe { self.matches_ptr(ptr::from_ref(word).cast()) }
  }

  /// # Safety
  /// `ptr` is valid for read up to `self.width` bytes.
  #[inline(always)]
  pub const unsafe fn matches_ptr(&self, ptr: *const u8) -> bool {
    macro_rules! inner {
      ($ptr:ident, $ty:ident) => {{
        let val = read_unaligned::<$ty>($ptr.cast());
        let mut query = self.bytes.as_ptr().cast::<$ty>();
        match self.flags & (bits::ALLOW | bits::DISCARD) {
          bits::DISCARD => val != read_unaligned(query),
          bits::ALLOW => {
            let end_ptr = self.bytes.as_ptr().add(self.bytes.len());
            while ptr::addr_of(query) != ptr::addr_of(end_ptr) {
              let allow = read_unaligned(query);
              if val == allow {
                return true;
              }
              query = query.add(1);
            }
            false
          }
          _ => true,
        }
      }};
    }
    match self.width {
      0 => true,
      1 => inner!(ptr, u8),
      2 => inner!(ptr, u16),
      4 => inner!(ptr, u32),
      8 => inner!(ptr, u64),
      16 => inner!(ptr, u128),
      _ => unsafe { hint::unreachable_unchecked() },
    }
  }
}

impl<'a> InitLayout<'a> {
  #[inline(always)]
  pub const fn width(&self) -> usize {
    self.layout.width() - self.offset
  }

  #[inline(always)]
  pub const fn branches(&self) -> usize {
    self.layout.branches()
  }

  #[inline(always)]
  pub const fn branch(&self, branch: usize) -> Branch {
    assert!(branch < self.branches());
    match self.layout.flags {
      bits::DISCARD => Discard(self.layout.bytes[self.offset]),
      bits::ALLOW => Allow(self.layout.bytes[(branch * self.layout.width()) + self.offset]),
      _ => Any,
    }
  }
}

impl<'a> ByteLayout<'a> {
  #[inline(always)]
  pub const fn width(&self) -> usize {
    match self {
      ByteLayout::Uninit => 1,
      ByteLayout::Init(layout) => layout.width(),
    }
  }
}

impl<'a> BytesLayout<'a> {
  #[inline(always)]
  pub const fn empty() -> Self {
    Self {
      width: 0,
      offset: 0,
      layouts_count: 0,
      layouts: [WordLayout::ANY; 16],
      branches: [BranchIter::empty(); 16],
      transmutability: Transmutability::reachable_infallible_mutable(),
    }
  }

  /// Append a layout to the aggregate.
  #[inline(always)]
  pub const fn append(mut self, layout: ByteLayout<'a>) -> Self {
    assert!(self.layouts_count < self.layouts.len());
    let (branches, layout, width) = match layout {
      ByteLayout::Uninit => (BranchIter::empty(), WordLayout::ANY, 1),
      ByteLayout::Init(layout) => {
        if layout.offset != 0 {
          assert!(self.layouts_count == 0 && self.offset == 0);
          self.offset = layout.offset;
        }
        (BranchIter::new(layout.branches()), layout.layout, layout.width())
      }
    };
    self.branches[self.layouts_count] = branches;
    self.layouts[self.layouts_count] = layout;
    self.width += width;
    self.layouts_count += 1;
    self
  }

  /// Shift right the aggregate by `n` bytes.
  #[inline(always)]
  pub const fn shift(self, n: usize) -> Self {
    let mut shifted = Self::empty();
    shifted.transmutability = self.transmutability;
    while shifted.width + n < self.width {
      shifted = shifted.append(self.offset(shifted.width + n));
    }
    shifted
  }

  #[inline(always)]
  pub const fn offset(self, mut offset: usize) -> ByteLayout<'a> {
    assert!(offset < self.width);
    let mut i = 0;
    while i < self.layouts_count {
      let mut width = self.layouts[i].width();
      if i == 0 {
        width -= self.offset;
      }
      if offset < width {
        return ByteLayout::Init(InitLayout { offset, layout: self.layouts[i] });
      }
      i += 1;
      offset -= width;
    }
    unreachable!()
  }

  #[inline(always)]
  pub fn intersects2(mut self, layout: &WordLayout) -> Self {
    assert!(layout.width() <= self.width);
    let mut dst_branches = BranchIter::new(layout.branches());
    let mut offset = 0;
    let mut i = 0;
    // Keep track of the full transmutability against target.
    let mut full_t = Transmutability::reachable_infallible_mutable();
    // Iterate over all source layouts until target is fully covered.
    while offset < layout.width() && i < self.layouts_count {
      let dst = InitLayout { offset, layout };
      // Keep track of the target covered branches for this source.
      let mut dst_coverage = 0u64;
      // Keep track of this source chunk transmutability against target.
      let mut chunk_t = Transmutability::unreachable();
      // Try each source branches, invalidate unreachable ones.
      let mut src_branches = self.branches[i].next();
      while let Some(src_branch) = src_branches.0 {
        // Keep track of this branch transmutability against target.
        let branch_t;
        (branch_t, dst_coverage) =
          self.branch_transmutability(i, dst, dst_branches, dst_coverage, src_branch);
        // Or branches into chunk transmutability.
        chunk_t = chunk_t.or(branch_t);
        // If this source branch is unreachable, mark it as invalid.
        if !branch_t.reachable {
          self.branches[i] = self.branches[i].invalidate(src_branch);
        }
        // Move to next source branch.
        src_branches = src_branches.1.next();
      }
      // And chunks into full transmutability.
      full_t = full_t.and(chunk_t);
      // Only continue with target covered branches.
      dst_branches = BranchIter(dst_coverage);
      // Advance target offset by the width of the source layout at index `i`.
      offset += match i {
        0 => self.layouts[i].width() - self.offset, // The first source layout can be offset.
        _ => self.layouts[i].width(),
      };
      // Are all chunk branches covered ?
      full_t.infallible &= self.branches[i].branches() == self.layouts[i].branches();
      // Move to next source layout.
      i += 1;
    }
    // Are all target branches covered ?
    full_t.mutable &= dst_branches.branches() == layout.branches();
    // And this transmutability to global state.
    self.transmutability = self.transmutability.and(full_t);
    self
  }

  pub const fn branches_transmutability(
    &self,
    i: usize,
    mut dst: InitLayout,
    branches: (usize, usize),
  ) -> Transmutability {
    let mut result = Transmutability::reachable_infallible_mutable();
    // Compute source offset.
    let mut src_offset = match i {
      0 => self.offset,
      _ => 0,
    };
    // Iterate over all bytes of this branch.
    while dst.width() > 0 && src_offset < self.layouts[i].width() {
      // Compute both source and target branch patterns.
      let pattern = (
        InitLayout { offset: src_offset, layout: self.layouts[i] }.branch(branches.0),
        dst.branch(branches.1),
      );
      // Aggregate results.
      result.reachable &= pattern.0.contains_any(pattern.1);
      result.mutable &= pattern.0.contains_all(pattern.1);
      result.infallible &= pattern.1.contains_all(pattern.0);
      // Move to next offset.
      src_offset += 1;
      dst.offset += 1;
    }
    result
  }

  pub const fn branch_transmutability(
    &self,
    i: usize,
    dst: InitLayout,
    dst_branches: BranchIter,
    mut dst_coverage: u64,
    src_branch: usize,
  ) -> (Transmutability, u64) {
    let mut result = Transmutability::unreachable();
    // Try each target branches.
    let mut dst_branch_iter = dst_branches.next();
    while let Some(dst_branch) = dst_branch_iter.0 {
      let transmutability = self.branches_transmutability(i, dst, (src_branch, dst_branch));
      result = result.or(transmutability);
      // If this branch combination is reachable, mark this target branch as covered.
      if transmutability.reachable {
        dst_coverage |= 1 << dst_branch;
      }
      // Move to next target branch.
      dst_branch_iter = dst_branch_iter.1.next();
    }
    (result, dst_coverage)
  }
}

unsafe impl Word for () {
  type Raw = ();
  const WORD_LAYOUT: &'static WordLayout = layout!(any);
}

unsafe impl Word for u8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(any);
}

unsafe impl Word for u16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(any);
}

unsafe impl Word for u32 {
  type Raw = u32;
  const WORD_LAYOUT: &'static WordLayout = layout!(any);
}

unsafe impl Word for bool {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1);
}

unsafe impl Word for NonZeroU16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(!0);
}

#[repr(u32)]
#[derive(Copy, Clone)]
enum TestEnum {
  A = 0x01010101,
  B = 0x02020202,
  C = 0x03030303,
}

unsafe impl Word for TestEnum {
  type Raw = u32;
  const WORD_LAYOUT: &'static WordLayout = layout!(0x01010101 | 0x02020202 | 0x03030303);
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo1u8 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Word for UpTo1u8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1);
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo1u16 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Word for UpTo1u16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1);
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo2u8 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Word for UpTo2u8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1 | 2);
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo2u16 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Word for UpTo2u16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1 | 2);
}

#[test]
fn test() {
  pub fn transmutability(src: &[ByteLayout], dst: &[ByteLayout]) -> Transmutability {
    let mut src_index = 0;
    let mut dst_index = 0;
    let mut bytes = BytesLayout::empty();
    while dst_index < dst.len() {
      while bytes.width < dst[dst_index].width() {
        bytes = bytes.append(src[src_index]);
        src_index += 1;
      }
      bytes = match dst[dst_index] {
        ByteLayout::Init(init) => bytes.intersects2(init.layout).shift(init.width()),
        _ => bytes.shift(1),
      };
      dst_index += 1;
    }
    bytes.transmutability
  }

  macro_rules! transmutability {
    (($($src:tt)+) -> ($($dst:tt)+)) => (
      transmutability(
        &[$(transmutability!(@impl $src)),+],
        &[$(transmutability!(@impl $dst)),+],
      )
    );
    (@impl _) => (
      ByteLayout::Uninit
    );
    (@impl $ty:ident) => (
      ByteLayout::Init(InitLayout { offset: 0, layout: $ty::WORD_LAYOUT })
    );
  }

  macro_rules! test_transmutability {
    (($($src:tt)+) -> ($($dst:tt)+) => $res:ident) => (
      assert_eq!(
        transmutability!(($($src)+) -> ($($dst)+)),
        Transmutability::$res(),
      )
    );
  }

  test_transmutability!((u8) -> (u8) => reachable_infallible_mutable);
  test_transmutability!((u32) -> (u32) => reachable_infallible_mutable);
  test_transmutability!((bool) -> (bool) => reachable_infallible_mutable);
  test_transmutability!((TestEnum) -> (TestEnum) => reachable_infallible_mutable);

  test_transmutability!((u8) -> (bool) => reachable_mutable);
  test_transmutability!((bool) -> (u8) => reachable_infallible);
  test_transmutability!((bool) -> (UpTo1u8) => reachable_infallible_mutable);
  test_transmutability!((UpTo1u8) -> (bool) => reachable_infallible_mutable);

  test_transmutability!((u32) -> (TestEnum) => reachable_mutable);
  test_transmutability!((TestEnum) -> (u32) => reachable_infallible);

  test_transmutability!((bool bool bool bool) -> (TestEnum) => reachable);
  test_transmutability!((bool bool bool bool) -> (u8 u8 u8 u8) => reachable_infallible);
  test_transmutability!((bool _ _ bool) -> (u8 _ _ u8) => reachable_infallible);

  test_transmutability!((u8 bool UpTo2u16) -> (UpTo2u8 UpTo2u16 u8) => reachable);
}
