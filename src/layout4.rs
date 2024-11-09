macro_rules! inline {
  { $( ($( $args:tt )*) )* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

pub trait Len {
  type Inc: Len;
  type Dec: Len;
  type Peek<T: Dfa, E: Dfa>: Dfa;
  type IfZero<Then: Len, Else: Len>: Len;
  const USIZE: usize;
}

pub struct B0;
pub struct B1;

const _: () = {
  impl Len for () {
    type Inc = ((), B1);
    type Dec = ();
    type Peek<T: Dfa, E: Dfa> = E;
    type IfZero<Then: Len, Else: Len> = Then;
    const USIZE: usize = 0;
  }

  impl<U: Len> Len for (U, B0) {
    type Inc = (U, B1);
    type Dec = (U::Dec, B1);
    type Peek<T: Dfa, E: Dfa> = T;
    type IfZero<Then: Len, Else: Len> = Else;
    const USIZE: usize = 0 | (U::USIZE << 1);
  }

  impl<U: Len> Len for (U, B1) {
    type Inc = (U::Inc, B0);
    type Dec = U::IfZero<(), (U, B0)>;
    type Peek<T: Dfa, E: Dfa> = T;
    type IfZero<Then: Len, Else: Len> = Else;
    const USIZE: usize = 1 | (U::USIZE << 1);
  }
};

pub trait IntoLen {
  type Output: Len;
}

pub struct PhantomUsize<const USIZE: usize>;

pub trait Dfa {
  type J0: Dfa; type J1: Dfa; type J2: Dfa; type J3: Dfa;
  type J4: Dfa; type J5: Dfa; type J6: Dfa; type J7: Dfa;
  type J8: Dfa; type J9: Dfa; type Ja: Dfa; type Jb: Dfa;
  type Jc: Dfa; type Jd: Dfa; type Je: Dfa; type Jf: Dfa;

  type Head: Dfa;
  type Tail: Dfa;
  type IfZero<Then: Dfa, Else: Dfa>: Dfa;

  const START: Option<u16>;
  const BRANCH: bool;
  const ANY: bool;
  const LEN: usize;
}

pub struct Any8;
pub struct Any4;
pub struct NonZero8;
pub struct NonZero4;
pub struct Zero;
pub struct Bool;

impl Dfa for () {
  type J0 = (); type J1 = (); type J2 = (); type J3 = ();
  type J4 = (); type J5 = (); type J6 = (); type J7 = ();
  type J8 = (); type J9 = (); type Ja = (); type Jb = ();
  type Jc = (); type Jd = (); type Je = (); type Jf = ();

  type Head = ();
  type Tail = ();
  type IfZero<Then: Dfa, Else: Dfa> = Then;

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
  type IfZero<Then: Dfa, Else: Dfa> = Else;

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
  type IfZero<Then: Dfa, Else: Dfa> = Else;

  const START: Option<u16> = Some(0xffff); // any half-byte.
  const BRANCH: bool = false; // accept any half-byte, hence no branching.
  const ANY: bool = true; // accept any half-byte.
  const LEN: usize = 1; // 1 half-byte.
}

impl Dfa for NonZero8 {
  type J0 = NonZero4; type J1 = Any4; type J2 = Any4; type J3 = Any4;
  type J4 = Any4; type J5 = Any4; type J6 = Any4; type J7 = Any4;
  type J8 = Any4; type J9 = Any4; type Ja = Any4; type Jb = Any4;
  type Jc = Any4; type Jd = Any4; type Je = Any4; type Jf = Any4;

  type Head = Self;
  type Tail = ();
  type IfZero<Then: Dfa, Else: Dfa> = Else;

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
  type IfZero<Then: Dfa, Else: Dfa> = Else;

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
  type IfZero<Then: Dfa, Else: Dfa> = Else;

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
  type IfZero<Then: Dfa, Else: Dfa> = Else;

  const START: Option<u16> = Some(0b11);
  const BRANCH: bool = false;
  const ANY: bool = false;
  const LEN: usize = 2;
}

pub struct Array<T, N>(T, N);

impl<T: Dfa, N: Len> Dfa for Array<T, N> {
  type J0 = N::Peek<(T::J0, Array<T, N::Dec>), ()>;
  type J1 = N::Peek<(T::J1, Array<T, N::Dec>), ()>;
  type J2 = N::Peek<(T::J2, Array<T, N::Dec>), ()>;
  type J3 = N::Peek<(T::J3, Array<T, N::Dec>), ()>;
  type J4 = N::Peek<(T::J4, Array<T, N::Dec>), ()>;
  type J5 = N::Peek<(T::J5, Array<T, N::Dec>), ()>;
  type J6 = N::Peek<(T::J6, Array<T, N::Dec>), ()>;
  type J7 = N::Peek<(T::J7, Array<T, N::Dec>), ()>;
  type J8 = N::Peek<(T::J8, Array<T, N::Dec>), ()>;
  type J9 = N::Peek<(T::J9, Array<T, N::Dec>), ()>;
  type Ja = N::Peek<(T::Ja, Array<T, N::Dec>), ()>;
  type Jb = N::Peek<(T::Jb, Array<T, N::Dec>), ()>;
  type Jc = N::Peek<(T::Jc, Array<T, N::Dec>), ()>;
  type Jd = N::Peek<(T::Jd, Array<T, N::Dec>), ()>;
  type Je = N::Peek<(T::Je, Array<T, N::Dec>), ()>;
  type Jf = N::Peek<(T::Jf, Array<T, N::Dec>), ()>;

  type Head = N::Peek<T, ()>;
  type Tail = N::Peek<Array<T, N::Dec>, ()>;
  type IfZero<Then: Dfa, Else: Dfa> = N::Peek<Else, Then>;

  const START: Option<u16> = if N::USIZE == 0 { None } else { T::START };
  const BRANCH: bool = N::USIZE != 0 && T::BRANCH;
  const ANY: bool = N::USIZE == 0 || T::ANY;
  const LEN: usize = N::USIZE * T::LEN;
}

impl<T: Dfa, N: Len, const LEN: usize> Dfa for [T; LEN]
where
  PhantomUsize<LEN>: IntoLen<Output = N>
{
  type J0 = N::Peek<(T::J0, Array<T, N::Dec>), ()>;
  type J1 = N::Peek<(T::J1, Array<T, N::Dec>), ()>;
  type J2 = N::Peek<(T::J2, Array<T, N::Dec>), ()>;
  type J3 = N::Peek<(T::J3, Array<T, N::Dec>), ()>;
  type J4 = N::Peek<(T::J4, Array<T, N::Dec>), ()>;
  type J5 = N::Peek<(T::J5, Array<T, N::Dec>), ()>;
  type J6 = N::Peek<(T::J6, Array<T, N::Dec>), ()>;
  type J7 = N::Peek<(T::J7, Array<T, N::Dec>), ()>;
  type J8 = N::Peek<(T::J8, Array<T, N::Dec>), ()>;
  type J9 = N::Peek<(T::J9, Array<T, N::Dec>), ()>;
  type Ja = N::Peek<(T::Ja, Array<T, N::Dec>), ()>;
  type Jb = N::Peek<(T::Jb, Array<T, N::Dec>), ()>;
  type Jc = N::Peek<(T::Jc, Array<T, N::Dec>), ()>;
  type Jd = N::Peek<(T::Jd, Array<T, N::Dec>), ()>;
  type Je = N::Peek<(T::Je, Array<T, N::Dec>), ()>;
  type Jf = N::Peek<(T::Jf, Array<T, N::Dec>), ()>;

  type Head = N::Peek<T, ()>;
  type Tail = N::Peek<Array<T, N::Dec>, ()>;
  type IfZero<Then: Dfa, Else: Dfa> = N::Peek<Else, Then>;

  const START: Option<u16> = if LEN == 0 { None } else { T::START };
  const BRANCH: bool = LEN != 0 && T::BRANCH;
  const ANY: bool = LEN == 0 || T::ANY;
  const LEN: usize = LEN * T::LEN;
}

macro_rules! impl_seq {
  { $T:ident } => { };
  { $T:ident $( $R:ident )+ } => {
    impl_seq! { $( $R )+ }
    #[allow(unused_parens)]
    impl<$T: Dfa, $( $R: Dfa ),*> Dfa for ($T, $( $R, )+) {
      type J0 = <$T::J0 as Dfa>::IfZero<($( $R ),+), ($T::J0, $( $R, )+)>;
      type J1 = <$T::J1 as Dfa>::IfZero<($( $R ),+), ($T::J1, $( $R, )+)>;
      type J2 = <$T::J2 as Dfa>::IfZero<($( $R ),+), ($T::J2, $( $R, )+)>;
      type J3 = <$T::J3 as Dfa>::IfZero<($( $R ),+), ($T::J3, $( $R, )+)>;
      type J4 = <$T::J4 as Dfa>::IfZero<($( $R ),+), ($T::J4, $( $R, )+)>;
      type J5 = <$T::J5 as Dfa>::IfZero<($( $R ),+), ($T::J5, $( $R, )+)>;
      type J6 = <$T::J6 as Dfa>::IfZero<($( $R ),+), ($T::J6, $( $R, )+)>;
      type J7 = <$T::J7 as Dfa>::IfZero<($( $R ),+), ($T::J7, $( $R, )+)>;
      type J8 = <$T::J8 as Dfa>::IfZero<($( $R ),+), ($T::J8, $( $R, )+)>;
      type J9 = <$T::J9 as Dfa>::IfZero<($( $R ),+), ($T::J9, $( $R, )+)>;
      type Ja = <$T::Ja as Dfa>::IfZero<($( $R ),+), ($T::Ja, $( $R, )+)>;
      type Jb = <$T::Jb as Dfa>::IfZero<($( $R ),+), ($T::Jb, $( $R, )+)>;
      type Jc = <$T::Jc as Dfa>::IfZero<($( $R ),+), ($T::Jc, $( $R, )+)>;
      type Jd = <$T::Jd as Dfa>::IfZero<($( $R ),+), ($T::Jd, $( $R, )+)>;
      type Je = <$T::Je as Dfa>::IfZero<($( $R ),+), ($T::Je, $( $R, )+)>;
      type Jf = <$T::Jf as Dfa>::IfZero<($( $R ),+), ($T::Jf, $( $R, )+)>;

      type Head = $T;
      type Tail = ($( $R ),+);
      type IfZero<Then: Dfa, Else: Dfa> = Else;

      const START: Option<u16> = match $T::START { Some(a) => Some(a), _ => <($( $R ),+)>::START  };
      const BRANCH: bool = $T::BRANCH;
      const ANY: bool = $T::ANY $( && $R::ANY )+;
      const LEN: usize = $T::LEN $( + $R::LEN )+;
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

pub const fn intersect<A: Dfa, B: Dfa>() -> u8 {
  const {
    // early optimization when target is any.
    if B::ANY { 0b11 }
    // non-overlapping states.
    else if A::Tail::LEN > 0 && A::Head::LEN == B::Head::LEN {
      intersect::<A::Head, B::Head>() & intersect::<A::Tail, B::Tail>()
    } else {
      match (A::START, B::START) {
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
    type Bool8 = (Bool, Bool, Bool, Bool, Bool, Bool, Bool, Bool);
    type Bool8x8 = (Bool8, Bool8, Bool8, Bool8, Bool8, Bool8, Bool8, Bool8);
    type Bool8x8x8 = (Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8, Bool8x8);
    type Bool8x8x8x8 = (Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8, Bool8x8x8);

    type U88 = (U8, (U8, (U8, (U8, (U8, (U8, (U8, U8)))))));
    type U88x8 = (U88, (U88, (U88, (U88, (U88, (U88, (U88, U88)))))));
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
    assert_eq!(0b01, intersect::<([(Bool, Bool, Bool, [Bool; 128]); 2048], U8), ([(U8, U8, U8, [U8; 128]); 2048], Bool)>());

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
