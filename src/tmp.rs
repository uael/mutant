use core::num::{NonZeroU8, NonZeroUsize};
use core::marker::PhantomData;
use core::mem;

/// Unwrap a `Result` and return the error if it is `Err`.
#[rustfmt::skip]
macro_rules! bail {
  ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => return Err(e) });
  ($e:expr, $x:ident => $map:expr $(,)?) => (match $e { Ok(r) => r, Err($x) => return Err($map) });
}

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

const fn min(a: u8, b: u8) -> u8 {
  if a < b { a } else { b }
}

const fn max(a: u8, b: u8) -> u8 {
  if a > b { a } else { b }
}

impl Interval {
  const fn new(start: u8, end: u8) -> Self {
    Self { start, end }
  }

  const fn len(self) -> usize {
    let Self { start, end } = self;
    if start > end { 0 } else { (end - start) as usize + 1 }
  }

  #[inline(always)]
  const fn contains(self, value: u8) -> bool {
    debug_assert!(self.start <= self.end);
    value >= self.start && value <= self.end
  }

  const fn union(self, o: Self) -> Self {
    match (self, o) {
      (a, b) | (b, a) if a.len() == 0 => b,
      (Self { start: a, end: b }, Self { start: x, end: y }) => Self::new(min(a, x), max(b, y)),
    }
  }

  const fn intersects(self, o: Self) -> Self {
    match (self, o) {
      (a, _) | (_, a) if a.len() == 0 => a,
      (Self { start: a, end: b }, Self { start: x, end: y }) => Self::new(max(a, x), min(b, y)),
    }
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

  const fn intersects(self, with: Self) -> Self {
    Self { l: self.l & with.l, h: self.h & with.h }
  }
}

// -------------------------------------------------------------------------------------------------
// `MaskedInterval`

#[derive(Debug, Copy, Clone)]
struct MaskedInterval(Option<NonZeroU8>, Interval);

impl MaskedInterval {
  const fn new(mask: u8, interval: Interval) -> Self {
    let mask = if mask == 0xff { None } else { NonZeroU8::new(mask) };
    Self(mask, interval)
  }

  const fn contains(&self, value: u8) -> bool {
    match self.0 {
      Some(mask) => self.1.contains((value & mask.get()) >> mask.trailing_zeros()),
      None => self.1.contains(value),
    }
  }

  const fn intersects(&self, o: Self) -> ByteSet {
    self.byteset().intersects(o.byteset())
  }

  const fn byteset(&self) -> ByteSet {
    let MaskedInterval(mask, Interval { mut start, end }) = *self;
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
// `MaybeUninit`

/// `T` is either initialized or uninitialized.
#[derive(Debug, Copy, Clone)]
enum MaybeUninit<T> { Init(T), Uninit }

impl MaybeUninit<Interval> {
  const fn union(self, o: Self) -> Self {
    use MaybeUninit::*;
    match (self, o) {
      (Init(a), Init(b)) => Init(a.union(b)),
      (Init(a), _) | (_, Init(a)) => Init(a),
      _ => Uninit,
    }
  }

  const fn intersects(self, o: Self) -> Self {
    use MaybeUninit::*;
    match (self, o) {
      (Uninit, _) | (_, Uninit) => Uninit,
      (Init(a), Init(b)) => Init(a.intersects(b)),
    }
  }
}

impl MaybeUninit<ByteSet> {
  const fn union(self, o: Self) -> Self {
    use MaybeUninit::*;
    match (self, o) {
      (Init(a), Init(b)) => Init(a.union(b)),
      (Init(a), _) | (_, Init(a)) => Init(a),
      _ => Uninit,
    }
  }

  const fn intersects(self, o: Self) -> Self {
    use MaybeUninit::*;
    match (self, o) {
      (Uninit, _) | (_, Uninit) => Uninit,
      (Init(a), Init(b)) => Init(a.intersects(b)),
    }
  }
}

impl MaybeUninit<MaskedInterval> {
  const fn len(&self) -> usize {
    use MaybeUninit::*;
    match self {
      Uninit => 256,
      Init(interval) => interval.1.len(),
    }
  }

  const fn contains(&self, value: u8) -> bool {
    use MaybeUninit::*;
    match self {
      Uninit => true,
      Init(interval) => interval.contains(value)
    }
  }

  const fn byteset(&self) -> MaybeUninit<ByteSet> {
    use MaybeUninit::*;
    match self {
      Uninit => Uninit,
      Init(interval) => Init(interval.byteset()),
    }
  }

  const fn intersects(self, o: Self) -> MaybeUninit<ByteSet> {
    use MaybeUninit::*;
    match (self, o) {
      (Uninit, _) | (_, Uninit) => Uninit,
      (Init(a), Init(b)) => Init(a.byteset().intersects(b.byteset())),
    }
  }
}

// -------------------------------------------------------------------------------------------------
// `Pack`

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Pack<A, B: ?Sized>(A, B);

// -------------------------------------------------------------------------------------------------
// `BitsExpr`

/// Bits expressions.
#[derive(Debug)]
enum BitsExpr {
  Void, Byte(MaybeUninit<MaskedInterval>),
  Array(NonZeroUsize), Slice, Any, All
}

// -------------------------------------------------------------------------------------------------
// `Literal`

trait Literal<T> {
  const VALUE: T;
}

struct Uninit;
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
  impl Literal<MaybeUninit<MaskedInterval>> for Uninit {
    const VALUE: MaybeUninit<MaskedInterval> = MaybeUninit::Uninit;
  }

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

  impl<T: Literal<Interval>> Literal<MaybeUninit<MaskedInterval>> for T {
    const VALUE: MaybeUninit<MaskedInterval> = MaybeUninit::Init(
      MaskedInterval::new(0xff, T::VALUE)
    );
  }

  impl<const M: u8, T: Literal<Interval>> Literal<MaybeUninit<MaskedInterval>> for Mask<M, T> {
    const VALUE: MaybeUninit<MaskedInterval> = MaybeUninit::Init(
      MaskedInterval::new(M, T::VALUE)
    );
  }
};

// -------------------------------------------------------------------------------------------------
// mod `ops`

mod ops {
  use super::*;

  /// Join two bits-patterns.
  pub trait Join<Rhs: Bits + ?Sized>: Bits {
    type Output: Bits + ?Sized;
  }

  /// `() + T = T`
  impl<T: Bits> Join<T> for () {
    type Output = T;
  }

  /// `Pack<L, R> + T = Pack<L, R + T>`
  impl<L: Bits, R: Join<T> + ?Sized, T: Bits + ?Sized> Join<T> for Pack<L, R>
  where
    Self: Bits,
    Pack<L, R::Output>: Bits
  {
    type Output = Pack<L, R::Output>;
  }

  /// `Byte<P> + T = Pack<Byte<P>, T>
  impl<P: Literal<MaybeUninit<MaskedInterval>>, T: Bits + ?Sized> Join<T> for Byte<P> {
    type Output = Pack<Byte<P>, T>;
  }

  /// `[I; N] + T = Pack<[I; N], T>`
  impl<I: Bits, const N: usize, T: Bits + ?Sized> Join<T> for [I; N] {
    type Output = Pack<[I; N], T>;
  }

  /// `Any<L, R> + T = Any<L + T, R + T>`
  impl<L: Join<T>, R: Join<T>, T: Bits + ?Sized> Join<T> for Any<L, R> {
    type Output = Any<L::Output, R::Output>;
  }

  /// `All<L, R> + T = All<L + T, R + T>`
  impl<L: Join<T>, R: Join<T>, T: Bits + ?Sized> Join<T> for All<L, R> {
    type Output = All<L::Output, R::Output>;
  }
}

type Join<L, R> = <L as ops::Join<R>>::Output;

// -------------------------------------------------------------------------------------------------
// `Bits`

trait Bits {
  type Lhs: Bits + ?Sized;
  type Rhs: Bits + ?Sized;
  const EXPR: BitsExpr;
}

/// Bits are full maybe uninitialized byte.
#[repr(transparent)]
struct Byte<P>(PhantomData<P>, u8);

/// Bits are either `L` or `R`.
#[repr(transparent)]
#[derive(Copy, Clone)]
struct Any<L: ?Sized, R: ?Sized> { rhs: PhantomData<R>, lhs: L }

/// Bits are both `L` and `R`.
#[repr(transparent)]
#[derive(Copy, Clone)]
struct All<L: ?Sized, R: ?Sized> { rhs: PhantomData<R>, lhs: L }

/// Internal implements.
const _: () = {
  impl Bits for () {
    type Lhs = ();
    type Rhs = ();
    const EXPR: BitsExpr = BitsExpr::Void;
  }

  impl<P> Copy for Byte<P> {}

  impl<P> Clone for Byte<P> {
    fn clone(&self) -> Self {
      *self
    }
  }

  impl<P: Literal<MaybeUninit<MaskedInterval>>> Bits for Byte<P> {
    type Lhs = ();
    type Rhs = ();
    const EXPR: BitsExpr = BitsExpr::Byte(P::VALUE);
  }

  impl<P: Literal<MaybeUninit<MaskedInterval>>, T: Bits + ?Sized> Bits for Pack<Byte<P>, T> {
    type Lhs = T;
    type Rhs = ();
    const EXPR: BitsExpr = BitsExpr::Byte(P::VALUE);
  }

  impl<I: Bits, const N: usize> Bits for [I; N] {
    type Lhs = ();
    type Rhs = I;
    const EXPR: BitsExpr = BitsExpr::Array(unwrap!(NonZeroUsize::new(N)));
  }

  impl<I: Bits, T: Bits + ?Sized, const N: usize> Bits for Pack<[I; N], T> {
    type Lhs = T;
    type Rhs = I;
    const EXPR: BitsExpr = BitsExpr::Array(unwrap!(NonZeroUsize::new(N)));
  }

  impl<I: Bits> Bits for [I] {
    type Lhs = ();
    type Rhs = I;
    const EXPR: BitsExpr = BitsExpr::Slice;
  }

  impl<L: Bits + ?Sized, R: Bits + ?Sized> Bits for Any<L, R> {
    type Lhs = L;
    type Rhs = R;
    const EXPR: BitsExpr = {
      assert!(min_size_of::<L>() == min_size_of::<R>());
      BitsExpr::Any
    };
  }

  impl<L: Bits + ?Sized, R: Bits + ?Sized> Bits for All<L, R> {
    type Lhs = L;
    type Rhs = R;
    const EXPR: BitsExpr = {
      assert!(min_size_of::<L>() == min_size_of::<R>());
      BitsExpr::All
    };
  }
};

const fn min_size_of<T: Bits + ?Sized>() -> usize {
  const {
    use BitsExpr::*;
    match T::EXPR {
      Void => 0,
      Byte(_) => 1 + min_size_of::<T::Lhs>(),
      Array(len) => len.get() * min_size_of::<T::Rhs>() + min_size_of::<T::Lhs>(),
      Slice => 0,
      Any | All => min_size_of::<T::Lhs>(),
    }
  }
}

const fn size_of_tail_slice_item<T: Bits + ?Sized>() -> usize {
  const {
    use BitsExpr::*;
    match T::EXPR {
      Void => 0,
      Byte(_) => size_of_tail_slice_item::<T::Lhs>(),
      Array(_) => size_of_tail_slice_item::<T::Lhs>(),
      Slice => min_size_of::<T::Rhs>(),
      Any | All => size_of_tail_slice_item::<T::Lhs>(),
    }
  }
}

const unsafe fn matches_ptr<T: Bits + ?Sized>(ptr: *const u8, metadata: usize) -> bool {
  use BitsExpr::*;
  match T::EXPR {
    Void => true,
    Byte(byte) => matches_ptr::<T::Lhs>(ptr.add(1), metadata) && byte.contains(*ptr),
    Array(len) => matches_ptr::<T::Lhs>(ptr.add(1), metadata) && fold!(..len.get(),
      true,
      |j, m| m && matches_ptr::<T::Rhs>(ptr.add(j * min_size_of::<T::Rhs>()), metadata)
    ),
    Slice => fold!(..metadata,
      true,
      |j, m| m && matches_ptr::<T::Lhs>(ptr.add(j * min_size_of::<T::Rhs>()), metadata)
    ),
    Any => matches_ptr::<T::Lhs>(ptr, metadata) || matches_ptr::<T::Rhs>(ptr, metadata),
    All => matches_ptr::<T::Lhs>(ptr, metadata) && matches_ptr::<T::Rhs>(ptr, metadata),
  }
}

const ANY: u8 = 0b01;
const ALL: u8 = 0b11;

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum Intersection { Empty = 0, Incomplete = ANY, Complete = ALL }

const fn resolve<Src: Bits + ?Sized, Dst: Bits + ?Sized>()
  -> Result<(usize, usize, usize), &'static str>
{
  // const {
    resolve_inner::<Src, Dst>()
  // }
}

const fn resolve_inner<Src: Bits + ?Sized, Dst: Bits + ?Sized>()
  -> Result<(usize, usize, usize), &'static str>
{
  // Try to resolve `xa[ya] => xb[yb]`, where:
  // - `a` and `b` are `Src` and `Dst` respectively,
  // - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
  // - `ya` and `yb` are the tail slice item sizes of `a` and `b` respectively.
  // - `n` is the minimum number of the source tail slice items needed to align `a` and `b`.
  // - `m` is the source tail slice items multiplier.
  // Calculate minimum and tail slice item sizes for both source and target types.
  let (xa, ya) = (min_size_of::<Src>(), size_of_tail_slice_item::<Src>());
  let (xb, yb) = (min_size_of::<Dst>(), size_of_tail_slice_item::<Dst>());
  // Compute if source and target are DSTs, i.e. had a tail slice with a non-zero item size.
  let (src_is_dst, dst_is_dst) = (ya != 0, yb != 0);
  // Compute `n` the minimum source tail slice items needed to align source and target.
  // 1. Source is smaller than target:
  //  - Check for source to be a DST.
  //  - Compute the missing bytes to fit the target.
  //  - In case the target isn't a DST, check if the missing bytes are a multiple of
  //    the source DST tail slice item size.
  //  - The minimum source tail slice items is the missing bytes ceil divided by the source DST
  //    tail slice item size.
  let mut n = if xa < xb {
    // Check for source to be a DST.
    check!(src_is_dst, "source too small to fit target");
    // Compute the missing bytes to fit the target.
    let missing = xb - xa;
    // In case the target isn't a DST:
    if !dst_is_dst {
      // Check if the missing bytes are a multiple of the source DST tail slice item size.
      check!(
        missing % ya == 0,
        "missing bytes to fit the target is not a multiple of the source DST trailing slice \
         item size"
      );
    }
    // The minimum source tail slice items is the missing bytes ceil divided by the source DST
    // tail slice item size.
    (missing + ya - 1) / ya
  }
  // 2. Source is larger or equal to target:
  //  - The minimum source tail slice items is 0.
  else {
    // In case the target is a DST:
    // - Compute the remaining bytes to fit in the target.
    // - In case the source isn't a DST, check if the remaining bytes is a multiple
    //   of the target DST tail slice item size.
    if dst_is_dst {
      // Compute the remaining bytes to fit in the target.
      let remaining = xa - xb;
      // In case the source isn't a DST:
      if !src_is_dst {
        // Check if the remaining bytes is a multiple of the target DST tail slice item size.
        check!(
          remaining % yb == 0,
          "remaining bytes to fit in the target is not a multiple of the target DST trailing \
           slice item size",
        );
      }
    }
    // The minimum source tail slice items is 0.
    0
  };
  // Both source and target are DSTs:
  if src_is_dst && dst_is_dst {
    /// Greatest common divisor and least common multiple calculations to determine the alignment
    /// and size compatibility.
    const fn gcd(a: usize, b: usize) -> usize {
      match b {
        0 => a,
        _ => gcd(b, a % b),
      }
    }
    // Compute the GCD of the source and target DST trailing slice item sizes.
    let gcd = gcd(ya, yb);
    // Compute the absolute difference between the source and target DST minimum sizes.
    let absolute_miss = if xa < xb { xb - xa } else { xa - xb };
    // Make sure the source can catch up on the target.
    check!(absolute_miss % gcd == 0, "DST source cannot catch up on DST target");
    // Compute the minimum number of source tail slice items needed to align source and target.
    // We need to find `n` in a way that `xa + n*ya - yb` is a multiple of `yb`. Although this
    // could be resolved using complex equations, for the sake of readability, we can simplify it
    // by using the following loop:
    // - `n * ya` is the total size in bytes of the source tail slice for `n` source items.
    // - `xa + n * ya` is the total size in bytes of the source for `n` source items.
    // - `(xa + n * ya - xb)` is the total size of the target tail slice for `n` source items.
    // If the latter is a multiple of `yb`, then we found the minimum number of source tail slice
    // items needed to align source and target.
    while (xa + n * ya - xb) % yb != 0 {
      n += 1; // Increment the minimum number of source tail slice items by 1.
    }
    return Ok((gcd / ya, (xa + gcd + n * ya - xb) / yb, gcd));
  }
  // Compute `m` the minimum number of target tail slice items needed to align source and target.
  let m = if yb == 0 { 0 } else { (xa + n * ya - xb) / yb };
  Ok((n, m, 0))
}

fn n_inner<Src: Bits + ?Sized, Dst: Bits + ?Sized>(n: usize, m: usize) -> u8 {
  use BitsExpr::*;
  use MaybeUninit::*;
  // TODO: array's aren't working at all:
  //  the intersections is applied to `Dst` instead of `Dst` at the right offset.
  // println!(
  //   "{:?} <> {:?}\n{} <> {}",
  //   Src::EXPR, Dst::EXPR,
  //   core::any::type_name::<Src>().to_string().replace("mutant::tmp::", ""),
  //   core::any::type_name::<Dst>().to_string().replace("mutant::tmp::", ""),
  // );
  match (Src::EXPR, Dst::EXPR) {
    (Void, _) => ALL,
    // TODO: how the fuck did I write this without getting how bad its is ?
    //  is my algo even possible then ?? am I fucked-up again ??
    // (Array(len), _) => (
    //   fold!(..len.get(), ALL, |j, x| x & n_inner::<Src::Rhs, Dst>(n, m))
    //     & n_inner::<Src::Lhs, Dst>(n, m)
    // ),
    (Array(_), _) => todo!(),
    (Slice, _) => fold!(..n, ALL,
      |j, x| x & n_inner::<Src::Rhs, Dst>(n, m)
    ),
    (All, _) => n_inner::<Src::Lhs, Dst>(n, m) & n_inner::<Src::Rhs, Dst>(n, m),
    (Any, _) => match (n_inner::<Src::Lhs, Dst>(n, m), n_inner::<Src::Rhs, Dst>(n, m)) {
      (ALL, ALL) => ALL,
      (0, 0) => 0,
      _ => ANY,
    },
    (Byte(_), Void) => panic!(),
    // TODO: how the fuck did I write this without getting how bad its is ?
    //  is my algo even possible then ?? am I fucked-up again ??
    // (Byte(_), Array(len)) => (
    //   fold!(..len.get(), ALL, |j, x| x & n_inner::<Src, Dst::Rhs>(n, m))
    //     & n_inner::<Src, Dst::Lhs>(n, m)
    // ),
    (Byte(_), Array(_)) => todo!(),
    (Byte(_), Slice) => fold!(..m, ALL,
      |j, x| x & n_inner::<Src, Dst::Rhs>(n, m)
    ),
    (Byte(_), All) => n_inner::<Src, Dst::Lhs>(n, m) & n_inner::<Src, Dst::Rhs>(n, m),
    (Byte(_), Any) => match (n_inner::<Src, Dst::Lhs>(n, m), n_inner::<Src, Dst::Rhs>(n, m)) {
      (ALL, _) | (_, ALL) => ALL,
      (0, 0) => 0,
      _ => ANY,
    }
    (Byte(Uninit), Byte(Uninit)) => n_inner::<Src::Lhs, Dst::Lhs>(n, m),
    (Byte(Uninit), Byte(_)) => todo!(),
    (Byte(_), Byte(Uninit)) => todo!(),
    (Byte(Init(a)), Byte(Init(b))) => match (a.byteset(), a.intersects(b)) {
      (a, b) if a.len() == b.len() => n_inner::<Src::Lhs, Dst::Lhs>(n, m),
      (_, b) if b.len() > 0 => ANY & n_inner::<Src::Lhs, Dst::Lhs>(n, m),
      _ => 0,
    }
  }
}

fn intersects<Src: Bits + ?Sized, Dst: Bits + ?Sized>()
  -> Result<Intersection, &'static str>
{
  // const {
    match resolve::<Src, Dst>() {
      Err(e) => Err(e),
      Ok((n, m, _)) => {
        let flags = n_inner::<Src, Dst>(n, m);
        Ok(unsafe { mem::transmute(flags) })
      }
    }
  // }
}

#[test]
fn test() {
  type Bool = Byte<RangeToInclusive<1>>;
  type U8 = Byte<RangeFull>;

  println!("{}", min_size_of::<Bool>());
  println!("{}", min_size_of::<U8>());

  // println!("{:?}", intersects::<Bool, Bool>());
  // println!("{:?}", intersects::<U8, U8>());
  // println!("{:?}", intersects::<Bool, U8>());
  // println!("{:?}", intersects::<U8, Bool>());

  type Foo = Join<Bool, U8>;
  type Bar = Join<U8, Bool>;
  type Baz = Any<Foo, Bar>;
  type Yaz = Join<Foo, Bar>;

  println!("{}", core::any::type_name::<Foo>());
  println!("{}", core::any::type_name::<Bar>());
  println!("{}", core::any::type_name::<Baz>());
  println!("{}", core::any::type_name::<Yaz>());

  println!("{}", min_size_of::<Foo>());
  println!("{}", min_size_of::<Bar>());
  println!("{}", min_size_of::<Baz>());
  println!("{}", min_size_of::<Yaz>());

  type A = Join<Foo, Join<Baz, [Bar]>>;
  type B = Join<Bar, Join<Baz, [Foo]>>;

  println!("{}", min_size_of::<A>());
  println!("{}", min_size_of::<B>());

  println!("{:?}", intersects::<A, A>());
  println!("{:?}", intersects::<B, B>());
  println!("{:?}", intersects::<A, B>());
  println!("{:?}", intersects::<B, A>());

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
