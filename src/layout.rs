use crate::num::{cast, Native};

// -------------------------------------------------------------------------------------------------
// `inline!` macro

macro_rules! inline {
  { $( ($( $args:tt )*) ),* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

// -------------------------------------------------------------------------------------------------
// utilities

const fn mask(n: usize) -> u128 {
  (1u128 << (n * 4)) - 1
}

const fn upcast<T: Native>(val: T) -> u128 {
  cast(val)
}

// -------------------------------------------------------------------------------------------------
// `Const`

pub trait Const<T> {
  const VAL: T;
}

const _: () = {
  use crate::num::{cast, Numeric, PhantomUint};

  impl<T: Numeric, const VAL: u128> Const<T> for PhantomUint<VAL> {
    const VAL: T = cast(VAL);
  }
};

// -------------------------------------------------------------------------------------------------
// `Idx`

pub trait Idx {
  type Inc: Idx;
  type Dec: Idx;
  type IfZero<Then: Idx, Else: Idx>: Idx;
  const IDX: usize;
}

pub struct B0;
pub struct B1;

const _: () = {
  impl Idx for () {
    type Inc = ((), B1);
    type Dec = ();
    type IfZero<Then: Idx, Else: Idx> = Then;
    const IDX: usize = 0;
  }

  impl<U: Idx> Idx for (U, B0) {
    type Inc = (U, B1);
    type Dec = (U::Dec, B1);
    type IfZero<Then: Idx, Else: Idx> = Else;
    const IDX: usize = 0 | (U::IDX << 1);
  }

  impl<U: Idx> Idx for (U, B1) {
    type Inc = (U::Inc, B0);
    type Dec = U::IfZero<(), (U, B0)>;
    type IfZero<Then: Idx, Else: Idx> = Else;
    const IDX: usize = 1 | (U::IDX << 1);
  }
};

// -------------------------------------------------------------------------------------------------
// `Endian`

pub struct Le;
pub struct Be;

#[cfg(target_endian = "little")]
pub type Ne = Le;
#[cfg(target_endian = "big")]
pub type Ne = Be;

// -------------------------------------------------------------------------------------------------
// `Hex`

pub trait Hex {
  const HEX: u8;
}

inline! {
  (X0 0x0), (X1 0x1), (X2 0x2), (X3 0x3), (X4 0x4), (X5 0x5), (X6 0x6), (X7 0x7),
  (X8 0x8), (X9 0x9), (XA 0xA), (XB 0xB), (XC 0xC), (XD 0xD), (XE 0xE), (XF 0xF);
  ($T:ident $HEX:literal)
  pub struct $T;
  impl Hex for $T { const HEX: u8 = $HEX; }
}

// -------------------------------------------------------------------------------------------------
// `Input`

pub trait Input {
  const LEN: usize;
  const VAL: u128;
  type Push<H: Hex>: Input;
}

const _: () = {
  impl Input for () {
    const LEN: usize = 0;
    const VAL: u128 = 0;
    type Push<H: Hex> = H;
  }

  impl<T: Hex> Input for T {
    const LEN: usize = 1;
    const VAL: u128 = T::HEX as u128;
    type Push<H: Hex> = (T, H);
  }

  impl<I: Input, T: Hex> Input for (I, T) {
    const LEN: usize = I::LEN + 1;
    const VAL: u128 = ((T::HEX as u128) << (I::LEN * 4)) | I::VAL;
    type Push<H: Hex> = (Self, H);
  }
};

// -------------------------------------------------------------------------------------------------
// `Bits`

/// Type-system encoded bits pattern, or internally, a Deterministic Finite Automaton (DFA) for
/// nibble sequences.
pub trait Bits {
  /// Apply a nibble to the current state and return the next state.
  /// NOTE: Applying a nibble that is not accepted by `Self::START` is undefined behavior.
  /// NOTE: Applying a nibble to the end state is undefined behavior.
  /// NOTE: `Self::Repr` remain unchanged after applying a nibble.
  type Transition<H: Hex>: Bits;
  /// The number of nibbles accepted by the current state.
  const LEN: usize;
  const BRANCH_LESS: bool = false;
  /// `None` indicate the end of the sequence.
  /// `Some(x)` indicates the starting state of the sequence, where `x` is a bit mask of the
  /// accepted nibbles.
  const START: Option<u16>;
}

// -------------------------------------------------------------------------------------------------
// `Any`

pub struct Void;

impl Bits for Void {
  type Transition<H: Hex> = Void;
  const LEN: usize = 0;
  const START: Option<u16> = None;
}

pub struct Any<T, I = ()>(T, I);

impl<T: Native, I: Idx> Bits for Any<T, I> {
  type Transition<H: Hex> = Any<T, I::Inc>;
  const LEN: usize = (2 * size_of::<T>()) - I::IDX;
  const BRANCH_LESS: bool = true;
  const START: Option<u16> = match I::IDX {
    i if i >= (2 * size_of::<T>()) => None,
    _ => Some(0xffff),
  };
}

// -------------------------------------------------------------------------------------------------
// `Equ`

pub struct Equ<T, Val, I = ()>(T, Val, I);

impl<T: Native, Val: Const<T>, I: Idx> Bits for Equ<T, Val, I> {
  type Transition<H: Hex> = Equ<T, Val, I::Inc>;
  const LEN: usize = (2 * size_of::<T>()) - I::IDX;
  const BRANCH_LESS: bool = true;
  const START: Option<u16> = match (I::IDX, 2 * size_of::<T>()) {
    (i, n) if i >= n => None,
    _ => Some(1u16 << ((upcast(Val::VAL) >> (4 * I::IDX)) & 0xf)), // `hex..=hex`
  };
}

// -------------------------------------------------------------------------------------------------
// `Gte`

/// `>=`: Greater than or equal to
pub struct Gte<T, Val, I = ()>(T, Val, I);

impl<T: Native, Val: Const<T>, I: Input> Bits for Gte<T, Val, I> {
  type Transition<H: Hex> = Gte<T, Val, I::Push<H>>;
  const LEN: usize = (2 * size_of::<T>()) - I::LEN;
  const START: Option<u16> = match (I::LEN, 2 * size_of::<T>()) {
    (i, n) if i >= n => None,
    (i, n) => Some({
      let hex = (upcast(Val::VAL) >> (i * 4)) & 0xf;
      if i + 1 < n && (upcast(Val::VAL) >> ((i + 1) * 4)) != mask(n - i - 1) {
        0xffff // `..`
      } else if I::VAL >= (upcast(Val::VAL) & mask(i)) || hex >= 0xf {
        !((1u16 << hex) - 1) // `hex..=0xF`
      } else {
        !((1u16 << (hex + 1)) - 1) // `(hex + 1)..=0xF`
      }
    }),
  };
}

// -------------------------------------------------------------------------------------------------
// `Lte`

pub struct Lte<T, Val, I = ()>(T, Val, I);

impl<T: Native, Val: Const<T>, I: Input> Bits for Lte<T, Val, I> {
  type Transition<H: Hex> = Lte<T, Val, I::Push<H>>;
  const LEN: usize = (2 * size_of::<T>()) - I::LEN;
  const START: Option<u16> = match (I::LEN, 2 * size_of::<T>()) {
    (i, n) if i >= n => None,
    (i, _) => Some({
      let hex = (upcast(Val::VAL) >> (i * 4)) & 0xf;
      if (upcast(Val::VAL) >> ((i + 1) * 4)) != 0 {
        0xffff // `..`
      } else if I::VAL <= (upcast(Val::VAL) & mask(i)) || hex == 0 {
        ((1u32 << (hex + 1)) - 1) as u16 // `0x0..=hex`
      } else {
        ((1u32 << hex) - 1) as u16 // `0x0..=(hex - 1)`
      }
    }),
  };
}

// -------------------------------------------------------------------------------------------------
// `Union`

pub struct Union<A: Bits, B: Bits>(A, B);

impl<A: Bits, B: Bits> Bits for Union<A, B> {
  type Transition<N: Hex> = Union<A::Transition<N>, B::Transition<N>>;
  const LEN: usize = A::LEN;
  const START: Option<u16> = match (A::START, B::START) {
    (Some(a), Some(b)) => Some(a | b),
    (Some(x), _) | (_, Some(x)) => Some(x),
    _ => None,
  };
}

// -------------------------------------------------------------------------------------------------
// `Intersection`

pub struct Intersection<A: Bits, B: Bits>(A, B);

impl<A: Bits, B: Bits> Bits for Intersection<A, B> {
  type Transition<N: Hex> = Intersection<A::Transition<N>, B::Transition<N>>;
  const LEN: usize = A::LEN;
  const BRANCH_LESS: bool = A::BRANCH_LESS && B::BRANCH_LESS;
  const START: Option<u16> = match (A::START, B::START) {
    (Some(a), Some(b)) => Some(a & b),
    _ => None,
  };
}

// -------------------------------------------------------------------------------------------------
// `Layout`

#[rustfmt::skip]
pub enum LayoutKind {
  Void, Bits, Item
}

pub trait Layout {
  const KIND: LayoutKind;
  type Head: Bits;
  type Item: Layout;
  type Tail: Layout;
  type Push<X: Layout>: Layout;
}

struct Repeat<T, const N: usize, I = ()>(T, I);

const _: () = {
  impl Layout for () {
    const KIND: LayoutKind = LayoutKind::Void;
    type Head = Any<()>;
    type Item = ();
    type Tail = ();
    type Push<X: Layout> = X;
  }

  impl<T: Bits> Layout for T {
    const KIND: LayoutKind = LayoutKind::Bits;
    type Head = T;
    type Item = ();
    type Tail = ();
    type Push<X: Layout> = (T, X);
  }

  impl<T: Layout, const N: usize, I: Idx> Layout for Repeat<T, N, I> {
    const KIND: LayoutKind = if I::IDX >= N { LayoutKind::Bits } else { LayoutKind::Item };
    type Head = Any<()>;
    type Item = (T, Repeat<T, N, I::Inc>);
    type Tail = ();
    type Push<X: Layout> = (Self, X);
  }

  macro_rules! impl_seq {
    { $T:ident $( $R:ident )* } => {
      impl_seq! { @impl $( $R )* }
      impl<$T: Layout, $( $R: Layout ),*> Layout for ($T, $( $R, )*) {
        const KIND: LayoutKind = $T::KIND;
        type Head = $T::Head;
        type Item = $T::Item;
        type Tail = <$T::Tail as Layout>::Push<($( $R, )*)>;
        type Push<X: Layout> = (Self, X);
      }
    };
    { @impl } => { };
    { @impl $T:ident $( $R:ident )* } => {
      impl_seq! { @impl $( $R )* }
      impl<$T: Layout, $( $R: Layout ),*> Layout for ($T, $( $R, )*) {
        const KIND: LayoutKind = $T::KIND;
        type Head = $T::Head;
        type Item = $T::Item;
        type Tail = <$T::Tail as Layout>::Push<($( $R, )*)>;
        type Push<X: Layout> = ($T, $( $R, )* X);
      }
    };
  }

  impl_seq! {
     _0   _1   _2   _3   _4   _5   _6   _7   _8   _9  _10  _11  _12  _13  _14  _15
    // _16  _17  _18  _19  _20  _21  _22  _23  _24  _25  _26  _27  _28  _29  _30  _31
    // _32  _33  _34  _35  _36  _37  _38  _39  _40  _41  _42  _43  _44  _45  _46  _47
    // _48  _49  _50  _51  _52  _53  _54  _55  _56  _57  _58  _59  _60  _61  _62  _63
   //  _64  _65  _66  _67  _68  _69  _70  _71  _72  _73  _74  _75  _76  _77  _78  _79
   //  _80  _81  _82  _83  _84  _85  _86  _87  _88  _89  _90  _91  _92  _93  _94  _95
   //  _96  _97  _98  _99 _100 _101 _102 _103 _104 _105 _106 _107 _108 _109 _110 _111
   // _112 _113 _114 _115 _116 _117 _118 _119 _120 _121 _122 _123 _124 _125 _126 _127
   // _128 _129 _130 _131 _132 _133 _134 _135 _136 _137 _138 _139 _140 _141 _142 _143
   // _144 _145 _146 _147 _148 _149 _150 _151 _152 _153 _154 _155 _156 _157 _158 _159
   // _160 _161 _162 _163 _164 _165 _166 _167 _168 _169 _170 _171 _172 _173 _174 _175
   // _176 _177 _178 _179 _180 _181 _182 _183 _184 _185 _186 _187 _188 _189 _190 _191
   // _192 _193 _194 _195 _196 _197 _198 _199 _200 _201 _202 _203 _204 _205 _206 _207
   // _208 _209 _210 _211 _212 _213 _214 _215 _216 _217 _218 _219 _220 _221 _222 _223
   // _224 _225 _226 _227 _228 _229 _230 _231 _232 _233 _234 _235 _236 _237 _238 _239
   // _240 _241 _242 _243 _244 _245 _246 _247 _248 _249 _250 _251 _252 _253 _254 _255
  }
};

// -------------------------------------------------------------------------------------------------
// `accept`

pub const fn accept<T: Bits, const VAL: u128>() -> bool {
  struct Accept<const VAL: u128, T, I>(T, I);

  impl<const VAL: u128, T: Bits, I: Idx> Accept<VAL, T, I> {
    const RESULT: bool = match T::START {
      None => true,
      Some(x) if x & (1 << ((VAL >> (I::IDX * 4)) & 0xf)) == 0 => false,
      _ => match (VAL >> (I::IDX * 4)) & 0xf {
        0x0 => accept_next::<T, VAL, X0, I>(),
        0x1 => accept_next::<T, VAL, X1, I>(),
        0x2 => accept_next::<T, VAL, X2, I>(),
        0x3 => accept_next::<T, VAL, X3, I>(),
        0x4 => accept_next::<T, VAL, X4, I>(),
        0x5 => accept_next::<T, VAL, X5, I>(),
        0x6 => accept_next::<T, VAL, X6, I>(),
        0x7 => accept_next::<T, VAL, X7, I>(),
        0x8 => accept_next::<T, VAL, X8, I>(),
        0x9 => accept_next::<T, VAL, X9, I>(),
        0xA => accept_next::<T, VAL, XA, I>(),
        0xB => accept_next::<T, VAL, XB, I>(),
        0xC => accept_next::<T, VAL, XC, I>(),
        0xD => accept_next::<T, VAL, XD, I>(),
        0xE => accept_next::<T, VAL, XE, I>(),
        0xF => accept_next::<T, VAL, XF, I>(),
        _ => false,
      },
    };
  }

  pub const fn accept_next<T: Bits, const VAL: u128, N: Hex, I: Idx>() -> bool {
    Accept::<VAL, T::Transition<N>, I::Inc>::RESULT
  }

  Accept::<VAL, T, ()>::RESULT
}

// -------------------------------------------------------------------------------------------------
// `intersect_bits`

#[inline(always)]
const fn or(a: u8, b: u8) -> u8 {
  (a & b) | (a != 0) as u8 | (b != 0) as u8
}

pub const fn intersect_bits<A: Bits, B: Bits>() -> u8 {
  const fn transitions<A: Bits, B: Bits>(mut res: u8, i: u16) -> u8 {
    const fn transition<A: Bits, B: Bits, N: Hex>() -> u8 {
      intersect_bits::<A::Transition<N>, B::Transition<N>>()
    }

    inline! {
      (X0 0x0), (X1 0x1), (X2 0x2), (X3 0x3), (X4 0x4), (X5 0x5), (X6 0x6), (X7 0x7),
      (X8 0x8), (X9 0x9), (XA 0xA), (XB 0xB), (XC 0xC), (XD 0xD), (XE 0xE), (XF 0xF);
      ($T:tt $HEX:tt) if i & (1 << $HEX) == 1 {
        if A::BRANCH_LESS && B::BRANCH_LESS { return transition::<A, B, $T>(); }
        res = or(res, transition::<A, B, $T>());
      }
    }
    res
  }

  const {
    match (A::START, B::START) {
      (None, _) => 0b11,
      (_, None) => 0,
      (Some(a), Some(b)) if a & b == 0 => 0,
      (Some(a), Some(b)) if a & b == a => transitions::<A, B>(0b11, a & b),
      (Some(a), Some(b)) => transitions::<A, B>(0b01, a & b),
    }
  }
}

// -------------------------------------------------------------------------------------------------
// `intersect`

const fn intersect_with<A: Layout, B: Layout, Ab: Bits, Bb: Bits>() -> u8 {
  const fn transitions<A: Layout, B: Layout, Ab: Bits, Bb: Bits>(mut res: u8, i: u16) -> u8 {
    const fn transition<A: Layout, B: Layout, Ab: Bits, Bb: Bits, H: Hex>() -> u8 {
      intersect_with::<A, B, Ab::Transition<H>, Bb::Transition<H>>()
    }

    inline! {
      (X0 0x0), (X1 0x1), (X2 0x2), (X3 0x3), (X4 0x4), (X5 0x5), (X6 0x6), (X7 0x7),
      (X8 0x8), (X9 0x9), (XA 0xA), (XB 0xB), (XC 0xC), (XD 0xD), (XE 0xE), (XF 0xF);
      ($T:tt $HEX:tt) if i & (1 << $HEX) == 1 {
        if Ab::BRANCH_LESS && Bb::BRANCH_LESS { return res & transition::<A, B, Ab, Bb, $T>(); }
        res = or(res, transition::<A, B, Ab, Bb, $T>());
      }
    }
    res
  }

  const {
    use LayoutKind::*;
    match (A::KIND, Ab::START, B::KIND, Bb::START) {
      (_, Some(_), _, Some(_)) if Ab::LEN == Bb::LEN => {
        intersect_bits::<Ab, Bb>() & intersect::<A::Tail, B::Tail>()
      }

      (_, Some(a), _, Some(b)) if a & b == 0 => 0,
      (_, Some(a), _, Some(b)) if a & b == a => transitions::<A, B, Ab, Bb>(0b11, a & b),
      (_, Some(a), _, Some(b)) => transitions::<A, B, Ab, Bb>(0b01, a & b),

      (Void, None, _, _) => 0b11,
      (_, _, Void, None) => 0b00,
      (Item, None, Item, None) => intersect::<A::Item, B::Item>(),
      (_, None, _, None) => intersect::<A::Tail, B::Tail>(),
      (Item, None, _, _) => intersect_with::<A::Item, B, <A::Item as Layout>::Head, Bb>(),
      (_, None, _, _) => intersect_with::<A::Tail, B, <A::Tail as Layout>::Head, Bb>(),
      (_, _, Item, None) => intersect_with::<A, B::Item, Ab, <B::Item as Layout>::Head>(),
      (_, _, _, None) => intersect_with::<A, B::Tail, Ab, <B::Tail as Layout>::Head>(),
    }
  }
}

pub const fn intersect<A: Layout, B: Layout>() -> u8 {
  intersect_with::<A, B, A::Head, B::Head>()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::num::PhantomUint;

  type Bool = Union<Equ<u8, PhantomUint<0>>, Equ<u8, PhantomUint<1>>>;
  type U8 = Any<u8>;
  type U16 = Any<u16>;
  type U32 = Any<u32>;
  type NonZeroU8 = Gte<u8, PhantomUint<1>>;
  type NonZeroU16 = Gte<u16, PhantomUint<1>>;
  type NonZeroU32 = Gte<u32, PhantomUint<1>>;

  #[test]
  fn test_layout() {
    // intersect::<(), (Repeat<(U8, Bool), 2>, U16)>();

    // assert_eq!(0b11, intersect::<Repeat<U8, 1024>, Repeat<U8, 1024>>());
    // assert_eq!(0b11, intersect::<Repeat<U8, 4096>, Repeat<U8, 4096>>());
    // assert_eq!(0b11, intersect::<Repeat<U8, 8192>, Repeat<U8, 8192>>());
    // assert_eq!(0b01, intersect::<Repeat<(U16, U8, U8), 8192>, Repeat<(Bool, Bool, U16), 8192>>());
    // assert_eq!(0b11, intersect::<Repeat<(U32, U16, U8), 34433>, Repeat<(U32, U16, U8), 34433>>());

    // assert_eq!(0b11, intersect::<U32, Repeat<U8, 4>>());
    // assert_eq!(0b01, intersect::<U32, (Repeat<U8, 3>, Bool)>());

    assert_eq!(0b11, intersect::<(U8, Bool), (U8, U8)>());
    // assert_eq!(0b11, intersect::<(Repeat<U8, 1>, Bool), (Repeat<U8, 1>, U8)>());
    assert_eq!(0b01, intersect::<(U8, U8), (U8, Bool)>());
    // assert_eq!(0b01, intersect::<(Repeat<U8, 1>, U8), (Repeat<U8, 1>, Bool)>());

    assert_eq!(0b11, intersect::<(), ()>());
    assert_eq!(
      0b01,
      intersect::<<U8 as Bits>::Transition<X0>, <NonZeroU8 as Bits>::Transition<X0>>()
    );
    assert_eq!(
      0b11,
      intersect::<<U8 as Bits>::Transition<X1>, <NonZeroU8 as Bits>::Transition<X1>>()
    );
    assert_eq!((Some(0xffff), Some(0xffff)), (U8::START, NonZeroU8::START));
    assert_eq!(
      (Some(0xffff), Some(0xffff & !1)),
      (<<U8 as Bits>::Transition<X0>>::START, <<NonZeroU8 as Bits>::Transition<X0>>::START)
    );
    assert_eq!(
      (Some(0xffff), Some(0xffff)),
      (<<U8 as Bits>::Transition<X1>>::START, <<NonZeroU8 as Bits>::Transition<X1>>::START)
    );
    assert_eq!(
      (None, None),
      (
        <<<U8 as Bits>::Transition<X1> as Bits>::Transition<X1>>::START,
        <<<NonZeroU8 as Bits>::Transition<X1> as Bits>::Transition<X1>>::START
      )
    );
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());

    assert_eq!(0b11, intersect::<U8, U8>());
    assert_eq!(0b11, intersect::<Bool, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    assert_eq!(0b11, intersect::<NonZeroU8, U8>());
    assert_eq!(0b01, intersect::<U16, NonZeroU16>());
    assert_eq!(0b11, intersect::<NonZeroU16, U16>());

    assert_eq!(0b11, intersect::<(U8, Bool), (U8, Bool)>());
    assert_eq!(0b11, intersect::<(U8, Bool, NonZeroU16, U16), (U8, Bool, NonZeroU16, U16)>());
    assert_eq!(0b01, intersect::<(U8, U8, U16, U32), (Bool, NonZeroU8, NonZeroU16, NonZeroU32)>());
    assert_eq!(0b11, intersect::<(Bool, NonZeroU8, NonZeroU16, NonZeroU32), (U8, U8, U16, U32)>());
    assert_eq!(0b11, intersect::<(Bool, NonZeroU8, NonZeroU16), U32>());
    assert_eq!(0b01, intersect::<U32, (Bool, NonZeroU8, NonZeroU16)>());

    type A0 = (Bool, NonZeroU8, NonZeroU16, NonZeroU32);
    type A1 = <A0 as Layout>::Push<(U8, U8, U16, U32)>;
    type A2 = <A1 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type A3 = <A2 as Layout>::Push<(U8, U8, U16, U32)>;
    type A4 = <A3 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type A5 = <A4 as Layout>::Push<(U8, U8, U16, U32)>;
    type A6 = <A5 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type A7 = <A6 as Layout>::Push<(U8, U8, U16, U32)>;
    type A8 = <A7 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;

    assert_eq!(0b11, intersect::<A8, A8>());

    type B0 = (Bool, NonZeroU8, NonZeroU16, NonZeroU32);
    type B1 = <B0 as Layout>::Push<(U8, U8, U16, NonZeroU32)>;
    type B2 = <B1 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type B3 = <B2 as Layout>::Push<(U8, U8, U16, NonZeroU32)>;
    type B4 = <B3 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type B5 = <B4 as Layout>::Push<(U8, U8, U16, NonZeroU32)>;
    type B6 = <B5 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;
    type B7 = <B6 as Layout>::Push<(U8, U8, U16, NonZeroU32)>;
    type B8 = <B7 as Layout>::Push<(Bool, NonZeroU8, NonZeroU16, NonZeroU32)>;

    assert_eq!(0b11, intersect::<B8, B8>());
    assert_eq!(0b11, intersect::<B8, A8>());

    assert_eq!(0b01, intersect::<U32, NonZeroU32>());
    assert_eq!(0b11, intersect::<NonZeroU32, U32>());

    assert_eq!(0b01, intersect::<(U32, U16), (NonZeroU32, U16)>());
    assert_eq!(0b11, intersect::<(NonZeroU32, U16), (U32, U16)>());

    assert_eq!(0b01, intersect::<(U16, U32), (U16, NonZeroU32)>());
    assert_eq!(0b11, intersect::<(U16, NonZeroU32), (U16, U32)>());
    assert_eq!(0b01, intersect::<(U16, (U8, U32)), (U16, (U8, NonZeroU32))>());
    assert_eq!(0b11, intersect::<(U16, (U8, NonZeroU32)), (U16, (U8, U32))>());
    assert_eq!(0b01, intersect::<(U16, (U8, U32)), (U16, (U8, NonZeroU32))>());
    assert_eq!(0b11, intersect::<(U16, (U8, NonZeroU32)), (U16, (U8, U32))>());

    assert_eq!(0b01, intersect::<A8, B8>());
  }

  #[test]
  fn test_bits() {
    type NonZeroU32 = Gte<u32, PhantomUint<1>>;

    assert!(accept::<NonZeroU32, 1>());
    assert!(!accept::<NonZeroU32, 0>());

    type Ox0123 = Equ<u16, PhantomUint<0x0123>>;
    type OxABCD = Equ<u16, PhantomUint<0xabcd>>;

    assert!(accept::<Ox0123, 0x0123>());
    assert!(!accept::<Ox0123, 0xabcd>());
    assert!(accept::<OxABCD, 0xabcd>());
    assert!(!accept::<OxABCD, 0x0123>());

    type Ox0123UOxABCD = Union<Ox0123, OxABCD>;

    assert!(!accept::<Ox0123UOxABCD, 0x0122>());
    assert!(accept::<Ox0123UOxABCD, 0x0123>());
    assert!(!accept::<Ox0123UOxABCD, 0x0124>());
    assert!(!accept::<Ox0123UOxABCD, 0xabcc>());
    assert!(accept::<Ox0123UOxABCD, 0xabcd>());
    assert!(!accept::<Ox0123UOxABCD, 0xabce>());

    type Gte0x1234 = Gte<u16, PhantomUint<0x1234>>;
    type Lte0x1234 = Lte<u16, PhantomUint<0x1234>>;

    assert!(!accept::<Gte0x1234, { 0x1234 - 0xf }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0xe }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0xd }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0xc }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0xb }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0xa }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x9 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x8 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x7 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x6 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x5 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x4 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x3 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x2 }>());
    assert!(!accept::<Gte0x1234, { 0x1234 - 0x1 }>());
    assert!(accept::<Gte0x1234, 0x1234>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x1 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x2 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x3 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x4 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x5 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x6 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x7 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x8 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0x9 }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xa }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xb }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xc }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xd }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xe }>());
    assert!(accept::<Gte0x1234, { 0x1234 + 0xf }>());

    assert!(accept::<Lte0x1234, { 0x1234 - 0xf }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0xe }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0xd }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0xc }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0xb }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0xa }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x9 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x8 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x7 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x6 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x5 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x4 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x3 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x2 }>());
    assert!(accept::<Lte0x1234, { 0x1234 - 0x1 }>());
    assert!(accept::<Lte0x1234, 0x1234>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x1 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x2 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x3 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x4 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x5 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x6 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x7 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x8 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0x9 }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xa }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xb }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xc }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xd }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xe }>());
    assert!(!accept::<Lte0x1234, { 0x1234 + 0xf }>());

    type Gte0xfafc = Gte<u16, PhantomUint<0xfafc>>;
    type Lte0xfafc = Lte<u16, PhantomUint<0xfafc>>;

    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xf }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xe }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xd }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xc }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xb }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0xa }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x9 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x8 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x7 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x6 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x5 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x4 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x3 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x2 }>());
    assert!(!accept::<Gte0xfafc, { 0xfafc - 0x1 }>());
    assert!(accept::<Gte0xfafc, 0xfafc>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x1 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x2 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x3 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x4 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x5 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x6 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x7 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x8 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0x9 }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xa }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xb }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xc }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xd }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xe }>());
    assert!(accept::<Gte0xfafc, { 0xfafc + 0xf }>());

    assert!(accept::<Lte0xfafc, { 0xfafc - 0xf }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0xe }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0xd }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0xc }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0xb }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0xa }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x9 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x8 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x7 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x6 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x5 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x4 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x3 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x2 }>());
    assert!(accept::<Lte0xfafc, { 0xfafc - 0x1 }>());
    assert!(accept::<Lte0xfafc, 0xfafc>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x1 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x2 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x3 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x4 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x5 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x6 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x7 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x8 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0x9 }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xa }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xb }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xc }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xd }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xe }>());
    assert!(!accept::<Lte0xfafc, { 0xfafc + 0xf }>());

    type Gte0x0123 = Gte<u16, PhantomUint<0x0123>>;

    assert!(!accept::<Gte0x0123, 0x0122>());
    assert!(accept::<Gte0x0123, 0x0123>());
    assert!(accept::<Gte0x0123, 0x0124>());
    assert!(accept::<Gte0x0123, 0x0125>());
    assert!(accept::<Gte0x0123, 0x0126>());
    assert!(accept::<Gte0x0123, 0xffff>());

    type Lte0xabcd = Lte<u16, PhantomUint<0xabcd>>;

    assert!(!accept::<Lte0xabcd, 0xabce>());
    assert!(accept::<Lte0xabcd, 0xabcd>());
    assert!(accept::<Lte0xabcd, 0xabcc>());
    assert!(accept::<Lte0xabcd, 0xabcb>());
    assert!(accept::<Lte0xabcd, 0xabca>());
    assert!(accept::<Lte0xabcd, 0x0>());

    type Gte0x0123ILte0xabcd = Intersection<Gte0x0123, Lte0xabcd>;

    assert!(!accept::<Gte0x0123ILte0xabcd, 0x0122>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0x0123>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0x0124>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0x0125>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0x0126>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0xabcd>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0xabcc>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0xabcb>());
    assert!(accept::<Gte0x0123ILte0xabcd, 0xabca>());
    assert!(!accept::<Gte0x0123ILte0xabcd, 0xabce>());

    println!("{:b}", intersect_bits::<Gte0x0123, Gte0x0123>());
    println!("{:b}", intersect_bits::<Gte0x0123ILte0xabcd, Gte0x0123ILte0xabcd>());
    println!("{:b}", intersect_bits::<OxABCD, OxABCD>());
    println!("{:b}", intersect_bits::<OxABCD, Any<u16>>());
    println!("{:b}", intersect_bits::<Any<u16>, OxABCD>());
    println!("{:b}", intersect_bits::<Ox0123, OxABCD>());
    println!("{:b}", intersect_bits::<OxABCD, Gte0x0123ILte0xabcd>());
    println!("{:b}", intersect_bits::<Gte0x0123ILte0xabcd, OxABCD>());

    type _1u2 = Union<Equ<u16, PhantomUint<1>>, Equ<u16, PhantomUint<2>>>;
    type _0u1u2 = Union<Equ<u16, PhantomUint<0>>, _1u2>;

    println!("{:b}", intersect_bits::<_0u1u2, _1u2>());
    println!("{:b}", intersect_bits::<_1u2, _0u1u2>());
  }
}
