#![cfg_attr(not(test), no_std)]
use core::num::NonZeroU8;
use core::marker::PhantomData;
use core::fmt;
use core::fmt::{Debug, Formatter};
use core::mem;

/// Check a condition and return an error if it is `false`.
#[rustfmt::skip]
macro_rules! check { ($c:expr, $e:expr $(,)?) => { if (!$c) { return Err($e); } } }

/// Same as iter.fold() but for const fns.
#[rustfmt::skip]
macro_rules! fold {
  (..$len:expr, $init:expr, |$i:tt, $acc:tt| $x:expr) => {{
    let (mut $i, mut $acc) = (0, $init);
    loop {
      if $i >= $len { break $acc; }
      ($acc, $i) = ($x, $i + 1);
    }
  }};
  ($iter:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $iter.len(), $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    fold!($iter, $iter.len(), $init, |$i, $acc, $it| $x)
  }};
  ($iter:expr, $len:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $len, $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $len:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    let (mut $i, mut $acc) = (0, $init);
    loop {
      if $i >= $len { break $acc; }
      let $it = &$iter[$i];
      ($acc, $i) = ($x, $i + 1);
    }
  }};
}

/// Unwrap an `Option` and panic if it is `None`.
#[rustfmt::skip]
macro_rules! unwrap {
  ($e:expr, $none:expr $(,)?) => (match $e { Some(r) => r, None => $none });
  ($e:expr $(,)?) => (unwrap!($e, unreachable!()));
}

// -------------------------------------------------------------------------------------------------
// `Interval`

/// An inclusive byte interval, as of `start..=end`.
#[derive(Debug, Copy, Clone)]
struct Interval { start: u8, end: u8 }

impl Interval {
  const fn new(start: u8, end: u8) -> Self {
    Self { start, end }
  }

  #[inline(always)]
  const fn contains(self, value: u8) -> bool {
    debug_assert!(self.start <= self.end);
    value >= self.start && value <= self.end
  }
}

// -------------------------------------------------------------------------------------------------
// `ByteSet`

/// A set of byte values.
#[derive(Debug, Copy, Clone)]
struct ByteSet { l: u128, h: u128 }

const fn mask(x: u8) -> u128 { if x == 0 { 0 } else { u128::MAX >> (128 - x) } }

const fn mask_interval(s: u8, e: u8) -> u128 { mask(e + 1) & !mask(s) }

impl ByteSet {
  const fn from_value(val: u8) -> Self {
    match val {
      ..=127 => Self { l: 1 << val, h: 0 },
      _ => Self { l: 0, h: 1 << (val - 128) },
    }
  }

  const fn from_interval(interval: Interval) -> Self {
    let Interval { start, end } = interval;
    assert!(start <= end);
    match (start, end) {
      (..=127, ..=127) => Self { l: mask_interval(start, end), h: 0 },
      (..=127, _) => Self { l: mask_interval(start, 127), h: mask_interval(0, end - 128) },
      _ => Self { l: 0, h: mask_interval(start - 128, end - 128) },
    }
  }

  const fn len(&self) -> usize {
    (self.l.count_ones() + self.h.count_ones()) as usize
  }

  const fn union(self, with: Self) -> Self {
    Self { l: self.l | with.l, h: self.h | with.h }
  }

  const fn intersects(&self, with: &Self) -> Self {
    Self { l: self.l & with.l, h: self.h & with.h }
  }
}

// -------------------------------------------------------------------------------------------------
// `Pattern`

#[derive(Debug, Copy, Clone)]
struct Pattern { mask: Option<NonZeroU8>, interval: Interval }

impl Pattern {
  const fn new(mask: u8, interval: Interval) -> Self {
    let mask = if mask == 0xff { None } else { Some(unwrap!(NonZeroU8::new(mask))) };
    Self { mask, interval }
  }

  const fn contains(&self, value: u8) -> bool {
    match self.mask {
      Some(mask) => self.interval.contains((value & mask.get()) >> mask.trailing_zeros()),
      None => self.interval.contains(value),
    }
  }

  const fn intersects(&self, o: &Self) -> usize {
    self.byteset().intersects(&o.byteset()).len()
  }

  const fn byteset(&self) -> ByteSet {
    let Pattern { mask, interval: Interval { mut start, end } } = *self;
    match mask {
      None => ByteSet::from_interval(Interval::new(start, end)),
      Some(mask) => {
        let mut set = ByteSet::from_value(end << mask.trailing_zeros());
        while start < end {
          set = set.union(ByteSet::from_value(start << mask.trailing_zeros()));
          start += 1;
        }
        set
      }
    }
  }
}

// -------------------------------------------------------------------------------------------------
// `Pack`

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Pack<A, B: ?Sized>(A, B);

// -------------------------------------------------------------------------------------------------
// `Literal`

trait Literal<T>: 'static {
  const VALUE: T;
}

struct Exact<const VALUE: u8>;
struct Range<const START: u8, const END: u8>;
struct RangeFrom<const START: u8>;
struct RangeFull;
struct RangeInclusive<const START: u8, const END: u8>;
struct RangeTo<const END: u8>;
struct RangeToInclusive<const END: u8>;
struct Mask<const MASK: u8, T: Literal<Interval>>(PhantomData<T>);

/// Internal implements.
const _: () = {
  impl<const VALUE: u8> Literal<Interval> for Exact<VALUE> {
    const VALUE: Interval = Interval { start: VALUE, end: VALUE };
  }

  impl<const START: u8, const END: u8> Literal<Interval> for Range<START, END>{
    const VALUE: Interval = Interval { start: START, end: END - 1 };
  }

  impl<const START: u8> Literal<Interval> for RangeFrom<START> {
    const VALUE: Interval = Interval { start: START, end: 0xff };
  }

  impl Literal<Interval> for RangeFull {
    const VALUE: Interval = Interval { start: 0, end: 0xff };
  }

  impl<const START: u8, const END: u8> Literal<Interval> for RangeInclusive<START, END>{
    const VALUE: Interval = Interval { start: START, end: END };
  }

  impl<const END: u8> Literal<Interval> for RangeTo<END> {
    const VALUE: Interval = Interval { start: 0, end: END - 1 };
  }

  impl<const END: u8> Literal<Interval> for RangeToInclusive<END> {
    const VALUE: Interval = Interval { start: 0, end: END };
  }

  impl<T: Literal<Interval>> Literal<Pattern> for T {
    const VALUE: Pattern = Pattern::new(0xff, T::VALUE);
  }

  impl<const M: u8, T: Literal<Interval>> Literal<Pattern> for Mask<M, T> {
    const VALUE: Pattern = Pattern::new(M, T::VALUE);
  }
};

// -------------------------------------------------------------------------------------------------
// `Expr`

/// Bits expressions.
#[derive(Debug, Copy, Clone)]
enum Expr<'a> {
  Void,
  Any(&'a Expr<'a>, &'a Expr<'a>),
  All(&'a Expr<'a>, &'a Expr<'a>),
  Byte(Pattern, &'a Expr<'a>),
}

// -------------------------------------------------------------------------------------------------
// `Bits`

pub trait Bits: Copy + 'static {
  type Lhs: Bits;
  type Rhs: Bits;
  const EXPR: Expr<'static>;

  /// Append `T` to `Self`.
  type Append<T: Bits>: Bits;
}

type Join<L, R> = <L as Bits>::Append<R>;

/// Bits are full maybe uninitialized byte.
#[repr(transparent)]
struct Byte<P>(PhantomData<P>, u8);

/// Bits are either `L` or `R`.
#[repr(transparent)]
#[derive(Copy, Clone)]
struct Any<L: ?Sized, R: ?Sized>(PhantomData<R>, L);

/// Bits are both `L` and `R`.
#[repr(transparent)]
#[derive(Copy, Clone)]
struct All<L: ?Sized, R: ?Sized>(PhantomData<R>, L);

/// Internal implements.
const _: () = {
  impl Bits for () {
    type Lhs = ();
    type Rhs = ();
    const EXPR: Expr<'static> = Expr::Void;
    /// `() + T -> T`
    type Append<T: Bits> = T;
  }

  impl<P> Copy for Byte<P> {}

  impl<P> Clone for Byte<P> {
    fn clone(&self) -> Self {
      *self
    }
  }

  impl<P: Literal<Pattern>> Bits for Byte<P> {
    type Lhs = ();
    type Rhs = ();
    const EXPR: Expr<'static> = Expr::Byte(P::VALUE, &Expr::Void);
    /// `Byte<P> + T -> Pack<Byte<P>, T>`
    type Append<T: Bits> = Pack<Byte<P>, T>;
  }

  impl<P: Literal<Pattern>, Tl: Bits> Bits for Pack<Byte<P>, Tl> {
    type Lhs = Tl;
    type Rhs = ();
    const EXPR: Expr<'static> = Expr::Byte(P::VALUE, &Tl::EXPR);
    /// `Pack<Byte<P>, Tl> + Tr -> Pack<Byte<P>, Tl + Tr>`
    type Append<Tr: Bits> = Pack<Byte<P>, Tl::Append<Tr>>;
  }

  impl<L: Bits, R: Bits> Bits for Any<L, R> {
    type Lhs = L;
    type Rhs = R;
    const EXPR: Expr<'static> = Expr::Any(&L::EXPR, &R::EXPR);
    /// `Any<L, R> + T -> Any<L + T, R + T>`
    type Append<T: Bits> = Any<L::Append<T>, R::Append<T>>;
  }

  impl<L: Bits, R: Bits> Bits for All<L, R> {
    type Lhs = L;
    type Rhs = R;
    const EXPR: Expr<'static> = Expr::All(&L::EXPR, &R::EXPR);
    /// `All<L, R> + T -> All<L + T, R + T>`
    type Append<T: Bits> = All<L::Append<T>, R::Append<T>>;
  }
};

pub const fn size<T: Bits>() -> usize {
  const {
    use Expr::*;
    match T::EXPR {
      Void => 0,
      Byte { .. } => 1 + size::<T::Lhs>(),
      Any { .. } | All { .. } => size::<T::Lhs>(),
    }
  }
}

#[inline(always)]
pub const unsafe fn matches_ptr<T: Bits>(ptr: *const u8) -> bool {
  use Expr::*;
  match T::EXPR {
    Void => true,
    Byte(byte, _) => byte.contains(*ptr) && matches_ptr::<T::Lhs>(ptr.add(1)),
    Any(..) => matches_ptr::<T::Lhs>(ptr) || matches_ptr::<T::Rhs>(ptr),
    All(..) => matches_ptr::<T::Lhs>(ptr) && matches_ptr::<T::Rhs>(ptr),
  }
}

const ANY: u8 = 0b01;
const ALL: u8 = 0b11;

impl<'a> Expr<'a> {
  const fn intersects(&self, b: &Expr) -> u8 {
    use Expr::*;
    match (self, b) {
      (Void, _) => ALL,
      (All(lhs, rhs), _) => lhs.intersects(b) & rhs.intersects(b),
      (Any(lhs, rhs), _) => match (lhs.intersects(b), rhs.intersects(b)) {
        (ALL, ALL) => ALL,
        (0, 0) => 0,
        _ => ANY,
      },
      (_, Void) => unreachable!(),
      (_, All(lhs, rhs)) => self.intersects(lhs) & self.intersects(rhs),
      (_, Any(lhs, rhs)) => match (self.intersects(lhs), self.intersects(rhs)) {
        (ALL, _) | (_, ALL) => ALL,
        (0, 0) => 0,
        _ => ANY,
      }
      (Byte(a, x), Byte(b, y)) => match (a.byteset(), a.intersects(b)) {
        (a, b) if a.len() == b => x.intersects(y),
        (_, b) if b > 0 => ANY & x.intersects(y),
        _ => 0,
      }
    }
  }
}

#[test]
fn test() {
  type Bool = Byte<RangeToInclusive<1>>;
  type U8 = Byte<RangeFull>;
  type U32 = [U8; 4];

  assert_eq!(4, mem::size_of::<U32>());

  println!("{}", size::<Bool>());
  println!("{}", size::<U8>());

  // println!("{:?}", static_analysis::<Bool, Bool>());
  // println!("{:?}", static_analysis::<U8, U8>());
  // println!("{:?}", static_analysis::<Bool, U8>());
  // println!("{:?}", static_analysis::<U8, Bool>());

  type Foo = Join<Bool, U8>;
  type Bar = Join<U8, Bool>;
  type Baz = Any<Foo, Bar>;
  type Yaz = Join<Foo, Bar>;

  println!("{}", size::<Foo>());
  println!("{}", size::<Bar>());
  println!("{}", size::<Baz>());
  println!("{}", size::<Yaz>());

  type _0or4 = Any<
    Byte<Exact<0>>,
    Byte<Exact<4>>,
  >;

  type _0or4or8 = Any<
    _0or4,
    Byte<Exact<8>>,
  >;

  // println!("{:?}", static_analysis::<_0or4, _0or4>());
  // println!("{:?}", static_analysis::<_0or4or8, _0or4or8>());
  // println!("{:?}", static_analysis::<_0or4, _0or4or8>());
  // println!("{:?}", static_analysis::<_0or4or8, _0or4>());

  // println!("{:#05b}", intersects::<Foo, Foo>());
  // println!("{:#05b}", intersects::<Foo, Bar>());
  // println!("{:#05b}", intersects::<Bar, Foo>());
  // println!("{:#05b}", intersects::<Join<U8, U8>, Join<U8, U8>>());
  // println!("{:#05b}", intersects::<Join<Bool, Bool>, Join<Bool, Bool>>());
  //
  // type _0or4 = Bits<byte::Init<0, 0>, (), Bits<byte::Init<4, 4>, (), ()>>;
  // type _0or4or8 = Any<_0or4, Bits<byte::Init<8, 8>, (), ()>>;
  //
  // println!("{:#05b}", intersects::<_0or4, _0or4>());
  // println!("{:#05b}", intersects::<_0or4or8, _0or4or8>());
  // println!("{:#05b}", intersects::<_0or4, _0or4or8>());
  // println!("{:#05b}", intersects::<_0or4or8, _0or4>());
}
