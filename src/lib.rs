use core::num::NonZeroU8;
use core::num::NonZeroUsize;
use core::marker::PhantomData;

/// Unwrap an `Option` and panic if it is `None`.
#[rustfmt::skip]
macro_rules! unwrap {
  ($e:expr, $none:expr $(,)?) => (match $e { Some(r) => r, None => $none });
  ($e:expr $(,)?) => (unwrap!($e, unreachable!()));
}

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

#[derive(Debug, Copy, Clone)]
struct Interval {
  start: u8,
  end: u8,
}

#[derive(Debug, Copy, Clone)]
enum Byte {
  Initialized(Interval),
  Uninitialized,
}

#[derive(Debug, Copy, Clone)]
enum MaybeOob<T> {
  InBounds(T),
  OutOfBounds,
}

enum ExprTag {
  TVoid, TAny, TAll,
  TByte { byte: Byte, mask: Option<NonZeroU8> },
  TOff { off: usize }, TArr { len: NonZeroUsize },
}

/// Layout is a single initialized byte ranging from `START` to `END`.
struct Init<const START: u8 = 0, const END: u8 = 0xff, const MASK: u8 = 0xff>;

/// Layout is a single uninitialized byte.
struct Uninit;

/// Layout is `L` or `R`.
struct Any<L, R>(PhantomData<(L, R)>);

/// Layout is `L` and `R`.
struct All<L, R>(PhantomData<(L, R)>);

/// Layout is `T` offset by `OFF` bytes.
struct Off<const OFF: usize, T>(PhantomData<T>);

trait Bits {
  const TAG: ExprTag;
  type Lhs: Bits;
  type Rhs: Bits;
}

use Byte::*;
use MaybeOob::*;
use ExprTag::*;

const fn size<T: Bits>() -> usize {
  match T::TAG {
    TVoid => 0,
    TByte { .. } => 1,
    TOff { off } => off + size::<T::Lhs>(),
    TArr { len } => len.get() * size::<T::Lhs>(),
    _ => match (size::<T::Lhs>(), size::<T::Rhs>()) {
      (a, b) if a > b => a,
      (_, b) => b
    },
  }
}

const ANY: u8 = 0b01;
const ALL: u8 = 0b11;

const fn intersects<A: Bits, B: Bits>(x: usize) -> u8 {
  match (A::TAG, B::TAG) {
    (TVoid, _) => ALL,
    (TByte { byte, .. }, _) => match (byte, byte.intersects::<B>(x, 0)) {
      (a, b) if a.len() == b.len() => ALL,
      (_, b) if b.len() > 0 => ANY,
      _ => 0,
    },
    (TOff { off }, _) => intersects::<A::Lhs, B>(x + off),
    (TArr { len }, _) => fold!(..len.get(), intersects::<A::Lhs, B>(x), |j, acc| match acc {
      0 => 0,
      i => i & intersects::<A::Lhs, B>(x + j * size::<A::Lhs>())
    }),
    (TAll, _) => match intersects::<A::Lhs, B>(x) {
      0 => 0,
      i => i & intersects::<A::Rhs, B>(x),
    }
    (TAny, _) => match (intersects::<A::Lhs, B>(x), intersects::<A::Rhs, B>(x)) {
      (ALL, ALL) => ALL,
      (0, 0) => 0,
      _ => ANY,
    }
  }
}

#[inline(always)]
const unsafe fn matches<T: Bits>(ptr: *const u8) -> bool {
  match T::TAG {
    TVoid => true,
    TAny => matches::<T::Lhs>(ptr) || matches::<T::Rhs>(ptr),
    TAll => matches::<T::Lhs>(ptr) && matches::<T::Rhs>(ptr),
    TByte { byte, mask: Some(mask) } => byte.contains(*ptr & mask.get()),
    TByte { byte, .. } => byte.contains(*ptr),
    TOff { off } => matches::<T::Lhs>(ptr.add(off)),
    TArr { len } => fold!(..len.get(),
      true,
      |j, acc| acc && matches::<T::Lhs>(ptr.add(j * size::<T::Lhs>()))
    )
  }
}

/// Internal implements.
const _: () =  {
  // -----------------------------------------------------------------------------------------------
  // impl `Bits`

  impl Bits for () {
    const TAG: ExprTag = TVoid;
    type Lhs = ();
    type Rhs = ();
  }

  impl Bits for Uninit {
    const TAG: ExprTag = TByte { byte: Uninitialized, mask: None };
    type Lhs = ();
    type Rhs = ();
  }

  impl<L: Bits, T: Bits> Bits for Any<L, T> {
    const TAG: ExprTag = TAny;
    type Lhs = L;
    type Rhs = T;
  }

  impl<L: Bits, T: Bits> Bits for All<L, T> {
    const TAG: ExprTag = TAll;
    type Lhs = L;
    type Rhs = T;
  }

  impl<const START: u8, const END: u8, const MASK: u8> Bits for Init<START, END, MASK> {
    const TAG: ExprTag = TByte {
      byte: Initialized(Interval { start: START, end: END }),
      mask: if MASK == 0xff { None } else { Some(unwrap!(NonZeroU8::new(MASK))) }
    };
    type Lhs = ();
    type Rhs = ();
  }

  impl<const OFF: usize, T: Bits> Bits for Off<OFF, T> {
    const TAG: ExprTag = TOff { off: OFF };
    type Lhs = T;
    type Rhs = ();
  }

  impl<const N: usize, T: Bits> Bits for [T; N] {
    const TAG: ExprTag = TArr { len: unwrap!(NonZeroUsize::new(N)) };
    type Lhs = T;
    type Rhs = ();
  }

  // -----------------------------------------------------------------------------------------------
  // impl `MaybeOob`

  impl MaybeOob<Byte> {
    const fn len(&self) -> usize {
      match self {
        InBounds(byte) => byte.len(),
        OutOfBounds => 0,
      }
    }

    const fn union(self, other: Self) -> Self {
      match (self, other) {
        (InBounds(a), InBounds(b)) => InBounds(a.union(b)),
        (InBounds(a), _) | (_, InBounds(a)) => InBounds(a),
        _ => OutOfBounds,
      }
    }

    const fn intersection(self, other: Self) -> Self {
      match (self, other) {
        (InBounds(a), InBounds(b)) => InBounds(a.intersection(b)),
        _ => OutOfBounds,
      }
    }
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Byte`

  impl Byte {
    const fn len(&self) -> usize {
      match self {
        Uninitialized => 1,
        Initialized(interval) => interval.len(),
      }
    }

    const fn contains(&self, value: u8) -> bool {
      match self {
        Uninitialized => true,
        Initialized(interval) => interval.contains(value)
      }
    }

    const fn union(self, o: Self) -> Self {
      match (self, o) {
        (Initialized(a), Initialized(b)) => Initialized(a.union(b)),
        (Initialized(a), _) | (_, Initialized(a)) => Initialized(a),
        _ => Uninitialized,
      }
    }

    const fn intersection(self, o: Self) -> Self {
      match (self, o) {
        (Uninitialized, _) | (_, Uninitialized) => Uninitialized,
        (Initialized(a), Initialized(b)) => Initialized(a.intersects(b)),
      }
    }

    const fn intersects<T: Bits>(&self, x: usize, y: usize) -> MaybeOob<Self> {
      if x > y + size::<T>() {
        return OutOfBounds;
      }
      match T::TAG {
        // This path is unreachable because of the above if, but left here for completeness.
        TVoid => OutOfBounds,
        TOff { off } => self.intersects::<T::Lhs>(x, y + off),
        TArr { len } => fold!(..len.get(),
          self.intersects::<T::Lhs>(x, y),
          |j, acc| acc.intersection(self.intersects::<T::Lhs>(x, y + (j * size::<T::Lhs>())))
        ),
        TAny => self.intersects::<T::Lhs>(x, y).union(self.intersects::<T::Rhs>(x, y)),
        TAll => self.intersects::<T::Lhs>(x, y).intersection(self.intersects::<T::Rhs>(x, y)),
        _ if x != y => return InBounds(*self),
        TByte { byte, .. } => InBounds(self.intersection(byte)),
      }
    }
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Interval`

  impl Interval {
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
        (a, b) if a.len() == 0 => b,
        (a, b) if b.len() == 0 => a,
        (Self { start: a, end: b }, Self { start: x, end: y }) => {
          let start = if a < x { a } else { x };
          let end = if b > y { b } else { y };
          Self { start, end }
        }
      }
    }

    const fn intersects(self, o: Self) -> Self {
      match (self, o) {
        (a, _) if a.len() == 0 => a,
        (_, b) if b.len() == 0 => b,
        (Self { start: a, end: b }, Self { start: x, end: y }) => {
          let start = if a > x { a } else { x };
          let end = if b < y { b } else { y };
          Self { start, end }
        }
      }
    }
  }
};

type _0or4 = Any<
  All<Init<0, 0>, Off<1, Init<0, 0xff>>>,
  All<Init<4, 4>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8 = Any<
  _0or4,
  All<Init<8, 8>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12 = Any<
  _0or4or8,
  All<Init<12, 12>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16 = Any<
  _0or4or8or12,
  All<Init<16, 16>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20 = Any<
  _0or4or8or12or16,
  All<Init<20, 20>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24 = Any<
  _0or4or8or12or16or20,
  All<Init<24, 24>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24or28 = Any<
  _0or4or8or12or16or20or24,
  All<Init<28, 28>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24or28or32 = Any<
  _0or4or8or12or16or20or24or28,
  All<Init<32, 32>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24or28or32or36 = Any<
  _0or4or8or12or16or20or24or28or32,
  All<Init<36, 36>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24or28or32or36or40 = Any<
  _0or4or8or12or16or20or24or28or32or36,
  All<Init<40, 40>, Off<1, Init<0, 0xff>>>,
>;

type _0or4or8or12or16or20or24or28or32or36or40or44 = Any<
  _0or4or8or12or16or20or24or28or32or36or40,
  All<Init<44, 44>, Off<1, Init<0, 0xff>>>,
>;

#[test]
fn test() {
  println!("{}", size::<_0or4>());
  println!("{}", size::<_0or4or8>());
  println!("{}", size::<_0or4or8or12>());
  println!("{}", size::<_0or4or8or12or16>());
  println!("{}", size::<_0or4or8or12or16or20>());
  println!("{}", size::<_0or4or8or12or16or20or24>());
  println!("{}", size::<_0or4or8or12or16or20or24or28>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36or40>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36or40or44>());
  println!("{:#04b}", intersects::<_0or4, _0or4>(0));
  println!("{:#04b}", intersects::<_0or4or8, _0or4or8>(0));
  println!("{:#04b}", intersects::<_0or4, _0or4or8>(0));
  println!("{:#04b}", intersects::<_0or4or8, _0or4>(0));

  type BoolPattern = Init<0, 1>;
  type U8Pattern = Init<0, 0xff>;

  println!("{:#04b}", intersects::<BoolPattern, U8Pattern>(0));
  println!("{:#04b}", intersects::<U8Pattern, BoolPattern>(0));
}

pub const unsafe fn test_0or4(ptr: *const u8) -> bool {
  matches::<_0or4>(ptr)
}

pub const unsafe fn test_0or4or8(ptr: *const u8) -> bool {
  matches::<_0or4or8>(ptr)
}

pub const unsafe fn test_0or4or8or12(ptr: *const u8) -> bool {
  matches::<_0or4or8or12>(ptr)
}

pub const unsafe fn test_0or4or8or12or16(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20or24(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20or24>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20or24or28(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20or24or28>(ptr)
}
