use crate::{each, fold, transmute_unchecked, unwrap};
use core::any::type_name_of_val;
use core::fmt::Debug;
use core::marker::PhantomData;

use Endianness::*;
use Pattern::*;

/// Const min fn.
#[rustfmt::skip] const fn min_u8(a: u8, b: u8) -> u8 { if a < b { a } else { b } }
#[rustfmt::skip] const fn min_usize(a: usize, b: usize) -> usize { if a < b { a } else { b } }

/// Const max fn.
#[rustfmt::skip] const fn max_u8(a: u8, b: u8) -> u8 { if a > b { a } else { b } }
#[rustfmt::skip] const fn max_usize(a: usize, b: usize) -> usize { if a > b { a } else { b } }

#[rustfmt::skip] const fn mask(x: u8) -> u128 { if x == 0 { 0 } else { u128::MAX >> (128 - x) } }
#[rustfmt::skip] const fn mask_range(s: u8, e: u8) -> u128 { mask(e + 1) & !mask(s) }

#[rustfmt::skip]
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Endianness { LE, BE }

#[cfg(target_endian = "little")]
const NE: Endianness = LE;
#[cfg(target_endian = "big")]
const NE: Endianness = BE;

impl Endianness {
  const fn read(self, bytes: &[u8]) -> u128 {
    match self {
      LE => fold!(bytes, 0, |i, n, b| n | (*b as u128) << (8 * i)),
      BE => fold!(bytes, 0, |i, n, b| n | (*b as u128) << (8 * (bytes.len() - i - 1))),
    }
  }
}

// A set for any possible byte value.
#[rustfmt::skip]
#[repr(C, packed)]
#[derive(Copy, Clone)]
struct ByteSet { l: u128, h: u128 }

impl ByteSet {
  const ANY: Self = Self { l: !0u128, h: !0u128 };

  const fn empty() -> Self {
    Self { l: 0, h: 0 }
  }

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
      (_, _) => Self { l: 0, h: mask_range(start - 128, end - 128) },
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
  const fn next(mut self) -> Option<(Interval, Self)> {
    if self.is_empty() {
      return None;
    }
    Some(match self.l.trailing_zeros() as u8 {
      128 => {
        let start = self.h.trailing_zeros() as u8;
        let end = start + (self.h >> start).trailing_ones() as u8 - 1;
        self.h &= !mask_range(start, end);
        (Interval { start: start + 128, end: end + 128 }, self)
      }
      start => {
        let mut end = start + (self.l >> start).trailing_ones() as u8 - 1;
        self.l &= !mask_range(start, end);
        match (end, self.h.trailing_ones() as u8) {
          (127, 0) => (Interval { start, end }, self),
          (127, end) => {
            self.h &= !mask_range(0, end - 1);
            (Interval { start, end: end + 127 }, self)
          }
          _ => (Interval { start, end }, self),
        }
      }
    })
  }
}

impl Debug for ByteSet {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let mut s = f.debug_set();
    let mut b = *self;
    while let Some((i, r)) = b.next() {
      s.entry(&i);
      b = r;
    }
    s.finish()
  }
}

#[test]
fn test_byte_set() {
  let mut s = ByteSet::range(1, 3)
    .add_value(5)
    .add_range(7, 9)
    .add_range(125, 222)
    .add_value(233)
    .add_range(240, 255);
  println!("{:?}", s);
  while let Some((i, r)) = s.next() {
    println!("{:?}", i);
    s = r;
  }
}

/// A byte interval.
#[rustfmt::skip]
#[derive(Copy, Clone)]
struct Interval { start: u8, end: u8 }

impl Debug for Interval {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let (start, end) = (self.start, self.end);
    if start == end {
      write!(f, "{:#04x}", start)
    } else if start == 0 && end == 0xff {
      write!(f, "..")
    } else if start == 0 {
      write!(f, "..={:#04x}", end)
    } else if end == 0xff {
      write!(f, "{:#04x}..", start)
    } else {
      write!(f, "{:#04x}..={:#04x}", start, end)
    }
  }
}

#[repr(u8)]
enum BytesPatternTag {
  Uninit,
  Init,
  Skip,
  Any,
  Seq,
}

trait BytesPattern: Sized {
  type Head: BytesPattern;
  type Tail: BytesPattern;
  const TAG: BytesPatternTag;
  /// Width of the pattern in bytes.
  const WIDTH: usize;
}

impl BytesPattern for () {
  type Head = ();
  type Tail = ();
  const TAG: BytesPatternTag = BytesPatternTag::Uninit;
  const WIDTH: usize = 0;
}

impl<const N: usize> BytesPattern for [ByteSet; N] {
  type Head = ();
  type Tail = ();
  const TAG: BytesPatternTag = BytesPatternTag::Init;
  const WIDTH: usize = N;
}

#[repr(transparent)]
struct Skip<S: BytesPattern, T: BytesPattern>(PhantomData<S>, T);

impl<S: BytesPattern, T: BytesPattern> BytesPattern for Skip<S, T> {
  type Head = S;
  type Tail = T;
  const TAG: BytesPatternTag = BytesPatternTag::Skip;
  const WIDTH: usize = T::WIDTH + S::WIDTH;
}

trait Patterns: Copy + Debug {
  type Head: Patterns;
  type Tail: Patterns;
  const LEN: usize;
}

#[derive(Copy, Clone)]
#[repr(C)]
struct Binary<A: Patterns, B: Patterns>(Pattern<A>, Pattern<B>);

impl<A: Patterns + Debug, B: Patterns + Debug> Debug for Binary<A, B> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_list().entry(&self.0).entry(&self.1).finish()
  }
}

struct InAny<'a, T: Patterns>(&'a Binary<T::Head, T::Tail>);

impl<'a, T: Patterns + Debug> Debug for InAny<'a, T> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let Binary(head, tail) = self.0;
    match head {
      Any(x) => InAny::<T::Head>(head.as_pair(x)).fmt(f),
      _ => head.fmt(f),
    }?;
    write!(f, ", ")?;
    match tail {
      Any(x) => InAny::<T::Tail>(tail.as_pair(x)).fmt(f),
      _ => tail.fmt(f),
    }
  }
}

struct InCat<'a, T: Patterns>(&'a Binary<T::Head, T::Tail>);

impl<'a, T: Patterns + Debug> Debug for InCat<'a, T> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let Binary(head, tail) = self.0;
    match head {
      Cat(x) => InCat::<T::Head>(head.as_pair(x)).fmt(f),
      _ => head.fmt(f),
    }?;
    write!(f, ", ")?;
    match tail {
      Cat(x) => InCat::<T::Tail>(tail.as_pair(x)).fmt(f),
      _ => tail.fmt(f),
    }
  }
}

impl Patterns for () {
  type Head = ();
  type Tail = ();
  const LEN: usize = 0;
}

impl<A: Patterns, B: Patterns> Patterns for Binary<A, B> {
  type Head = A;
  type Tail = B;
  const LEN: usize = 2;
}

/// A bytes pattern expression.
#[rustfmt::skip]
#[repr(C, u8)]
#[derive(Copy, Clone)]
enum Pattern<T: Patterns = ()> {
  Uninit(usize), Init(usize, [ByteSet; 16]),
  Any(T), Cat(T),
}

impl<T: Patterns + Debug> Debug for Pattern<T> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Uninit(n) => write!(f, "Uninit({})", n),
      Init(n, init) => write!(f, "Init{:?}", &init[..*n]),
      Any(x) => write!(f, "Any({:?})", InAny::<T>(self.as_pair(x))),
      Cat(x) => write!(f, "Cat({:?})", InCat::<T>(self.as_pair(x))),
    }
  }
}

const fn uninit(n: usize) -> Pattern {
  Uninit(n)
}

const fn init<const N: usize>(init: [(u8, u8); N]) -> Pattern {
  Init(
    N,
    fold!(init, [ByteSet::empty(); 16], |i, r, (s, e)| {
      r[i] = ByteSet::range(*s, *e);
      r
    }),
  )
}

const fn empty<T: Patterns>(n: usize) -> Pattern<T> {
  Init(n, [ByteSet::empty(); 16])
}

type Pattern2<A, B> = Pattern<Binary<A, B>>;
type Pattern3<A, B, C> = Pattern<Binary<A, Binary<B, C>>>;
type Pattern4<A, B, C, D> = Pattern<Binary<A, Binary<B, Binary<C, D>>>>;
type Pattern5<A, B, C, D, E> = Pattern<Binary<A, Binary<B, Binary<C, Binary<D, E>>>>>;
type Pattern6<A, B, C, D, E, F> = Pattern<Binary<A, Binary<B, Binary<C, Binary<D, Binary<E, F>>>>>>;

const fn any2<A: Patterns, B: Patterns>(a: Pattern<A>, b: Pattern<B>) -> Pattern2<A, B> {
  Any(Binary(a, b))
}

const fn any3<A: Patterns, B: Patterns, C: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>,
) -> Pattern3<A, B, C> {
  any2(a, any2(b, c))
}

const fn any4<A: Patterns, B: Patterns, C: Patterns, D: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>,
) -> Pattern4<A, B, C, D> {
  any2(a, any3(b, c, d))
}

const fn any5<A: Patterns, B: Patterns, C: Patterns, D: Patterns, E: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>, e: Pattern<E>,
) -> Pattern5<A, B, C, D, E> {
  any2(a, any4(b, c, d, e))
}

const fn any6<A: Patterns, B: Patterns, C: Patterns, D: Patterns, E: Patterns, F: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>, e: Pattern<E>, f: Pattern<F>,
) -> Pattern6<A, B, C, D, E, F> {
  any2(a, any5(b, c, d, e, f))
}

const fn cat2<A: Patterns, B: Patterns>(a: Pattern<A>, b: Pattern<B>) -> Pattern2<A, B> {
  Cat(Binary(a, b))
}

const fn cat3<A: Patterns, B: Patterns, C: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>,
) -> Pattern3<A, B, C> {
  cat2(a, cat2(b, c))
}

const fn cat4<A: Patterns, B: Patterns, C: Patterns, D: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>,
) -> Pattern4<A, B, C, D> {
  cat2(a, cat3(b, c, d))
}

const fn cat5<A: Patterns, B: Patterns, C: Patterns, D: Patterns, E: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>, e: Pattern<E>,
) -> Pattern5<A, B, C, D, E> {
  cat2(a, cat4(b, c, d, e))
}

const fn cat6<A: Patterns, B: Patterns, C: Patterns, D: Patterns, E: Patterns, F: Patterns>(
  a: Pattern<A>, b: Pattern<B>, c: Pattern<C>, d: Pattern<D>, e: Pattern<E>, f: Pattern<F>,
) -> Pattern6<A, B, C, D, E, F> {
  cat2(a, cat5(b, c, d, e, f))
}

const fn pair<T: Patterns>(head: Pattern<T::Head>, tail: Pattern<T::Tail>) -> T {
  assert!(T::LEN == 2);
  unsafe { transmute_unchecked(Binary(head, tail)) }
}

impl<T: Patterns> Pattern<T> {
  const fn as_next(&self, x: &T) -> &Pattern<T::Head> {
    assert!(T::LEN == 1);
    unsafe { transmute_unchecked(x) }
  }

  const fn as_pair(&self, x: &T) -> &Binary<T::Head, T::Tail> {
    assert!(T::LEN == 2);
    unsafe { transmute_unchecked(x) }
  }

  const fn next(self, x: T) -> Pattern<T::Head> {
    assert!(T::LEN == 1);
    unsafe { transmute_unchecked(x) }
  }

  const fn pair(self, x: T) -> Binary<T::Head, T::Tail> {
    assert!(T::LEN == 2);
    unsafe { transmute_unchecked(x) }
  }

  /// Returns the width of the pattern.
  const fn width(&self) -> usize {
    match self {
      Uninit(n) | Init(n, _) => *n,
      Any(x) => {
        let Binary(head, tail) = self.as_pair(x);
        let width = head.width();
        assert!(width == tail.width());
        width
      }
      Cat(x) => {
        let Binary(head, tail) = self.as_pair(x);
        head.width() + tail.width()
      }
    }
  }

  /// Returns the width of the pattern.
  const fn into_width(self) -> usize {
    match self {
      Uninit(n) | Init(n, _) => n,
      Any(x) => {
        let Binary(head, tail) = self.pair(x);
        let width = head.into_width();
        assert!(width == tail.into_width());
        width
      }
      Cat(x) => {
        let Binary(head, tail) = self.pair(x);
        head.into_width() + tail.into_width()
      }
    }
  }

  /// Returns true if the pattern matches the given bytes.
  const fn matches(&self, bytes: &[u8], e: Endianness) -> bool {
    if bytes.len() < self.width() {
      return false;
    }
    unsafe { self.matches_ptr(bytes.as_ptr(), e) }
  }

  /// Returns true if the pattern matches the given bytes ptr.
  const unsafe fn matches_ptr(self, ptr: *const u8, e: Endianness) -> bool {
    match self {
      Uninit(_) => false,
      // Check if the byte is in the interval.
      Init(n, init) => fold!(init, n, true, |i, r, x| r && x.contains(*ptr.wrapping_add(i))),
      // Check if any pattern matches.
      Any(x) => {
        let Binary(head, tail) = self.pair(x);
        head.matches_ptr(ptr, e) || tail.matches_ptr(ptr, e)
      }
      // Check if the concatenated patterns match.
      Cat(x) => {
        let Binary(head, tail) = self.pair(x);
        head.matches_ptr(ptr, e) && tail.matches_ptr(ptr.wrapping_add(head.into_width()), e)
      }
    }
  }

  const fn union(self, with: Self) -> Self {
    match (self, with) {
      (Init(n, mut a), Init(m, b)) => {
        let n = min_usize(n, m);
        each!(0..n, |i| a[i] = a[i].union(b[i]));
        Init(n, a)
      }
      (Any(x), Any(y)) => {
        let Binary(hx, tx) = self.pair(x);
        let Binary(hy, ty) = self.pair(y);
        Any(pair(hx.union(hy), tx.union(ty)))
      }
      (Cat(x), Cat(y)) => {
        let Binary(hx, tx) = self.pair(x);
        let Binary(hy, ty) = self.pair(y);
        Cat(pair(hx.union(hy), tx.union(ty)))
      }
      _ => todo!(),
    }
  }

  const fn concat(self, with: Self) -> Self {
    match (self, with) {
      (Init(n, mut a), Init(m, b)) => {
        let mut i = n - m;
        while i < n {
          a[i] = b[i];
          i += 1;
        }
        Init(n, a)
      }
      (Any(x), Any(y)) => {
        let Binary(hx, tx) = self.pair(x);
        let Binary(hy, ty) = self.pair(y);
        Any(pair(hx.concat(hy), tx.concat(ty)))
      }
      (Cat(x), Cat(y)) => {
        let Binary(hx, tx) = self.pair(x);
        let Binary(hy, ty) = self.pair(y);
        Cat(pair(hx.concat(hy), tx.concat(ty)))
      }
      _ => todo!(),
    }
  }

  const fn intersects<U: Patterns>(self, with: Pattern<U>) -> (Self, Pattern<U>) {
    match (self, with) {
      (Init(n, mut a), Init(m, mut b)) => {
        each!(0..min_usize(n, m), |i| {
          a[i] = a[i].intersects(b[i]);
          b[i] = b[i].intersects(a[i]);
        });
        (Init(n, a), Init(m, b))
      }
      (Any(x), _) => {
        let Binary(h, t) = self.pair(x);
        let (h, w0) = h.intersects(with);
        let (t, w1) = t.intersects(with);
        (Any(pair(h, t)), w0.union(w1))
      }
      (Cat(x), _) => {
        let Binary(h, t) = self.pair(x);
        let n = h.into_width();
        let (h, w0) = h.intersects(with);
        let (t, w1) = t.intersects(with.skip(n));
        (Cat(pair(h, t)), w0.concat(w1))
      }
      (_, Any(_) | Cat(_)) => {
        let (b, a) = with.intersects(self);
        (a, b)
      }
      _ => todo!(),
    }
  }

  const fn skip(self, s: usize) -> Self {
    match self {
      Init(n, a) => {
        if s >= n {
          return empty(0);
        }
        let mut init = [ByteSet::empty(); 16];
        each!(0..n - s, |i| init[i] = a[i + s]);
        Init(n - s, init)
      }
      Any(x) => {
        let Binary(h, t) = self.pair(x);
        Any(pair(h.skip(s), t.skip(s)))
      }
      Cat(x) => {
        let Binary(h, t) = self.pair(x);
        let n = h.into_width();
        match s > n {
          true => Cat(pair(h.skip(s), t.skip(s - n))),
          _ => Cat(pair(h.skip(s), t)),
        }
      }
      _ => todo!(),
    }
  }
}

#[test]
fn test() {
  println!("{:?}", uninit(0).width());
  println!("{:?}", uninit(1).width());

  let x = any2(init([(21, 42), (21, 42)]), init([(0, 16), (0, 16)]));
  println!("{:?}", x.width());
  println!("{:?}", x.matches(&[21, 21], LE));
  println!("{:?}", x.matches(&[16, 16], LE));
  println!("{:?}", x.matches(&[19, 19], LE));

  let _0or4 = any2(init([(0, 0), (0, 0)]), init([(4, 4), (0, 0)]));
  // println!("{:?}", _0or4);
  // println!("{:?}", _0or4.width());
  // println!("{:?}", _0or4.union(_0or4));
  // println!("{:?}", _0or4.intersects(_0or4));

  let _0or8 = any2(init([(0, 0), (0, 0)]), init([(8, 8), (0, 0)]));
  println!("{:?}", _0or4.intersects(_0or8));
  println!("{:?}", _0or8.intersects(_0or4));

  let _0or4or8 = any3(init([(0, 0), (0, 0)]), init([(4, 4), (0, 0)]), init([(8, 8), (0, 0)]));
  println!("{:?}", _0or4or8);
  println!("{:?}", _0or4or8.width());
  println!("{:?}", _0or4or8.intersects(_0or4));
  println!("{:?}", _0or4or8.intersects(_0or8));

  let _0or4or8or16 = any4(
    init([(0, 0), (0, 0)]),
    init([(4, 4), (0, 0)]),
    init([(8, 8), (0, 0)]),
    init([(16, 16), (0, 0)]),
  );
  let _0or4or10or16 = any4(
    init([(0, 0), (0, 0)]),
    init([(4, 4), (0, 0)]),
    init([(10, 10), (0, 0)]),
    init([(16, 16), (0, 0)]),
  );
  println!("{:#?}", _0or4or8or16);
  println!("{:#?}", _0or4or8or16.width());
  println!("{:#?}", _0or4or8or16.intersects(_0or4));
  println!("{:#?}", _0or4or8or16.intersects(_0or8));
  println!("{:#?}", _0or4or8or16.intersects(_0or4or8));
  println!("{:#?}", _0or4or8or16.intersects(_0or4or8or16));
  println!("{:#?}", _0or4or8or16.intersects(_0or4or10or16));

  let _bool = init([(0, 1)]);
  let _u8 = init([(0, 0xff)]);
  let _u16 = init([(0, 0xff), (0, 0xff)]);
  let _t = cat3(_u16, _bool, _0or8);
  let _u = cat3(_0or8, _u8, _0or4or8);

  println!("{:#?}", (_t, _u));
  println!("{:#?}", _t.intersects(_u));
  println!("{:#?}", _bool.intersects(_bool));
  println!("{:#?}", _bool.intersects(_u8));
  println!("{:#?}", cat2(_u8, _u8).intersects(cat2(_u8, _u8)));
  println!("{:#?}", cat2(_bool, _bool).intersects(cat2(_bool, _bool)));
  println!("{:#?}", cat2(_bool, _u8).intersects(cat2(_u8, _bool)));

  println!("{:#?}", _u16.intersects(_0or8));
}
