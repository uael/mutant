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

use ExprTag::*;
enum ExprTag {
  TVoid, TUninit, TAny, TAll,
  TInit { byte: Interval }, TOff { off: usize }, TArr { len: NonZeroUsize },
}

/// Layout is a single uninitialized byte.
struct Uninit;

/// Layout is `L` or `R`.
#[repr(C)]
struct Any<L, R>(PhantomData<(L, R)>);

/// Layout is `L` and `R`.
#[repr(C)]
struct All<L, R>(PhantomData<(L, R)>);

/// Layout is a single initialized byte ranging from `START` to `END`.
struct Init<const START: u8, const END: u8>;

/// Layout is `T` offset by `OFF` bytes.
#[repr(C)]
struct Off<const OFF: usize, T>(PhantomData<T>);

/// Layout is `T` repeated `N` times.
#[repr(C)]
struct Arr<const N: usize, T>(PhantomData<T>);

trait BitsExpr {
  const TAG: ExprTag;
  type Lhs: BitsExpr;
  type Rhs: BitsExpr;
}

const fn size<T: BitsExpr>() -> usize {
  match T::TAG {
    TVoid => 0,
    TUninit | TInit { .. } => 1,
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

const fn intersects<A: BitsExpr, B: BitsExpr>(x: usize, y: usize) -> u8 {
  match (A::TAG, B::TAG) {
    (TInit { byte }, _) => match (byte, byte.intersects::<B>(x, y)) {
      (a, b) if a.len() == b.len() => ALL,
      (_, b) if b.len() > 0 => ANY,
      _ => 0,
    },
    (TAny, _) => any(intersects::<A::Lhs, B>(x, y), intersects::<A::Rhs, B>(x, y)),
    (_, TAny) => intersects::<A, B::Lhs>(x, y) | intersects::<A, B::Rhs>(x, y),
    (TAll, _) => and(intersects::<A::Lhs, B>(x, y), intersects::<A::Rhs, B>(x, y)),
    (_, TAll) => and(intersects::<A, B::Lhs>(x, y), intersects::<A, B::Rhs>(x, y)),
    (TOff { off }, _) => intersects::<A::Lhs, B>(x + off, y),
    (_, TOff { off }) => intersects::<A, B::Lhs>(x, y + off),
    (TArr { len }, _) => fold!(..len.get(),
      intersects::<A::Lhs, B>(x, y),
      |j, acc| and(acc, intersects::<A::Lhs, B>(x + j * size::<A::Lhs>(), y))
    ),
    (_, TArr { len }) => fold!(..len.get(),
      intersects::<A, B::Lhs>(x, y),
      |j, acc| and(acc, intersects::<A, B::Lhs>(x, y + j * size::<B::Lhs>()))
    ),
    _ if x != y => ALL,
    (TVoid, TVoid) => ALL,
    (TUninit, TUninit) => ALL,
    (TUninit, TInit { .. }) => todo!(),
    (TVoid, TUninit | TInit { .. }) | (TUninit, TVoid) => 0,
  }
}

#[inline(always)]
const unsafe fn matches<T: BitsExpr>(ptr: *const u8) -> bool {
  match T::TAG {
    TVoid | TUninit => true,
    TAny => matches::<T::Lhs>(ptr) || matches::<T::Rhs>(ptr),
    TAll => matches::<T::Lhs>(ptr) && matches::<T::Rhs>(ptr),
    TInit { byte: Interval { start: 0, end: 0xff } } => true,
    TInit { byte } => byte.contains(*ptr),
    TOff { off } => matches::<T::Lhs>(ptr.add(off)),
    TArr { len } => fold!(..len.get(),
      true,
      |j, acc| acc && matches::<T::Lhs>(ptr.add(j * size::<T::Lhs>()))
    )
  }
}

const fn and(a: u8, b: u8) -> u8 {
  match (a, b) {
    (ALL, ALL) => ALL,
    (0, _) | (_, 0) => 0,
    _ => ANY,
  }
}

const fn any(a: u8, b: u8) -> u8 {
  match (a, b) {
    (ALL, ALL) => ALL,
    (0, 0) => 0,
    _ => ANY,
  }
}

/// Internal implements.
const _: () =  {
  // -----------------------------------------------------------------------------------------------
  // impl `Expr`

  impl BitsExpr for () {
    const TAG: ExprTag = TVoid;
    type Lhs = ();
    type Rhs = ();
  }

  impl BitsExpr for Uninit {
    const TAG: ExprTag = TUninit;
    type Lhs = ();
    type Rhs = ();
  }

  impl<L: BitsExpr, T: BitsExpr> BitsExpr for Any<L, T> {
    const TAG: ExprTag = TAny;
    type Lhs = L;
    type Rhs = T;
  }

  impl<L: BitsExpr, T: BitsExpr> BitsExpr for All<L, T> {
    const TAG: ExprTag = TAll;
    type Lhs = L;
    type Rhs = T;
  }

  impl<const START: u8, const END: u8> BitsExpr for Init<START, END> {
    const TAG: ExprTag = TInit { byte: Interval { start: START, end: END } };
    type Lhs = ();
    type Rhs = ();
  }

  impl<const OFF: usize, T: BitsExpr> BitsExpr for Off<OFF, T> {
    const TAG: ExprTag = TOff { off: OFF };
    type Lhs = T;
    type Rhs = ();
  }

  impl<const N: usize, T: BitsExpr> BitsExpr for Arr<N, T> {
    const TAG: ExprTag = TArr { len: unwrap!(NonZeroUsize::new(N)) };
    type Lhs = T;
    type Rhs = ();
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Init`

  const VOID_MARKER: u8 = 0xff;
  const UNINIT_MARKER: u8 = 0xfe;

  impl Interval {
    const VOID: Self = Self { start: VOID_MARKER, end: 0 };
    const UNINIT: Self = Self { start: UNINIT_MARKER, end: 0 };

    const fn len(self) -> usize {
      match (self.start, self.end) {
        (VOID_MARKER | UNINIT_MARKER, 0) => 0,
        (start, end) => (end - start) as usize + 1,
      }
    }

    const fn union(self, o: Self) -> Self {
      match ((self.start, self.end), (o.start, o.end)) {
        ((UNINIT_MARKER, 0), (UNINIT_MARKER, 0)) => self,
        ((VOID_MARKER, 0), (start, end)) | ((start, end), (VOID_MARKER, 0)) => Self { start, end },
        ((a, b), (x, y)) => {
          let start = if a < x { a } else { x };
          let end = if b > y { b } else { y };
          Self { start, end }
        }
      }
    }

    const fn intersection(&self, o: Self) -> Self {
      match (self.start, self.end) {
        (VOID_MARKER, 0) => Self::VOID,
        (UNINIT_MARKER, 0) => Self::UNINIT,
        _ => {
          assert!(self.end >= self.start && o.end >= o.start);
          let start = if self.start > o.start { self.start } else { o.start };
          let end = if self.end < o.end { self.end } else { o.end };
          if start <= end {
            Self { start, end }
          } else {
            Self::VOID
          }
        }
      }
    }

    const fn intersects<T: BitsExpr>(&self, x: usize, y: usize) -> Self {
      match T::TAG {
        TOff { off } => self.intersects::<T::Lhs>(x, y + off),
        TArr { len } => fold!(..len.get(),
          self.intersects::<T::Lhs>(x, y),
          |j, acc| acc.intersection(self.intersects::<T::Lhs>(x, y + (j * size::<T::Lhs>())))
        ),
        TAny => self.intersects::<T::Lhs>(x, y).union(self.intersects::<T::Rhs>(x, y)),
        TAll => self.intersects::<T::Lhs>(x, y).intersection(self.intersects::<T::Rhs>(x, y)),
        _ if x != y => return *self,
        TVoid => Interval::VOID,
        TUninit => Interval::UNINIT,
        TInit { byte } => self.intersection(byte),
      }
    }

    #[inline(always)]
    const fn contains(self, value: u8) -> bool {
      debug_assert!(self.start <= self.end);
      value >= self.start && value <= self.end
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
  println!("{:#04b}", intersects::<_0or4, _0or4>(0, 0));
  println!("{:#04b}", intersects::<_0or4or8, _0or4or8>(0, 0));
  println!("{:#04b}", intersects::<_0or4, _0or4or8>(0, 0));
  println!("{:#04b}", intersects::<_0or4or8, _0or4>(0, 0));

  type BoolPattern = Init<0, 1>;
  type U8Pattern = Init<0, 0xff>;

  println!("{:#04b}", intersects::<BoolPattern, U8Pattern>(0, 0));
  println!("{:#04b}", intersects::<U8Pattern, BoolPattern>(0, 0));
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
