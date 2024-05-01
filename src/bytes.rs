use core::fmt::{self, Debug};
use core::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

#[rustfmt::skip] const fn mask(x: u8) -> u128 { if x == 0 { 0 } else { u128::MAX >> (128 - x) } }
#[rustfmt::skip] const fn mask_range(s: u8, e: u8) -> u128 { mask(e + 1) & !mask(s) }

// A set for any possible byte value.
#[rustfmt::skip]
#[repr(C, align(8))]
#[derive(Copy, Clone, Eq, PartialEq)]
struct ByteSet { l: u128, h: u128 }

impl ByteSet {
  const ANY: Self = Self { l: !0u128, h: !0u128 };
  const EMPTY: Self = Self { l: 0, h: 0 };

  const fn value(val: u8) -> Self {
    match val {
      ..=127 => Self { l: 1 << val, h: 0 },
      _ => Self { l: 0, h: 1 << (val - 128) },
    }
  }

  const fn range(start: u8, end: u8) -> Self {
    assert!(start <= end);
    match (start, end) {
      (..=127, ..=127) => Self { l: mask_range(start, end), h: 0 },
      (..=127, _) => Self { l: mask_range(start, 127), h: mask_range(0, end - 128) },
      _ => Self { l: 0, h: mask_range(start - 128, end - 128) },
    }
  }

  const fn add_value(self, val: u8) -> Self {
    self.union(Self::value(val))
  }

  const fn add_range(self, start: u8, end: u8) -> Self {
    self.union(Self::range(start, end))
  }

  const fn len(&self) -> usize {
    (self.l.count_ones() + self.h.count_ones()) as usize
  }

  const fn is_empty(&self) -> bool {
    self.l == 0 && self.h == 0
  }

  const fn is_any(&self) -> bool {
    self.l == !0 && self.h == !0
  }

  const fn contains(&self, val: u8) -> bool {
    match val {
      ..=127 => (self.l & (1 << val)) != 0,
      128.. => (self.h & (1 << (val - 128))) != 0,
    }
  }

  const fn union(self, with: Self) -> Self {
    Self { l: self.l | with.l, h: self.h | with.h }
  }

  const fn intersects(self, with: Self) -> Self {
    Self { l: self.l & with.l, h: self.h & with.h }
  }

  /// Returns the next contiguous interval in the set if any, and the remaining set.
  const fn next(mut self) -> Option<((u8, u8), Self)> {
    if self.is_empty() {
      return None;
    }
    Some(match self.l.trailing_zeros() as u8 {
      128 => {
        let start = self.h.trailing_zeros() as u8;
        let end = start + (self.h >> start).trailing_ones() as u8 - 1;
        self.h &= !mask_range(start, end);
        ((start + 128, end + 128), self)
      }
      start => {
        let end = start + (self.l >> start).trailing_ones() as u8 - 1;
        self.l &= !mask_range(start, end);
        match (end, self.h.trailing_ones() as u8) {
          (127, 0) => ((start, end), self),
          (127, end) => {
            self.h &= !mask_range(0, end - 1);
            ((start, end + 127), self)
          }
          _ => ((start, end), self),
        }
      }
    })
  }
}

macro_rules! byteset {
  ($e:expr $(,)?) => { Interval($e).build() };
  ($e:expr, $($tt:tt)+) => { byteset!($e).union(byteset!($($tt)+)) };
}

macro_rules! pattern {
  (_ $(,)?) => { () };
  ([{$($l:tt)+} $(,)?] $(,)?) => { byteset!($($l)+) };
  ([{$($l:tt)+}, $($tt:tt)+] $(,)?) => { Cat(byteset!($($l)+), pattern!([$($tt)+])) };
  ([$e:expr $(,)?] $(,)?) => { byteset!($e) };
  ([$e:expr, $($tt:tt)+] $(,)?) => { Cat(byteset!($e), pattern!([$($tt)+])) };
  ({$($l:tt)+} $(,)?) => { byteset!($($l)+) };
  (($($l:tt)+) $(,)?) => { pattern!($($l)+) };
  ($l:tt | $($r:tt)+) => { Any(pattern!($l), pattern!($($r)+)) };
}

impl<T: Pattern> Interval<T> {
  const fn build(self) -> T {
    self.0
  }
}

macro_rules! impl_intervals {
  ($($ty:ty |$x:tt| $e:expr)+) => {
    #[repr(transparent)]
    struct Interval<T>(T);
    $(impl Interval<$ty> { const fn build(self) -> ByteSet { let $x = self.0; $e } })+
  };
}

impl_intervals! {
  u8 |x| ByteSet::value(x)
  Range<u8> |x| ByteSet::range(x.start, x.end - 1)
  RangeFrom<u8> |x| ByteSet::range(x.start, 0xff)
  RangeFull |_| ByteSet::range(0, 0xff)
  RangeInclusive<u8> |x| ByteSet::range(*x.start(), *x.end())
  RangeTo<u8> |x| ByteSet::range(0, x.end - 1)
  RangeToInclusive<u8> |x| ByteSet::range(0, x.end)
}

#[rustfmt::skip]
#[repr(u8)]
#[derive(Copy, Clone)]
enum ExprTag { Uninit, Init, Any, Cat }

#[derive(Debug, Copy, Clone)]
enum Expr<T: Pattern> {
  Uninit,
  Init(ByteSet),
  Any(T::Lhs, T::Rhs),
  Cat(T::Lhs, T::Rhs),
}

#[repr(C, align(8))]
#[derive(Copy, Clone)]
struct Any<L, R>(pub L, pub R);

#[repr(C, align(8))]
#[derive(Copy, Clone)]
struct Cat<L, R>(pub L, pub R);

/// # Safety
/// `Self` must be equivalent to a `Expr<Self::Lhs, Self::Rhs>` struct.
trait Pattern: Copy + Debug {
  type Lhs: Pattern;
  type Rhs: Pattern;
  const TAG: ExprTag;
  const WIDTH: usize;
}

const fn decode<T: Pattern>(pattern: T) -> Expr<T> {
  match T::TAG {
    ExprTag::Uninit => Expr::Uninit,
    ExprTag::Init => Expr::Init(unsafe { crate::transmute_unchecked(pattern) }),
    ExprTag::Any => {
      let Any(lhs, rhs) = unsafe { crate::transmute_unchecked(pattern) };
      Expr::Any(lhs, rhs)
    }
    ExprTag::Cat => {
      let Cat(lhs, rhs) = unsafe { crate::transmute_unchecked(pattern) };
      Expr::Cat(lhs, rhs)
    }
  }
}

const fn union<T: Pattern>(x: T, y: T) -> T {
  decode(x).union(decode(y)).encode()
}

const fn intersects<T: Pattern, U: Pattern>(x: T, y: U) -> (T, U) {
  let (x, y) = decode(x).intersects(decode(y), 0, 0);
  (x.encode(), y.encode())
}

impl Pattern for () {
  type Lhs = Self;
  type Rhs = ();
  const TAG: ExprTag = ExprTag::Uninit;
  const WIDTH: usize = 1;
}

impl Pattern for ByteSet {
  type Lhs = Self;
  type Rhs = ();
  const TAG: ExprTag = ExprTag::Init;
  const WIDTH: usize = 1;
}

impl<L: Pattern, R: Pattern> Pattern for Any<L, R> {
  type Lhs = L;
  type Rhs = R;
  const TAG: ExprTag = ExprTag::Any;
  const WIDTH: usize = {
    assert!(L::WIDTH == R::WIDTH);
    L::WIDTH
  };
}

impl<L: Pattern, R: Pattern> Pattern for Cat<L, R> {
  type Lhs = L;
  type Rhs = R;
  const TAG: ExprTag = ExprTag::Cat;
  const WIDTH: usize = L::WIDTH + R::WIDTH;
}

impl<T: Pattern> Expr<T> {
  const fn encode(self) -> T {
    use Expr::*;
    match self {
      Uninit => unsafe { crate::transmute_unchecked(()) },
      Init(byte) => unsafe { crate::transmute_unchecked(byte) },
      Any(lhs, rhs) => unsafe { crate::transmute_unchecked(self::Any(lhs, rhs)) },
      Cat(lhs, rhs) => unsafe { crate::transmute_unchecked(self::Cat(lhs, rhs)) },
    }
  }

  const fn union(self, other: Self) -> Self {
    use Expr::*;
    match (self, other) {
      (Uninit, Uninit) => Uninit,
      (Init(lhs), Init(rhs)) => Init(lhs.union(rhs)),
      (Any(xl, xr), Any(yl, yr)) => Any(union(xl, yl), union(xr, yr)),
      (Cat(xl, xr), Cat(yl, yr)) => Cat(union(xl, yl), union(xr, yr)),
      _ => unreachable!(),
    }
  }

  const fn intersects<U: Pattern>(self, other: Expr<U>, xo: usize, yo: usize) -> (Self, Expr<U>) {
    use Expr::*;

    const fn any<T: Pattern, U: Pattern>(
      lhs: T::Lhs, rhs: T::Rhs, y: Expr<U>, xo: usize, yo: usize,
    ) -> (Expr<T>, Expr<U>) {
      let (lhs, y0) = decode(lhs).intersects(y, xo, yo);
      let (rhs, y1) = decode(rhs).intersects(y, xo, yo);
      (Any(lhs.encode(), rhs.encode()), y0.union(y1))
    }

    const fn cat<T: Pattern, U: Pattern>(
      lhs: T::Lhs, rhs: T::Rhs, y: Expr<U>, xo: usize, yo: usize,
    ) -> (Expr<T>, Expr<U>) {
      let (lhs1, y) = decode(lhs).intersects(y, xo, yo);
      let (rhs1, y) = decode(rhs).intersects(y, xo, yo + <T::Lhs as Pattern>::WIDTH);
      (Cat(lhs1.encode(), rhs1.encode()), y)
    }

    const fn flip<T: Copy, U: Copy>(x: (T, U)) -> (U, T) {
      (x.1, x.0)
    }

    if xo != yo && matches!((self, other), (Uninit | Init(_), Uninit | Init(_))) {
      return (self, other);
    }

    match (self, other) {
      (Uninit, Uninit) => (Uninit, Uninit),
      (Uninit, Init(_)) => (Uninit, Init(ByteSet::EMPTY)),
      (Init(_), Uninit) => (Init(ByteSet::EMPTY), Uninit),
      (Init(x), Init(y)) => (Init(x.intersects(y)), Init(y.intersects(x))),
      (Any(lhs, rhs), y) => any(lhs, rhs, y, xo, yo),
      (x, Any(lhs, rhs)) => flip(any(lhs, rhs, x, yo, xo)),
      (Cat(lhs, rhs), y) => cat(lhs, rhs, y, xo, yo),
      (x, Cat(lhs, rhs)) => flip(cat(lhs, rhs, x, yo, xo)),
    }
  }
}

#[test]
fn test_union() {
  let bool = pattern!({ ..=1 });
  let u8 = pattern!({ .. });
  let u16 = pattern!([.., ..]);
  let nonzerou16 = pattern!([1.., ..] | [.., 1..]);

  let _0or4or10or16u8 = pattern!({0, 4, 10, 16});
  let _0or4or10or16u16 = pattern!([0, ..] | [4, ..] | [10, ..] | [16, ..]);
  let _0or4or10or16u16_v2 = pattern!([{0, 4, 10, 16}, ..]);

  println!("{:?}", intersects(pattern!([1.., ..=3]), pattern!([2.., ..=4])));
}

impl Debug for ByteSet {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let ((mut start, mut end), mut rest) = match self.next() {
      Some(x) => x,
      None => return write!(f, "_"),
    };
    let many = !rest.is_empty();
    if many {
      write!(f, "{{")?;
    }
    loop {
      if start == end {
        write!(f, "{}", start)
      } else if start == 0 && end == 0xff {
        write!(f, "..")
      } else if start == 0 {
        write!(f, "..={}", end)
      } else if end == 0xff {
        write!(f, "{}..", start)
      } else {
        write!(f, "{}..={}", start, end)
      }?;
      ((start, end), rest) = match rest.next() {
        Some(x) => {
          write!(f, ", ")?;
          x
        }
        None if many => break write!(f, "}}"),
        None => break Ok(()),
      };
    }
  }
}

impl<L: Pattern, R: Pattern> Debug for Any<L, R> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "({:?})", DebugOperand(self, self))
  }
}

impl<L: Pattern, R: Pattern> Debug for Cat<L, R> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "[{:?}]", DebugOperand(self, self))
  }
}

struct DebugOperand<'a, S: Pattern, O: Pattern>(&'a S, &'a O);

impl<'a, S: Pattern, O: Pattern> Debug for DebugOperand<'a, S, O> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let sep = match O::TAG {
      ExprTag::Uninit if O::WIDTH == 1 => return write!(f, "_"),
      ExprTag::Uninit => return write!(f, "[_; {}]", O::WIDTH),
      ExprTag::Init => return Debug::fmt(self.1, f),
      tag if tag as u8 != S::TAG as u8 => return Debug::fmt(self.1, f),
      ExprTag::Any => " | ",
      _ => ", ",
    };
    let Any(lhs, rhs) = unsafe { crate::transmute_unchecked::<_, &Any<O::Lhs, O::Rhs>>(self.1) };
    write!(f, "{:?}{}{:?}", DebugOperand(self.0, lhs), sep, DebugOperand(self.0, rhs))
  }
}
