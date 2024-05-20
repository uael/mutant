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
// `MaskedInterval`

#[derive(Debug, Copy, Clone)]
struct MaskedInterval(Option<NonZeroU8>, Interval);

impl MaskedInterval {
  const fn new(mask: u8, interval: Interval) -> Self {
    let mask = if mask == 0xff { None } else { Some(unwrap!(NonZeroU8::new(mask))) };
    Self(mask, interval)
  }

  const fn contains(&self, value: u8) -> bool {
    match self.0 {
      Some(mask) => self.1.contains((value & mask.get()) >> mask.trailing_zeros()),
      None => self.1.contains(value),
    }
  }

  const fn intersects(&self, o: Self) -> ByteSet {
    self.byteset().intersects(&o.byteset())
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
pub enum MaybeUninit<T> { Init(T), Uninit }

impl MaybeUninit<MaskedInterval> {
  const fn contains(&self, value: u8) -> bool {
    use MaybeUninit::*;
    match self {
      Uninit => true,
      Init(interval) => interval.contains(value)
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
// `BitsExpr`

/// Bits expressions.
#[derive(Debug)]
enum BitsExpr {
  Void, Byte(MaybeUninit<MaskedInterval>),
  Array(usize), Slice, Any, All
}

// -------------------------------------------------------------------------------------------------
// `Bits`

pub trait Bits {
  type Lhs: Bits + ?Sized;
  type Rhs: Bits + ?Sized;
  #[allow(private_interfaces)]
  const EXPR: BitsExpr;

  /// Append `T` to `Self`.
  type Append<T: Bits + ?Sized>: Bits + ?Sized;
}

type Join<L, R> = <L as Bits>::Append<R>;

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
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = BitsExpr::Void;
    /// `() + T -> T`
    type Append<T: Bits + ?Sized> = T;
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
    /// `Byte<P> + T -> Pack<Byte<P>, T>`
    type Append<T: Bits + ?Sized> = Pack<Byte<P>, T>;
  }

  impl<P: Literal<MaybeUninit<MaskedInterval>>, Tl: Bits + ?Sized> Bits for Pack<Byte<P>, Tl> {
    type Lhs = Tl;
    type Rhs = ();
    const EXPR: BitsExpr = BitsExpr::Byte(P::VALUE);
    /// `Pack<Byte<P>, Tl> + Tr -> Pack<Byte<P>, Tl + Tr>`
    type Append<Tr: Bits + ?Sized> = Pack<Byte<P>, Tl::Append<Tr>>;
  }

  impl<I: Bits, const N: usize> Bits for [I; N] {
    type Lhs = ();
    type Rhs = I;
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = BitsExpr::Array(N);
    /// `[I; N] + T -> Pack<[I; N], T>`
    type Append<T: Bits + ?Sized> = Pack<[I; N], T>;
  }

  impl<I: Bits, Tl: Bits + ?Sized, const N: usize> Bits for Pack<[I; N], Tl> {
    type Lhs = Tl;
    type Rhs = I;
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = BitsExpr::Array(N);
    /// `Pack<[I; N], Tl> + Tr -> Pack<[I; N], Tl + Tr>`
    type Append<Tr: Bits + ?Sized> = Pack<[I; N], Tl::Append<Tr>>;
  }

  impl<I: Bits> Bits for [I] {
    type Lhs = ();
    type Rhs = I;
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = BitsExpr::Slice;
    /// `[I] + T -> [I]`
    type Append<T: Bits + ?Sized> = [I];
  }

  impl<L: Bits + ?Sized, R: Bits + ?Sized> Bits for Any<L, R> {
    type Lhs = L;
    type Rhs = R;
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = {
      assert!(min_size_of::<L>() == min_size_of::<R>());
      BitsExpr::Any
    };
    /// `Any<L, R> + T -> Any<L + T, R + T>`
    type Append<T: Bits + ?Sized> = Any<L::Append<T>, R::Append<T>>;
  }

  impl<L: Bits + ?Sized, R: Bits + ?Sized> Bits for All<L, R> {
    type Lhs = L;
    type Rhs = R;
    #[allow(private_interfaces)]
    const EXPR: BitsExpr = {
      assert!(min_size_of::<L>() == min_size_of::<R>());
      BitsExpr::All
    };
    /// `All<L, R> + T -> All<L + T, R + T>`
    type Append<T: Bits + ?Sized> = All<L::Append<T>, R::Append<T>>;
  }
};

pub const fn min_size_of<T: Bits + ?Sized>() -> usize {
  const {
    use BitsExpr::*;
    match T::EXPR {
      Void => 0,
      Byte(_) => 1 + min_size_of::<T::Lhs>(),
      Array(len) => len * min_size_of::<T::Rhs>() + min_size_of::<T::Lhs>(),
      Slice => 0,
      Any | All => min_size_of::<T::Lhs>(),
    }
  }
}

pub const fn size_of_dst_slice_item<T: Bits + ?Sized>() -> usize {
  const {
    use BitsExpr::*;
    match T::EXPR {
      Void => 0,
      Byte(_) => size_of_dst_slice_item::<T::Lhs>(),
      Array(_) => size_of_dst_slice_item::<T::Lhs>(),
      Slice => min_size_of::<T::Rhs>(),
      Any | All => size_of_dst_slice_item::<T::Lhs>(),
    }
  }
}

#[inline(always)]
pub const unsafe fn matches_ptr<T: Bits + ?Sized>(ptr: *const u8, metadata: usize) -> bool {
  use BitsExpr::*;
  match T::EXPR {
    Void => true,
    Byte(byte) => matches_ptr::<T::Lhs>(ptr.add(1), metadata) && byte.contains(*ptr),
    Array(len) => matches_ptr::<T::Lhs>(ptr.add(1), metadata) && fold!(..len,
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

const fn resolve<Src: Bits + ?Sized, Dst: Bits + ?Sized>()
  -> Result<(usize, usize, Option<MetadataChecks>), &'static str>
{
  use MetadataChecks::*;
  // Try to resolve `xa[ya] => xb[yb]`, where:
  // - `a` and `b` are `Src` and `Dst` respectively,
  // - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
  // - `ya` and `yb` are the tail slice item sizes of `a` and `b` respectively.
  // - `n` is the minimum number of the source tail slice items needed to align `a` and `b`.
  // - `m` is the source tail slice items multiplier.
  // Calculate minimum and tail slice item sizes for both source and target types.
  let (xa, ya) = (min_size_of::<Src>(), size_of_dst_slice_item::<Src>());
  let (xb, yb) = (min_size_of::<Dst>(), size_of_dst_slice_item::<Dst>());
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
  // Compute the possible size mismatch between source and target.
  // 1. Source isn't a DST:
  let metadata_checks = if !src_is_dst {
    // No size mismatch.
    None
  }
  // 2. Source is a DST and target isn't a DST:
  else if !dst_is_dst {
    // In case the source is smaller than the target, the source metadata must be the minimum
    // source tail slice items count.
    // Otherwise, the source metadata must be empty.
    Some(if xa < xb { Is(n) } else { IsEmpty })
  }
  // 3. Source and target are DSTs:
  else {
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
    // Compute the metadata checks for the source DST tail slice.
    match (n, yb / gcd) {
      // No size mismatch.
      (0, 1) => None,
      // Source metadata must be a multiple of the GCD.
      (0, m) => Some(IsMultipleOf(m)),
      // Source metadata must be at least `n` and a multiple of `m`.
      (n, 1) => Some(IsAtLeast(n)),
      // Source metadata must be at least `n` and a multiple of `m`.
      (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
    }
  };
  // Compute `m` the minimum number of target tail slice items needed to align source and target.
  let m = if yb == 0 { 0 } else { (xa + n * ya - xb) / yb };
  Ok((n, m, metadata_checks))
}

const ANY: u8 = 0b01;
const ALL: u8 = 0b11;

// const fn intersects<Src: Bits + ?Sized, Dst: Bits + ?Sized>(n: usize, m: usize) -> u8 {
//   use BitsExpr::*;
//   use MaybeUninit::*;
//   match (Src::EXPR, Dst::EXPR) {
//     (Void, _) => ALL,
//     (Array(0), _) => intersects::<Src::Lhs, Dst>(n, m),
//     (Array(_), _) => intersects::<Join<Src::Rhs, Src::TailHack>, Dst>(n, m),
//     (Slice, _) if n == 0 => ALL,
//     (Slice, _) => intersects::<Join<Src::Rhs, Src>, Dst>(n - 1, m),
//     (All, _) => intersects::<Src::Lhs, Dst>(n, m) & intersects::<Src::Rhs, Dst>(n, m),
//     (Any, _) => match (intersects::<Src::Lhs, Dst>(n, m), intersects::<Src::Rhs, Dst>(n, m)) {
//       (ALL, ALL) => ALL,
//       (0, 0) => 0,
//       _ => ANY,
//     },
//     (_, Void) => unreachable!(),
//     (_, Array(0)) => intersects::<Src, Dst::Lhs>(n, m),
//     (_, Array(_)) => intersects::<Src, Join<Dst::Rhs, Dst::TailHack>>(n, m),
//     (_, Slice) if m == 0 => ALL,
//     (_, Slice) => intersects::<Src, Join<Dst::Rhs, Dst>>(n, m - 1),
//     (_, All) => intersects::<Src, Dst::Lhs>(n, m) & intersects::<Src, Dst::Rhs>(n, m),
//     (_, Any) => match (intersects::<Src, Dst::Lhs>(n, m), intersects::<Src, Dst::Rhs>(n, m)) {
//       (ALL, _) | (_, ALL) => ALL,
//       (0, 0) => 0,
//       _ => ANY,
//     }
//     (Byte(Uninit), Byte(Uninit)) => intersects::<Src::Lhs, Dst::Lhs>(n, m),
//     (Byte(Uninit), Byte(_)) => todo!(),
//     (Byte(_), Byte(Uninit)) => todo!(),
//     (Byte(Init(a)), Byte(Init(b))) => match (a.byteset(), a.intersects(b)) {
//       (a, b) if a.len() == b.len() => intersects::<Src::Lhs, Dst::Lhs>(n, m),
//       (_, b) if b.len() > 0 => ANY & intersects::<Src::Lhs, Dst::Lhs>(n, m),
//       _ => 0,
//     }
//   }
// }

/// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
///
/// # Internals
/// DSTs metadata represents the length of it's [tail slice](#tail-slice), such as `.1` in
/// `(u16, [u16])`. These checks [validate](MetadataChecks::validate) such metadata to ensure safe
/// and sound transmutations.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MetadataChecks {
  Is(usize),                             // Is equals to `n`
  IsAtLeast(usize),                      // Is at least `n`
  IsMultipleOf(usize),                   // Is a multiple of `m`.
  IsAtLeastThenMultipleOf(usize, usize), // Is at least `n` and a multiple of `m`.
  IsEmpty,                               // Implies no data, i.e. should be zero.
}

impl MetadataChecks {
  /// Validates `metadata` against constraints.
  /// In case of an error `Err(e)`, a positive `e` indicates an overflow of `metadata` by `e`,
  /// while a negative value signifies an underflow by `-e`.
  #[inline]
  pub const fn validate(self, metadata: usize) -> Result<(), isize> {
    use MetadataChecks::*;
    match self {
      Is(n) if metadata == n => Ok(()),
      IsAtLeast(n) if metadata >= n => Ok(()),
      Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
      IsMultipleOf(m) if metadata % m == 0 => Ok(()),
      IsMultipleOf(m) => Err(m as isize - (metadata % m) as isize),
      IsAtLeastThenMultipleOf(n, m) if metadata >= n && (metadata - n) % m == 0 => Ok(()),
      IsAtLeastThenMultipleOf(n, _) if metadata < n => Err(n as isize - metadata as isize),
      IsAtLeastThenMultipleOf(_, m) => Err(m as isize - (metadata % m) as isize),
      IsEmpty if metadata == 0 => Ok(()),
      IsEmpty => Err(-(metadata as isize)),
    }
  }

  /// Same as [`MetadataChecks::validate`] but when unpacking, i.e. the `MultipleOf` checks aren't
  /// checked anymore.
  #[inline]
  pub const fn validate_for_unpack(self, metadata: usize) -> Result<(), isize> {
    use MetadataChecks::*;
    match self {
      Is(n) | IsAtLeast(n) if metadata >= n => Ok(()),
      Is(n) | IsAtLeast(n) => Err(n as isize - metadata as isize),
      IsMultipleOf(_) => Ok(()),
      IsAtLeastThenMultipleOf(n, _) if metadata >= n => Ok(()),
      IsAtLeastThenMultipleOf(n, _) => Err(n as isize - metadata as isize),
      IsEmpty => Ok(()),
    }
  }
}

/// Static analyze result.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) struct StaticAnalyze {
  pub(crate) maybe_size_mismatch: Option<MetadataChecks>,
  pub(crate) maybe_invalid_bit_pattern: bool,
  pub(crate) sound_target_mutation: bool,
}

#[cfg(test)]
impl Debug for StaticAnalyze {
  #[inline]
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let tag = match (self.maybe_invalid_bit_pattern, self.sound_target_mutation) {
      (true, false) => "fallible",
      (true, true) => "fallible_mutable",
      (false, false) => "infallible",
      (false, true) => "infallible_mutable",
    };
    let mut d = f.debug_tuple(tag);
    d.field(&self.maybe_size_mismatch);
    d.finish()
  }
}

pub(crate) const fn static_analysis<Src: Bits + ?Sized, Dst: Bits + ?Sized>()
  -> Result<StaticAnalyze, &'static str>
{
  const {
    match resolve::<Src, Dst>() {
      Err(e) => Err(e),
      Ok((n, m, maybe_size_mismatch)) => {
        let src = ALL; // TODO
        if src == 0 {
          Err("none of the bits patterns in source are valid in target")
        } else {
          let dst = ALL; // TODO
          assert!(dst != 0);
          let maybe_invalid_bit_pattern = src != ALL;
          let sound_target_mutation = dst == ALL;
          Ok(StaticAnalyze {
            maybe_size_mismatch,
            maybe_invalid_bit_pattern,
            sound_target_mutation,
          })
        }
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

  println!("{}", min_size_of::<Bool>());
  println!("{}", min_size_of::<U8>());

  println!("{:?}", static_analysis::<Bool, Bool>());
  println!("{:?}", static_analysis::<U8, U8>());
  println!("{:?}", static_analysis::<Bool, U8>());
  println!("{:?}", static_analysis::<U8, Bool>());

  type Foo = Join<Bool, U8>;
  type Bar = Join<U8, Bool>;
  type Baz = Any<Foo, Bar>;
  type Yaz = Join<Foo, Bar>;

  println!("{}", min_size_of::<Foo>());
  println!("{}", min_size_of::<Bar>());
  println!("{}", min_size_of::<Baz>());
  println!("{}", min_size_of::<Yaz>());

  type A = Join<[Bar; 24234], Join<Baz, [Bar]>>;
  type B = Join<[Foo; 24234], Join<Baz, [Foo]>>;

  assert_eq!((2 * 24234) + 2, mem::size_of::<Join<[Bar; 24234], Baz>>());

  println!("{}", min_size_of::<A>());
  println!("{}", min_size_of::<B>());

  println!("{:?}", static_analysis::<A, A>());
  println!("{:?}", static_analysis::<B, B>());
  println!("{:?}", static_analysis::<A, B>());
  println!("{:?}", static_analysis::<B, A>());

  type _0or4 = Any<
    Byte<Exact<0>>,
    Byte<Exact<4>>,
  >;

  type _0or4or8 = Any<
    _0or4,
    Byte<Exact<8>>,
  >;

  println!("{:?}", static_analysis::<_0or4, _0or4>());
  println!("{:?}", static_analysis::<_0or4or8, _0or4or8>());
  println!("{:?}", static_analysis::<_0or4, _0or4or8>());
  println!("{:?}", static_analysis::<_0or4or8, _0or4>());

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

#[derive(Copy, Clone)]
enum Expr<'a> {
  Void,
  Uninit,
  Init(&'a ByteSet),
  Array(usize, &'a Expr<'a>),
  Slice(&'a Expr<'a>),
  Cat(&'a [Expr<'a>]),
  Any(&'a [Expr<'a>], Option<Option<u8>>),
}

impl<'a> Expr<'a> {
  const fn intersects(&self, mut b: Expr<'a>, mut n: usize) -> u8 {
    use Expr::*;

    let mut a = *self;
    let (mut sa, mut sb) = (([Void; 64], 0), ([Void; 64], 0));

    macro_rules! push {
      ($e:expr => $s:expr $(,$then:expr)?) => {{
        $s.0[$s.1] = $e;
        $s.1 += 1;
        $($then)?
      }};
    }
    macro_rules! top {
      ($s:expr) => { match $s.1 { 0 => None, _ => { Some($s.0[$s.1 - 1]) } } }
    }
    macro_rules! pop {
      ($s:expr) => { match $s.1 { 0 => None, _ => { $s.1 -= 1; Some($s.0[$s.1]) } } }
    }

    let mut i = ALL; // intersection result.
    loop {
      i = match (a, b) {
        (Array(0, _), _) => i,
        (Array(1, i), _) => { a = *i; continue; }
        (Array(n, i), _) => { a = *i; push!(Array(n - 1, i) => sa, continue); }

        (Slice(_), _) if n == 0 => i,
        (Slice(i), _) if n == 1 => { a = *i; n -= 1; continue; }
        (Slice(i), _) => { a = *i; n -= 1; push!(Slice(i) => sa, continue); }

        (Cat([l]), _) => { a = *l; continue; }
        (Cat([l, r @ ..]), _) => { a = *l; push!(Cat(r) => sa, continue); }

        (Any([l, r @ ..], None), _) => {
          a = *l;
          push!(b => sa); // Save `b` state on the `sa` stack.
          push!(Any(r, Some(None)) => sa, continue);
        }
        (Any([l, r @ ..], Some(j)), _) => {
          a = *l;
          b = unwrap!(top!(sa));
          let j = match (i, j) {
            (i, None) => i,
            (ALL, Some(ALL)) => ALL,
            (0, Some(0)) => 0,
            _ => ANY,
          };
          i = ALL;
          push!(Any(r, Some(Some(j))) => sa, continue);
        }
        (Any([], Some(j)), _) => {
          let _ = unwrap!(pop!(sa)); // Remove `b` state from the `sa` stack.
          match (i, j) {
            (i, None) => i,
            (ALL, Some(ALL)) => ALL,
            (0, Some(0)) => 0,
            _ => ANY,
          }
        }

        (_, Array(0, _)) => i,
        (_, Array(1, i)) => { b = *i; continue; }
        (_, Array(n, i)) => { b = *i; push!(Array(n - 1, i) => sb, continue); }

        (_, Slice(i)) => { b = *i; n -= 1; push!(Slice(i) => sb, continue); }

        (_, Cat([l])) => { b = *l; continue; }
        (_, Cat([l, r @ ..])) => { b = *l; push!(Cat(r) => sb, continue); }

        (_, Any([l, r @ ..], None)) => {
          b = *l;
          // TODO: Save previous `i`.
          push!(a => sa); // Duplicate `a` state on the `sa` stack.
          push!(Any(r, Some(None)) => sb, continue);
        }
        (_, Any([l, r @ ..], Some(j))) => {
          b = *l;
          let j = match (i, j) {
            (i, None) => i,
            (ALL, _) | (_, Some(ALL)) => ALL,
            (0, Some(0)) => 0,
            _ => ANY,
          };
          i = ALL;
          push!(a => sa); // Duplicate `a` state on the `sa` stack.
          push!(Any(r, Some(Some(j))) => sb, continue);
        }
        (_, Any([], Some(j))) => {
          // let _ = unwrap!(pop!(sa)); // Remove duplicated `a` state from the `sa` stack.
          match (i, j) {
            (i, None) => i,
            (ALL, _) | (_, Some(ALL)) => ALL,
            (0, Some(0)) => 0,
            _ => ANY,
          }
        }

        (_, Void | Cat([]) | Any([], None)) => i,
        (Void | Cat([]) | Any([], None), _) => i,

        (Uninit, Uninit) => ALL,
        (Uninit, _) => todo!(),
        (_, Uninit) => todo!(),
        (Init(a), Init(b)) => {
          let x = match (a, a.intersects(b)) {
            (a, b) if a.len() == b.len() => i & ALL,
            (_, b) if b.len() > 0 => i & ANY,
            _ => 0,
          };
          // println!("test {:?} n {:?} -> {}", a, b, x);
          x
        }
      };
      a = match pop!(sa) { None => break, Some(a) => a };
      b = match pop!(sb) { None => Void, Some(b) => b };
    }
    i
  }
}

const _0: ByteSet = ByteSet::from_interval(Interval::new(0, 0));
const _4: ByteSet = ByteSet::from_interval(Interval::new(4, 4));
const _8: ByteSet = ByteSet::from_interval(Interval::new(8, 8));
const _0OR4OR8: Expr = Expr::Any(&[Expr::Init(&_0), Expr::Init(&_4), Expr::Init(&_8)], None);
const _ULTIMATE_TEST: Expr = Expr::Array(8192, &_0OR4OR8);

const _X: u8 = _ULTIMATE_TEST.intersects(_ULTIMATE_TEST, 0);

#[test]
fn test_expr() {
  use Expr::*;

  let a = ByteSet::from_interval(Interval::new(1, 1));
  let b = ByteSet::from_interval(Interval::new(2, 2));
  let c = ByteSet::from_interval(Interval::new(3, 3));
  let d = ByteSet::from_interval(Interval::new(4, 4));
  let a = Init(&a);
  let b = Init(&b);
  let c = Init(&c);
  let d = Init(&d);

  let binding = [a, b, c, d];
  let binding = [Cat(&binding), b, c, d];
  let binding = [a, Cat(&binding), c, d];
  let binding = [a, b, Cat(&binding), d];
  let e = Cat(&binding);
  let f = Array(4096, &e);

  println!("{:#05b}", f.intersects(f, 0));

  let binding = [Init(&_0), Init(&_4)];
  let _0or4 = Any(&binding, None);
  let binding = [Init(&_0), Init(&_4), Init(&_8)];
  let _0or4or8 = Any(&binding, None);

  println!("{:#05b}", _0or4.intersects(Init(&_0), 0));
  println!("{:#05b}", _0or4.intersects(Init(&_4), 0));
  println!("{:#05b}", _0or4.intersects(Init(&_8), 0));
  println!("{:#05b}", _0or4.intersects(_0or4, 0));
  println!("{:#05b}", _0or4or8.intersects(_0or4or8, 0));
  println!("{:#05b}", _0or4.intersects(_0or4or8, 0));
  println!("{:#05b}", _0or4or8.intersects(_0or4, 0));

  let ultimate_test = Array(4096 * 100, &_0or4or8);
  println!("{:#05b}", ultimate_test.intersects(ultimate_test, 0));
}
