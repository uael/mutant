macro_rules! inline {
  { $( ($( $args:tt )*) )* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

pub trait Count {
  type Inc: Count;
  const USIZE: usize;
}

pub struct B0;
pub struct B1;

const _: () = {
  impl Count for B0 {
    type Inc = B1;
    const USIZE: usize = 0;
  }
  
  impl Count for B1 {
    type Inc = (B1, B0);
    const USIZE: usize = 1;
  }

  impl<U: Count> Count for (U, B0) {
    type Inc = (U, B1);
    const USIZE: usize = 0 | (U::USIZE << 1);
  }

  impl<U: Count> Count for (U, B1) {
    type Inc = (U::Inc, B0);
    const USIZE: usize = 1 | (U::USIZE << 1);
  }
};

pub trait Dfa {
  type J0: Dfa; type J1: Dfa; type J2: Dfa; type J3: Dfa;
  type J4: Dfa; type J5: Dfa; type J6: Dfa; type J7: Dfa;
  type J8: Dfa; type J9: Dfa; type Ja: Dfa; type Jb: Dfa;
  type Jc: Dfa; type Jd: Dfa; type Je: Dfa; type Jf: Dfa;

  type Head: Dfa;
  type Tail: Dfa;

  const START: Option<u16>;
  const BRANCH: bool;
  const WILDCARD: bool = false;
  const SEQUENCE: bool = false;
  const ANY: bool;
  const LEN: usize;
}

pub struct Any8;
pub struct Any4;
pub struct NonZero8;
pub struct NonZero4;
pub struct Zero;
pub struct Bool;
pub struct Equ4<const VAL: u8>;
pub struct Equ8<const VAL: u8, I: Count = B0>(I);

impl Dfa for () {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = ();
  type Tail = ();

  const START: Option<u16> = None; // leaf state, no continuation.
  const BRANCH: bool = false; // no continuation, hence no branching.
  const ANY: bool = true; // leaf state, hence false positive.
  const LEN: usize = 0; // leaf state, hence no length.
}

impl Dfa for Any8 {
  type J0 = Any4; type J1 = Any4; type J2 = Any4; type J3 = Any4;
  type J4 = Any4; type J5 = Any4; type J6 = Any4; type J7 = Any4;
  type J8 = Any4; type J9 = Any4; type Ja = Any4; type Jb = Any4;
  type Jc = Any4; type Jd = Any4; type Je = Any4; type Jf = Any4;

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(0xffff); // any half-byte.
  const BRANCH: bool = false; // accept any half-byte, hence no branching.
  const ANY: bool = true; // accept any half-byte.
  const LEN: usize = 2; // 2 half-bytes.
}

impl Dfa for Any4 {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(0xffff); // any half-byte.
  const BRANCH: bool = false; // accept any half-byte, hence no branching.
  const ANY: bool = true; // accept any half-byte.
  const LEN: usize = 1; // 1 half-byte.
}

impl<const VAL: u8> Dfa for Equ4<VAL> {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(1 << (VAL & 0xf));
  const BRANCH: bool = false;
  const ANY: bool = false;
  const LEN: usize = 1;
}

impl<const VAL: u8, I: Count> Dfa for Equ8<VAL, I> {
  type J0 = Equ8<VAL, I::Inc>; type J1 = Equ8<VAL, I::Inc>; type J2 = Equ8<VAL, I::Inc>; type J3 = Equ8<VAL, I::Inc>;
  type J4 = Equ8<VAL, I::Inc>; type J5 = Equ8<VAL, I::Inc>; type J6 = Equ8<VAL, I::Inc>; type J7 = Equ8<VAL, I::Inc>;
  type J8 = Equ8<VAL, I::Inc>; type J9 = Equ8<VAL, I::Inc>; type Ja = Equ8<VAL, I::Inc>; type Jb = Equ8<VAL, I::Inc>;
  type Jc = Equ8<VAL, I::Inc>; type Jd = Equ8<VAL, I::Inc>; type Je = Equ8<VAL, I::Inc>; type Jf = Equ8<VAL, I::Inc>;

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = if I::USIZE >= 2 { None } else { Some(1u16 << ((VAL >> (4 * I::USIZE)) & 0xf)) };
  const BRANCH: bool = false;
  const ANY: bool = false;
  const LEN: usize = 2 - I::USIZE;
}

impl Dfa for NonZero8 {
  type J0 = NonZero4; type J1 = Any4; type J2 = Any4; type J3 = Any4;
  type J4 = Any4; type J5 = Any4; type J6 = Any4; type J7 = Any4;
  type J8 = Any4; type J9 = Any4; type Ja = Any4; type Jb = Any4;
  type Jc = Any4; type Jd = Any4; type Je = Any4; type Jf = Any4;

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(0xffff); // any half-byte.
  const BRANCH: bool = true; // jump to `0` is different from others, hence branching.
  const ANY: bool = false; // jump to `0` lead to non-any state, hence not any.
  const LEN: usize = 2; // 2 half-bytes.
}

impl Dfa for NonZero4 {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(!1); // any half-byte except `0`.
  const BRANCH: bool = false; // accept any half-byte except `0`, hence no branching.
  const ANY: bool = false; // accept any half-byte except `0`.
  const LEN: usize = 1; // 1 half-byte.
}

impl Dfa for Zero {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(1); // only `0`.
  const BRANCH: bool = false; // only `0`, hence no branching.
  const ANY: bool = false; // only `0`.
  const LEN: usize = 1; // 1 half-byte.
}

impl Dfa for Bool {
  type J0 = Zero; type J1 = Zero; type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = Self;
  type Tail = ();

  const START: Option<u16> = Some(0b11);
  const BRANCH: bool = false;
  const ANY: bool = false;
  const LEN: usize = 2;
}

pub struct ArrayIter<T, I, const LEN: usize>(T, I);

impl<T: Dfa, I: Count, const LEN: usize> Dfa for ArrayIter<T, I, LEN> {
  type J0 = (T::J0, ArrayIter<T, I::Inc, LEN>);
  type J1 = (T::J1, ArrayIter<T, I::Inc, LEN>);
  type J2 = (T::J2, ArrayIter<T, I::Inc, LEN>);
  type J3 = (T::J3, ArrayIter<T, I::Inc, LEN>);
  type J4 = (T::J4, ArrayIter<T, I::Inc, LEN>);
  type J5 = (T::J5, ArrayIter<T, I::Inc, LEN>);
  type J6 = (T::J6, ArrayIter<T, I::Inc, LEN>);
  type J7 = (T::J7, ArrayIter<T, I::Inc, LEN>);
  type J8 = (T::J8, ArrayIter<T, I::Inc, LEN>);
  type J9 = (T::J9, ArrayIter<T, I::Inc, LEN>);
  type Ja = (T::Ja, ArrayIter<T, I::Inc, LEN>);
  type Jb = (T::Jb, ArrayIter<T, I::Inc, LEN>);
  type Jc = (T::Jc, ArrayIter<T, I::Inc, LEN>);
  type Jd = (T::Jd, ArrayIter<T, I::Inc, LEN>);
  type Je = (T::Je, ArrayIter<T, I::Inc, LEN>);
  type Jf = (T::Jf, ArrayIter<T, I::Inc, LEN>);

  type Head = T;
  type Tail = ArrayIter<T, I::Inc, LEN>;

  const START: Option<u16> = if I::USIZE == LEN { None } else { T::START };
  const BRANCH: bool = (LEN - I::USIZE) != 0 && T::BRANCH;
  const SEQUENCE: bool = true;
  const ANY: bool = (LEN - I::USIZE) == 0 || T::ANY;
  const LEN: usize = (LEN - I::USIZE) * T::LEN;
}

impl<T: Dfa, const LEN: usize> Dfa for [T; LEN] {
  type J0 = (T::J0, ArrayIter<T, B1, LEN>);
  type J1 = (T::J1, ArrayIter<T, B1, LEN>);
  type J2 = (T::J2, ArrayIter<T, B1, LEN>);
  type J3 = (T::J3, ArrayIter<T, B1, LEN>);
  type J4 = (T::J4, ArrayIter<T, B1, LEN>);
  type J5 = (T::J5, ArrayIter<T, B1, LEN>);
  type J6 = (T::J6, ArrayIter<T, B1, LEN>);
  type J7 = (T::J7, ArrayIter<T, B1, LEN>);
  type J8 = (T::J8, ArrayIter<T, B1, LEN>);
  type J9 = (T::J9, ArrayIter<T, B1, LEN>);
  type Ja = (T::Ja, ArrayIter<T, B1, LEN>);
  type Jb = (T::Jb, ArrayIter<T, B1, LEN>);
  type Jc = (T::Jc, ArrayIter<T, B1, LEN>);
  type Jd = (T::Jd, ArrayIter<T, B1, LEN>);
  type Je = (T::Je, ArrayIter<T, B1, LEN>);
  type Jf = (T::Jf, ArrayIter<T, B1, LEN>);

  type Head = T;
  type Tail = ArrayIter<T, B1, LEN>;

  const START: Option<u16> = if LEN == 0 { None } else { T::START };
  const BRANCH: bool = LEN != 0 && T::BRANCH;
  const SEQUENCE: bool = true;
  const ANY: bool = LEN == 0 || T::ANY;
  const LEN: usize = LEN * T::LEN;
}

pub struct Slice<T, const WILDCARD: bool = false>(T);

impl<T: Dfa, const WILDCARD: bool> Dfa for Slice<T, WILDCARD> {
  type J0 = (T::J0, Slice<T, true>);
  type J1 = (T::J1, Slice<T, true>);
  type J2 = (T::J2, Slice<T, true>);
  type J3 = (T::J3, Slice<T, true>);
  type J4 = (T::J4, Slice<T, true>);
  type J5 = (T::J5, Slice<T, true>);
  type J6 = (T::J6, Slice<T, true>);
  type J7 = (T::J7, Slice<T, true>);
  type J8 = (T::J8, Slice<T, true>);
  type J9 = (T::J9, Slice<T, true>);
  type Ja = (T::Ja, Slice<T, true>);
  type Jb = (T::Jb, Slice<T, true>);
  type Jc = (T::Jc, Slice<T, true>);
  type Jd = (T::Jd, Slice<T, true>);
  type Je = (T::Je, Slice<T, true>);
  type Jf = (T::Jf, Slice<T, true>);

  type Head = T;
  type Tail = Slice<T, true>;

  const START: Option<u16> = T::START;
  const BRANCH: bool = T::BRANCH;
  const WILDCARD: bool = WILDCARD;
  const SEQUENCE: bool = true;
  const ANY: bool = T::ANY;
  const LEN: usize = 0;
}

macro_rules! impl_seq {
  { $T:ident } => { };
  { $T:ident $( $R:ident )+ } => {
    impl_seq! { $( $R )+ }
    #[allow(unused_parens)]
    impl<$T: Dfa, $( $R: Dfa ),*> Dfa for ($T, $( $R, )+) {
      type J0 = ($T::J0, $( $R, )+);
      type J1 = ($T::J1, $( $R, )+);
      type J2 = ($T::J2, $( $R, )+);
      type J3 = ($T::J3, $( $R, )+);
      type J4 = ($T::J4, $( $R, )+);
      type J5 = ($T::J5, $( $R, )+);
      type J6 = ($T::J6, $( $R, )+);
      type J7 = ($T::J7, $( $R, )+);
      type J8 = ($T::J8, $( $R, )+);
      type J9 = ($T::J9, $( $R, )+);
      type Ja = ($T::Ja, $( $R, )+);
      type Jb = ($T::Jb, $( $R, )+);
      type Jc = ($T::Jc, $( $R, )+);
      type Jd = ($T::Jd, $( $R, )+);
      type Je = ($T::Je, $( $R, )+);
      type Jf = ($T::Jf, $( $R, )+);

      type Head = $T;
      type Tail = ($( $R ),+);

      const START: Option<u16> = match $T::START { Some(a) => Some(a), _ => <($( $R ),+)>::START  };
      const BRANCH: bool = $T::BRANCH || <($( $R ),+)>::BRANCH;
      const SEQUENCE: bool = true;
      const ANY: bool = $T::ANY && <($( $R ),+)>::ANY;
      const LEN: usize = $T::LEN + <($( $R ),+)>::LEN;
    }
  };
}

impl_seq! {
   _0   _1   _2   _3   _4   _5   _6   _7   _8   _9  _10  _11  _12  _13  _14  _15
  _16  _17  _18  _19  _20  _21  _22  _23  _24  _25  _26  _27  _28  _29  _30  _31
  _32  _33  _34  _35  _36  _37  _38  _39  _40  _41  _42  _43  _44  _45  _46  _47
  _48  _49  _50  _51  _52  _53  _54  _55  _56  _57  _58  _59  _60  _61  _62  _63
  _64  _65  _66  _67  _68  _69  _70  _71  _72  _73  _74  _75  _76  _77  _78  _79
  _80  _81  _82  _83  _84  _85  _86  _87  _88  _89  _90  _91  _92  _93  _94  _95
  _96  _97  _98  _99 _100 _101 _102 _103 _104 _105 _106 _107 _108 _109 _110 _111
 _112 _113 _114 _115 _116 _117 _118 _119 _120 _121 _122 _123 _124 _125 _126 _127
}

#[inline(always)]
const fn or(a: u8, b: u8) -> u8 {
  (a & b) | (a != 0) as u8 | (b != 0) as u8
}

#[inline(always)]
pub const fn intersect<A: Dfa, B: Dfa>() -> u8 {
  const {
    // early optimization when target is any, or slices are catching up.
    if B::ANY || (A::WILDCARD && B::WILDCARD) { 0b11 }
    // non-overlapping states.
    else if A::SEQUENCE && B::SEQUENCE && A::Head::LEN == B::Head::LEN {
      intersect::<A::Head, B::Head>() & intersect::<A::Tail, B::Tail>()
    } else {
      match (A::START, B::START) {
        // (None, _) if A::SEQUENCE && A::Head::LEN == 0 
        (None, _) | (_, None) => 0b11,
        (Some(a), Some(b)) => loop {
          let i = a & b;
          if i == 0 { break 0b00 }
          let mut res = if i == a { 0b11 } else { 0b01 };
          inline! {
            (J0 0x0) (J1 0x1) (J2 0x2) (J3 0x3) (J4 0x4) (J5 0x5) (J6 0x6) (J7 0x7)
            (J8 0x8) (J9 0x9) (Ja 0xA) (Jb 0xB) (Jc 0xC) (Jd 0xD) (Je 0xE) (Jf 0xF);
            ($T:tt $H:tt) if i & (1 << $H) != 0 {
              if A::BRANCH || B::BRANCH { res = or(res, intersect::<A::$T, B::$T>()); }
              else if i == a {
                break intersect::<A::$T, B::$T>()
              } else {
                break 0b01 & intersect::<A::$T, B::$T>()
              }
            }
          }
          break res
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  type U8 = Any8;
  type NonZeroU8 = NonZero8;

  #[test]
  fn test_intersect() {
    type Boolx8 = (Bool, (Bool, (Bool, (Bool, Bool, Bool, Bool, Bool))));
    type Bool8x8 = (Boolx8, Boolx8, Boolx8, Boolx8, Boolx8, Boolx8, Boolx8, Boolx8);
    type Bool8x8x8 = (Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8);
    type Bool8x8x8x8 = (Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8);

    type U8x8 = (U8, (U8, (U8, (U8, (U8, (U8, (U8, U8)))))));
    type U88x8 = (U8x8, (U8x8, (U8x8, (U8x8, (U8x8, (U8x8, (U8x8, U8x8)))))));
    type U88x8x8 = (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, U88x8)))))));
    type U88x8x8x8 = (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, U88x8x8)))))));

    assert_eq!(0b11, intersect::<U8, U8>());
    assert_eq!(0b11, intersect::<Bool, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    assert_eq!(0b11, intersect::<NonZeroU8, U8>());

    assert_eq!(0b11, intersect::<(U8, Bool), (U8, Bool)>());
    assert_eq!(0b11, intersect::<(Bool, U8), (Bool, U8)>());
    assert_eq!(0b11, intersect::<(U8, (U8, U8)), (U8, (U8, U8))>());
    assert_eq!(0b11, intersect::<(U8, (Bool, NonZeroU8)), (U8, (Bool, NonZeroU8))>());
    assert_eq!(0b01, intersect::<(U8, U8), (NonZeroU8, U8)>());
    assert_eq!(0b01, intersect::<(U8, (U8, NonZeroU8)), (U8, (Bool, NonZeroU8))>());
    assert_eq!(0b01, intersect::<(U8, U8, NonZeroU8), (U8, Bool, NonZeroU8)>());
    assert_eq!(0b01, intersect::<(U8, U8), (Bool, NonZeroU8)>());

    assert_eq!(0b11, intersect::<([U8; 1024], Bool), ([U8; 1024], U8)>());
    assert_eq!(0b01, intersect::<([U8; 1024], U8), ([U8; 1024], Bool)>());
    assert_eq!(0b11, intersect::<([(Bool, Bool, Bool, Bool); 1024], Bool), ([(Bool, Bool, Bool, Bool); 1024], U8)>());
    assert_eq!(0b01, intersect::<([(Bool, Bool, Bool, Bool); 1024], U8), ([(Bool, Bool, Bool, Bool); 1024], Bool)>());
    assert_eq!(0b01, intersect::<([(Bool, Bool, Bool, Bool); 2048], U8), ([(Bool, Bool, Bool, Bool); 2048], Bool)>());
    assert_eq!(0b11, intersect::<([(Bool, Bool, Bool, Bool); 2048], Bool), ([(U8, U8, U8, U8); 2048], U8)>());
    assert_eq!(0b11, intersect::<([(Bool, Bool, Bool, [Bool; 1024]); 2048], Bool), ([(U8, U8, U8, [U8; 1024]); 2048], U8)>());
    assert_eq!(0b11, intersect::<([(Bool, Bool, Bool, [Bool; 2048]); 2048], Bool), ([(U8, U8, U8, [U8; 2048]); 2048], U8)>());
    assert_eq!(0b01, intersect::<([[(Bool, Bool, Bool, [Bool; 2048]); 2048]; 4048], U8), ([[(U8, U8, U8, [U8; 2048]); 2048]; 4048], Bool)>());

    assert_eq!(0b11, intersect::<Bool8x8x8, Bool8x8x8>());
    assert_eq!(0b11, intersect::<U88x8x8x8, U88x8x8x8>());
    assert_eq!(0b11, intersect::<Bool8x8x8x8, U88x8x8x8>());

    assert_eq!(0b11, intersect::<(Bool, Bool8x8x8), (Bool, Bool8x8x8)>());
    assert_eq!(0b11, intersect::<(Bool, U88x8x8x8), (Bool, U88x8x8x8)>());
    assert_eq!(0b11, intersect::<(Bool8x8x8, Bool), (Bool8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(Bool8x8x8, U8), (Bool8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(Bool8x8x8x8, U8), (Bool8x8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(U88x8x8x8, (U88x8x8x8, U8)), (U88x8x8x8, (U88x8x8x8, Bool))>());
    assert_eq!(0b01, intersect::<(Bool8x8x8x8, (U88x8x8x8, (U88x8x8x8, U8))), (Bool8x8x8x8, (U88x8x8x8, (U88x8x8x8, Bool)))>());
    assert_eq!(0b11, intersect::<(Bool8x8x8x8, (U88x8x8x8, (U88x8x8x8, Bool))), (Bool8x8x8x8, (U88x8x8x8, (U88x8x8x8, U8)))>());
    assert_eq!(0b11, intersect::<(Bool8x8x8x8, (U88x8x8x8, U88x8x8x8)), (Bool8x8x8x8, (U88x8x8x8, U88x8x8x8))>());
    assert_eq!(0b11, intersect::<(Bool, (Bool8x8x8x8, (U88x8x8x8, U88x8x8x8))), (U8, (Bool8x8x8x8, (U88x8x8x8, U88x8x8x8)))>());
    assert_eq!(0b11, intersect::<(U8, (Bool8x8x8x8, (U88x8x8x8, U88x8x8x8))), (U8, (Bool8x8x8x8, (U88x8x8x8, U88x8x8x8)))>());

    assert_eq!(0xa, (0xaa >> (4 * 0)) & 0xf);
    assert_eq!(0xa, (0xaa >> (4 * 1)) & 0xf);
    
    assert_eq!(0b11, intersect::<Equ8<0xab>, Equ8<0xab>>());
    assert_eq!(0b11, intersect::<(Equ4<0xb>, Equ4<0xa>), Equ8<0xab>>());
    assert_eq!(0b11, intersect::<(Bool, [(Bool, NonZero8); 1024], Equ8<0xab>, Bool), (Bool, [(Bool, NonZero8); 1024], Equ8<0xab>, Bool)>());
    assert_eq!(0b01, intersect::<(Bool, Bool, Slice<U8>), (Bool, Bool, Bool, Slice<Bool>)>());
    assert_eq!(0b01, intersect::<(Bool, Bool, Slice<U8>), (Bool, Bool, Slice<Bool>)>());
    assert_eq!(0b01, intersect::<(Bool, Bool, [(Bool, U8, NonZero8); 20480], Equ4<0xa>, Slice<Bool>), (Bool, Bool, [(Bool, Bool, NonZero8); 20480], Equ4<0xa>, Slice<Bool>)>());
    // assert_eq!(0b01, intersect::<(Bool, Bool, [(Bool, U8, NonZero8); 20480], Equ4<0xa>, Equ4<0xa>, Slice<Bool>), (Bool, Bool, [(Bool, Bool, NonZero8); 20480], Equ8<0xaa>, Slice<Bool>)>());
    assert_eq!(0b00, intersect::<(Bool, Bool, [(Bool, U8, NonZero8); 20480], Equ4<0xa>, Slice<Bool>), (Bool, Bool, [(Bool, Bool, NonZero8); 20480], Equ4<0xb>, Slice<Bool>)>());
    
    // assert_eq!(0b11, intersect::<(NonZero, NonZero8x8x8), (NonZero, NonZero8x8x8)>());
    // assert_eq!(0b11, intersect::<(NonZero, U88x8x8x8), (NonZero, U88x8x8x8)>());
    // assert_eq!(0b11, intersect::<(NonZero8x8x8, NonZero), (NonZero8x8x8, NonZero)>());
    // assert_eq!(0b01, intersect::<(NonZero8x8x8, U8), (NonZero8x8x8, NonZero)>());
    // assert_eq!(0b01, intersect::<(NonZero8x8x8x8, U8), (NonZero8x8x8x8, NonZero)>());
    // assert_eq!(0b01, intersect::<(U88x8x8x8, (U88x8x8x8, U8)), (U88x8x8x8, (U88x8x8x8, NonZero))>());
    // assert_eq!(0b01, intersect::<(NonZero8x8x8x8, (U88x8x8x8, (U88x8x8x8, U8))), (NonZero8x8x8x8, (U88x8x8x8, (U88x8x8x8, NonZero)))>());
    // assert_eq!(0b11, intersect::<(NonZero8x8x8x8, (U88x8x8x8, (U88x8x8x8, NonZero))), (NonZero8x8x8x8, (U88x8x8x8, (U88x8x8x8, U8)))>());
    // assert_eq!(0b11, intersect::<(NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8)), (NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8))>());
    // assert_eq!(0b11, intersect::<(NonZero, (NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8))), (U8, (NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8)))>());
    // assert_eq!(0b11, intersect::<(U8, (NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8))), (U8, (NonZero8x8x8x8, (U88x8x8x8, U88x8x8x8)))>());

    // for i in 1024usize..=2048usize {
    //   let mut bits = vec![];
    //   for j in 0..(8 * size_of::<usize>() - i.leading_zeros() as usize) {
    //     bits.push(if i & (1 << j) != 0 { "B1" } else { "B0" });
    //   }
    //   let mut s = "()".to_string();
    //   while let Some(b) = bits.pop() {
    //     s = format!("({}, {})", s, b);
    //   }
    //   eprintln!(
    //     "impl IntoLen for PhantomUsize<{}> {{ type Output = {}; }}",
    //     i, s
    //   );
    // }
  }
}
