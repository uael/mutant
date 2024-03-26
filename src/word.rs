use core::marker::PhantomData;
use core::num::{NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};
use core::ops::Range;
use core::{fmt, hint, mem, slice};

/// Represent a type `T` valid bit pattern.
#[repr(C, packed)]
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct BitPattern<T: Copy> {
  bytes: T,
  kind: u8,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct WordLayout<T: Word = ()> {
  pub width: usize,
  patterns: &'static [u8],
  target: PhantomData<T>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct BitWeave<const N: usize = 16> {
  pub width: usize,
  bytes: [u8; N],
  kinds: [u8; N],
}

/// Marker trait for plain word types (e.g. integers or enum discriminants).
///
/// # Safety
/// - Implementor is responsible for `WORD_LAYOUT` to represents only valid values for `Self`.
/// - The size of `Self::Raw` must be equals to the size of `Self`.
/// - The `Self::Raw` type must accept any bit pattern.
pub unsafe trait Word: Copy + 'static {
  /// Any bit pattern representation of `Self`, e.g. `u8` for `NonZeroU8`.
  type Raw: Word<Raw = Self::Raw>;
  /// All valid bit patterns for this type.
  const WORD_LAYOUT: WordLayout<Self>;
}

const ANY: &[u8] = &[0xff; 1 + mem::size_of::<u128>()];
const MASK: u8 = 0xff;
const ALLOW: u8 = 0x01;
const DISCARD: u8 = 0x02;

impl<T: Word> BitPattern<T> {
  #[inline(always)]
  const fn new(bytes: T, kind: u8) -> Self {
    assert!(T::WORD_LAYOUT.is_any());
    assert!(mem::size_of::<T>().is_power_of_two());
    Self { bytes, kind }
  }

  #[inline(always)]
  pub const fn mask(bytes: T) -> Self {
    Self::new(bytes, MASK)
  }

  #[inline(always)]
  pub const fn allow(bytes: T) -> Self {
    Self::new(bytes, ALLOW)
  }

  #[inline(always)]
  pub const fn discard(bytes: T) -> Self {
    Self::new(bytes, DISCARD)
  }

  #[inline(always)]
  pub const fn width(&self) -> usize {
    mem::size_of::<T>()
  }

  #[inline(always)]
  pub const fn matches(&self, value: T) -> bool {
    macro_rules! matches {
      ($val:ident as $ty:ty) => {{
        let (val, pat): ($ty, $ty) = unsafe { (cast_unchecked($val), cast_unchecked(self.bytes)) };
        match self.kind {
          MASK => val | pat == pat,
          ALLOW => val == pat,
          DISCARD => val != pat,
          _ => unsafe { hint::unreachable_unchecked() }
        }
      }};
    }

    const unsafe fn cast_unchecked<Src: Copy, Dst: Copy>(src: Src) -> Dst {
      union Cast<Src: Copy, Dst: Copy> {
        src: Src,
        dst: Dst,
      }
      Cast { src }.dst
    }
    
    match mem::size_of::<T>() {
      1 => matches!(value as u8),
      2 => matches!(value as u16),
      4 => matches!(value as u32),
      8 => matches!(value as u64),
      16 => matches!(value as u128),
      _ => unreachable!(),
    }
  }

  #[inline(always)]
  pub const fn downcast(&self) -> BitWeave {
    let bytes = unsafe { slice::from_raw_parts((self as *const Self).cast(), mem::size_of::<T>()) };
    let mut weave = BitWeave { width: self.width(), bytes: [0; 16], kinds: [self.kind; 16] };
    let mut i = 0;
    while i < bytes.len() {
      weave.bytes[i] = bytes[i];
      i += 1;
    }
    weave
  }
}

impl<T: Word> WordLayout<T> {
  #[inline(always)]
  pub const fn new(patterns: &'static [BitPattern<T::Raw>]) -> Self {
    assert!(mem::size_of::<T>() == mem::size_of::<T::Raw>());
    let patterns = if patterns.is_empty() {
      ANY.split_at(1 + mem::size_of::<T>()).0
    } else {
      unsafe {
        slice::from_raw_parts(
          patterns.as_ptr().cast(),
          patterns.len() * mem::size_of::<BitPattern<T::Raw>>(),
        )
      }
    };
    Self { width: mem::size_of::<T>(), patterns, target: PhantomData }
  }

  #[inline(always)]
  pub const fn any() -> Self {
    Self::new(&[])
  }

  #[inline(always)]
  pub const fn get(&self, i: usize) -> &BitPattern<T::Raw> {
    let chunk = self.patterns.split_at(i * self.chunk()).1;
    unsafe { &*chunk.as_ptr().cast() }
  }

  #[inline(always)]
  pub const fn matches(&self, val: T::Raw) -> bool {
    let mut i = 0;
    while i < self.patterns() {
      if !self.get(i).matches(val) {
        return false;
      }
      i += 1;
    }
    true
  }

  #[inline(always)]
  pub const fn is_any(&self) -> bool {
    let mut i = 0;
    while i < self.patterns() {
      if self.get(i).downcast().is_any() {
        return true;
      }
      i += 1;
    }
    false
  }

  #[inline(always)]
  pub const fn count(&self) -> u128 {
    let mut i = 0;
    let mut count = 0;
    while i < self.patterns() {
      count += self.get_raw(i).count();
      i += 1;
    }
    count
  }

  #[inline(always)]
  pub const fn intersects<const N: usize>(&self, pat: BitWeave<N>) -> u128 {
    let mut i = 0;
    let mut count = 0;
    while i < self.patterns() {
      count += self.get_raw(i).intersects(pat);
      i += 1;
    }
    count
  }

  #[inline(always)]
  pub const fn patterns(&self) -> usize {
    self.patterns.len() / self.chunk()
  }

  #[inline(always)]
  pub const fn get_raw(&self, i: usize) -> BitWeave {
    let chunk = self.patterns.split_at(i * self.chunk()).1.split_at(self.width);
    let mut weave = BitWeave { width: self.width, bytes: [0; 16], kinds: [chunk.1[0]; 16] };
    let mut i = 0;
    while i < chunk.0.len() {
      weave.bytes[i] = chunk.0[i];
      i += 1;
    }
    weave
  }

  #[inline(always)]
  const fn chunk(&self) -> usize {
    1 + self.width
  }

  #[inline(always)]
  pub const fn downcast(&self) -> &WordLayout {
    unsafe { mem::transmute(self) }
  }
}

impl<const N: usize> BitWeave<N> {
  #[inline(always)]
  pub const fn new() -> Self {
    Self { width: 0, bytes: [0; N], kinds: [0; N] }
  }

  #[inline(always)]
  pub const fn is_any(&self) -> bool {
    let mut i = 0;
    while i < self.width {
      if self.kinds[i] != MASK || self.bytes[i] != 0xff {
        return false;
      }
      i += 1;
    }
    true
  }

  #[inline(always)]
  pub const fn append<const M: usize>(mut self, pat: BitWeave<M>) -> Self {
    let total_width = self.width + pat.width;
    assert!(total_width <= N, "not enough capacity");
    let mut i = 0;
    while i < pat.width {
      self.bytes[self.width + i] = pat.bytes[i];
      self.kinds[self.width + i] = pat.kinds[i];
      i += 1;
    }
    self.width += i;
    self
  }

  #[inline(always)]
  pub const fn shift(self, mut n: usize) -> Self {
    if n > self.width {
      n = self.width;
    }
    let (mut bytes, mut kinds, mut i) = ([0; N], [0; N], 0);
    while i < self.width - n {
      bytes[i] = self.bytes[n + i];
      kinds[i] = self.kinds[n + i];
      i += 1;
    }
    Self { width: i, bytes, kinds }
  }

  #[inline(always)]
  pub const fn shrink(mut self, n: usize) -> Self {
    assert!(n <= self.width);
    self.width = n;
    self
  }

  #[inline(always)]
  pub const fn slice(&self, range: Range<usize>) -> Self {
    self.shift(range.start).shrink(range.end - range.start)
  }

  #[inline(always)]
  pub const fn count(&self) -> u128 {
    if self.width == 0 {
      return 0;
    }
    let mut count = 1;
    let mut i = 0;
    while i < self.width {
      count *= match self.kinds[i] {
        MASK => 1 << self.bytes[i].count_ones(),
        ALLOW => 1,
        DISCARD => 255,
        _ => unreachable!(),
      };
      i += 1;
    }
    count
  }

  #[inline(always)]
  pub const fn intersects<const M: usize>(&self, pat: BitWeave<M>) -> u128 {
    assert!(pat.width == self.width);
    if self.width == 0 {
      return 0;
    }
    let mut count = 1;
    let mut i = 0;
    while i < self.width {
      let (a, b) = (self.bytes[i], pat.bytes[i]);
      count *= match (self.kinds[i], pat.kinds[i]) {
        (MASK, MASK) => 1 << (a & b).count_ones(),
        (MASK, ALLOW) if a & b == b => 1,
        (MASK, DISCARD) if a & b == 0 => 1 << a.count_ones(), // TODO
        (ALLOW, ALLOW) if a == b => 1,
        (ALLOW, _) => 0,
        (DISCARD, DISCARD) if a == b => 255,
        (DISCARD, ALLOW) if a != b => 1,
        (DISCARD, MASK) if a & b != a => 1 << b.count_ones(), // TODO
        _ => 0,
      };
      i += 1;
    }
    count
  }
}

unsafe impl Word for () {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl<T: ?Sized + 'static> Word for PhantomData<T> {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for u8 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for u16 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for u32 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for u64 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for u128 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for usize {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for i8 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for i16 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for i32 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for i64 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for i128 {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for isize {
  type Raw = Self;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for Option<NonZeroU8> {
  type Raw = u8;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for Option<NonZeroU16> {
  type Raw = u16;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for Option<NonZeroU32> {
  type Raw = u32;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for Option<NonZeroU64> {
  type Raw = u64;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for Option<NonZeroUsize> {
  type Raw = usize;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::any();
}

unsafe impl Word for bool {
  type Raw = u8;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::mask(0b1)]);
}

unsafe impl Word for NonZeroU8 {
  type Raw = u8;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::discard(0)]);
}

unsafe impl Word for NonZeroU16 {
  type Raw = u16;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::discard(0)]);
}

unsafe impl Word for NonZeroU32 {
  type Raw = u32;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::discard(0)]);
}

unsafe impl Word for NonZeroU64 {
  type Raw = u64;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::discard(0)]);
}

unsafe impl Word for NonZeroUsize {
  type Raw = usize;
  const WORD_LAYOUT: WordLayout<Self> = WordLayout::new(&[BitPattern::discard(0)]);
}

impl fmt::Debug for BitPattern<&[u8]> {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    struct Bytes<'a>(&'a [u8]);

    impl<'a> fmt::LowerHex for Bytes<'a> {
      fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        if f.alternate() {
          f.write_str("0x")?;
        }
        for byte in self.0 {
          f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
      }
    }

    impl<'a> fmt::Binary for Bytes<'a> {
      fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        if f.alternate() {
          f.write_str("0b")?;
        }
        for byte in self.0 {
          f.write_fmt(format_args!("{:08b}", byte))?;
        }
        Ok(())
      }
    }

    match self.kind {
      MASK => f.write_fmt(format_args!("{:#b}", Bytes(self.bytes))),
      ALLOW => f.write_fmt(format_args!("{:#x}", Bytes(self.bytes))),
      DISCARD => f.write_fmt(format_args!("!{:#x}", Bytes(self.bytes))),
      _ => unreachable!(),
    }
  }
}

impl<const N: usize> fmt::Debug for BitWeave<N> {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    let mut d = f.debug_list();
    for i in 0..self.width {
      let kind = self.kinds[i];
      let mut j = i + 1;
      while j < self.width && self.kinds[i] == self.kinds[j] {
        j += 1;
      }
      let bytes = &self.bytes[i..j];
      d.entry(&BitPattern { bytes, kind });
    }
    d.finish()
  }
}

impl<T: Word> fmt::Debug for WordLayout<T> {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    let mut d = f.debug_list();
    for i in 0..self.patterns() {
      d.entry(&self.get_raw(i));
    }
    d.finish()
  }
}
