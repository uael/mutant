// -------------------------------------------------------------------------------------------------\

macro_rules! inline {
  { $( ($( $args:tt )*) ),* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

// -------------------------------------------------------------------------------------------------

pub trait Const<T> {
  const VAL: T;
}

pub struct Uint<const V: u128>;

impl<const V: u128> Const<u128> for Uint<V> {
  const VAL: u128 = V;
}

pub struct Shift<T>(T);

impl<T: Const<u128>> Const<u128> for Shift<T> {
  const VAL: u128 = T::VAL >> 4;
}

pub trait Hex {
  const HEX: u8;
  type WhenZero<T: Bits, E: Bits>: Bits;
}

inline! {
  (X0 0x0 T), (X1 0x1 E), (X2 0x2 E), (X3 0x3 E), (X4 0x4 E), (X5 0x5 E), (X6 0x6 E), (X7 0x7 E),
  (X8 0x8 E), (X9 0x9 E), (XA 0xA E), (XB 0xB E), (XC 0xC E), (XD 0xD E), (XE 0xE E), (XF 0xF E);
  ($T:ident $HEX:literal $R:tt)
  pub struct $T;
  impl Hex for $T { const HEX: u8 = $HEX; type WhenZero<T: Bits, E: Bits> = $R; }
}

pub trait Bits {
  type WhenZero<Then: Bits, Else: Bits>: Bits;
  type Transition<Input: Hex>: Bits;
  const START: Option<u16>;
  const BRANCH: bool = true;
  const ANY: bool = false;
}

impl Bits for () {
  type WhenZero<Then: Bits, Else: Bits> = Then;
  type Transition<Input: Hex> = ();
  const START: Option<u16> = None;
  const BRANCH: bool = false;
}

impl<T: Bits> Bits for [T; 0] {
  type WhenZero<Then: Bits, Else: Bits> = Then;
  type Transition<Input: Hex> = ();
  const START: Option<u16> = None;
  const BRANCH: bool = false;
}

pub struct Any8;
pub struct Any4;

impl Bits for Any8 {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = Any4;
  const START: Option<u16> = Some(0xffff);
  const BRANCH: bool = false;
  const ANY: bool = true;
}

impl Bits for Any4 {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = ();
  const START: Option<u16> = Some(0xffff);
  const BRANCH: bool = false;
  const ANY: bool = true;
}

pub struct NotZero8;
pub struct NotZero4;

impl Bits for NotZero8 {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = Input::WhenZero<NotZero4, Any4>;
  const START: Option<u16> = Some(0xffff);
}

impl Bits for NotZero4 {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = ();
  const START: Option<u16> = Some(!(1 << 0));
  const BRANCH: bool = false;
}

pub struct Equ8<V: Const<u128>>(V);
pub struct Equ4<V: Const<u128>>(V);

impl<V: Const<u128>> Bits for Equ8<V> {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = Equ4<Shift<V>>;
  const START: Option<u16> = Some(1u16 << (V::VAL & 0xf));
  const BRANCH: bool = false;
}

impl<V: Const<u128>> Bits for Equ4<V> {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<Input: Hex> = ();
  const START: Option<u16> = Some(1u16 << (V::VAL & 0xf));
  const BRANCH: bool = false;
}

macro_rules! impl_seq {
  { $T:ident } => { };
  { $T:ident $( $R:ident )+ } => {
    impl_seq! { $( $R )+ }
    #[allow(unused_parens)]
    impl<$T: Bits, $( $R: Bits ),*> Bits for ($T, $( $R, )+) {
      type WhenZero<Then: Bits, Else: Bits> = Else;
      type Transition<H: Hex> = <$T::Transition<H> as Bits>::WhenZero<
        ($( $R ),+),
        ($T::Transition<H>, $( $R, )+),
      >;
      const START: Option<u16> = match $T::START {
        Some(a) => Some(a),
        _ => <($( $R ),+)>::START,
      };
      const BRANCH: bool = $T::BRANCH;
      const ANY: bool = $T::ANY $( & $R::ANY )+;
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

macro_rules! impl_arr {
  { $N:expr; } => { };
  { $N:expr; $T:ident $( $R:ident )* } => {
    impl_arr! { $N - 1; $( $R )* }
    #[allow(unused_parens)]
    impl<T: Bits> Bits for [T; { $N }] {
      type WhenZero<Then: Bits, Else: Bits> = <($T $(, $R )*) as Bits>::WhenZero<Then, Else>;
      type Transition<H: Hex> = <($T $(, $R )*) as Bits>::Transition<H>;
      const START: Option<u16> = <($T $(, $R )*) as Bits>::START;
      const BRANCH: bool = <($T $(, $R )*) as Bits>::BRANCH;
      const ANY: bool = T::ANY;
    }
  };
}

impl_arr! {
  128;
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
  T T T T T T T T T T T T T T T T
}

// impl<T: Bits> Bits for [T; 0] {
//   type Transition<Input: Hex, Then: Bits> = Then;
//   const START: Option<u16> = None;
// }
//
// impl<T: Bits> Bits for [T; 1] {
//   type Transition<Input: Hex, Then: Bits> = T::Transition<Input, Then>;
//   const START: Option<u16> = T::START;
// }
//
// impl<T: Bits> Bits for [T; 2] {
//   type Transition<Input: Hex, Then: Bits> = T::Transition<Input, (T, Then)>;
//   const START: Option<u16> = T::START;
// }

// -------------------------------------------------------------------------------------------------
// `Union`

pub struct Union<A: Bits, B: Bits>(A, B);

impl<A: Bits, B: Bits> Bits for Union<A, B> {
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<H: Hex> =
    <A::Transition<H> as Bits>::WhenZero<(), Union<A::Transition<H>, B::Transition<H>>>;
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
  type WhenZero<Then: Bits, Else: Bits> = Else;
  type Transition<H: Hex> =
    <A::Transition<H> as Bits>::WhenZero<(), Intersection<A::Transition<H>, B::Transition<H>>>;
  const START: Option<u16> = match (A::START, B::START) {
    (Some(a), Some(b)) => Some(a & b),
    _ => None,
  };
}

// -------------------------------------------------------------------------------------------------
// `intersect`

#[inline(always)]
const fn or(a: u8, b: u8) -> u8 {
  (a & b) | (a != 0) as u8 | (b != 0) as u8
}

#[inline(always)]
const fn intersect_next<A: Bits, B: Bits, H: Hex>() -> u8 {
  intersect_impl::<A::Transition<H>, B::Transition<H>>()
}

#[inline(always)]
const fn intersect_impl<A: Bits, B: Bits>() -> u8 {
  const {
    match (A::START, B::START) {
      _ if B::ANY => 0b11,
      (None, _) | (_, None) => 0b11,
      (Some(a), Some(b)) => loop {
        let i = a & b;
        if i == 0 { break 0b00 }
        let mut res = if i == a { 0b11 } else { 0b01 };
        inline! {
          (X0 0x0), (X1 0x1), (X2 0x2), (X3 0x3), (X4 0x4), (X5 0x5), (X6 0x6), (X7 0x7),
          (X8 0x8), (X9 0x9), (XA 0xA), (XB 0xB), (XC 0xC), (XD 0xD), (XE 0xE), (XF 0xF);
          ($T:tt $H:tt) if i & (1 << $H) != 0 {
            if A::BRANCH | B::BRANCH { res = or(res, intersect_next::<A, B, $T>()); }
            else if i == a {
              break intersect_next::<A, B, $T>()
            } else {
              break 0b01 & intersect_next::<A, B, $T>()
            }
          }
        }
        break res
      }
    }
  }
}

pub const fn intersect<A: Bits, B: Bits>() -> u8 {
  intersect_impl::<A, B>()
}

#[cfg(test)]
mod tests {
  use super::*;

  type Bool = Union<Equ8<Uint<0>>, Equ8<Uint<1>>>;
  type U8 = Any8;
  type NonZeroU8 = NotZero8;

  #[test]
  fn test_intersect() {
    println!("{}", core::any::type_name::<<(Any8, Any8) as Bits>::Transition<X0>>());
    println!("{}", core::any::type_name::<<(Any4, Any8) as Bits>::Transition<X0>>());
    println!("{}", core::any::type_name::<<Any8 as Bits>::Transition<X0>>());
    println!("{}", core::any::type_name::<<Any4 as Bits>::Transition<X0>>());

    println!("{}", core::any::type_name::<<(Any8, (Any8, Any8)) as Bits>::Transition<X0>>());
    println!("{}", core::any::type_name::<<(Any4, (Any8, Any8)) as Bits>::Transition<X0>>());

    assert_eq!(0b11, intersect::<U8, U8>());
    assert_eq!(0b11, intersect::<Bool, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    assert_eq!(0b11, intersect::<NonZeroU8, U8>());

    // assert_eq!(0b01, intersect::<([[(U8, U8, U8, U8); 128]; 128], U8), ([[(U8, U8, U8, U8); 128]; 128], Bool)>());

    // assert_eq!(0b11, intersect::<[(U8, Bool, NonZeroU8); 128], [(U8, Bool, NonZeroU8); 128]>());
    // assert_eq!(0b11, intersect::<[(U8, [Bool; 128], NonZeroU8); 128], [(U8, [Bool; 128], NonZeroU8); 128]>());

    assert_eq!(0b01, intersect::<Any4, Union<Equ4<Uint<0>>, Equ4<Uint<0>>>>());
    assert_eq!(0b01, intersect::<Any8, Union<Equ8<Uint<0>>, Equ8<Uint<1>>>>());
    assert_eq!(0b11, intersect::<NotZero4, NotZero4>());
    assert_eq!(0b01, intersect::<(NotZero4, Any8), (NotZero4, Bool)>());
    assert_eq!(0b01, intersect::<(NonZeroU8, U8), (NonZeroU8, Bool)>());

    assert_eq!(0b11, intersect::<(U8, Bool), (U8, Bool)>());
    assert_eq!(0b11, intersect::<(Bool, U8), (Bool, U8)>());
    assert_eq!(0b11, intersect::<(U8, (U8, U8)), (U8, (U8, U8))>());
    assert_eq!(0b11, intersect::<(U8, (Bool, NonZeroU8)), (U8, (Bool, NonZeroU8))>());
    assert_eq!(0b01, intersect::<(U8, U8), (NonZeroU8, U8)>());
    assert_eq!(0b01, intersect::<(U8, (U8, NonZeroU8)), (U8, (Bool, NonZeroU8))>());
    assert_eq!(0b01, intersect::<(U8, U8, NonZeroU8), (U8, Bool, NonZeroU8)>());
    assert_eq!(0b01, intersect::<(U8, U8), (Bool, NonZeroU8)>());

    type Bool8 = (Bool, (Bool, (Bool, (Bool, (Bool, (Bool, (Bool, Bool)))))));
    type Bool8x8 = (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, Bool8)))))));
    type Bool8x8x8 = (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, Bool8x8)))))));
    type Bool8x8x8x8 = (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, Bool8x8x8)))))));

    type U88 = (U8, (U8, (U8, (U8, (U8, (U8, (U8, U8)))))));
    type U88x8 = (U88, (U88, (U88, (U88, (U88, (U88, (U88, U88)))))));
    type U88x8x8 = (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, U88x8)))))));
    type U88x8x8x8 = (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, U88x8x8)))))));

    assert_eq!(0b01, intersect::<(Bool8x8x8, U8), (Bool8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(Bool8x8x8x8, U8), (Bool8x8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(U88x8x8x8, U88x8x8x8, U8), (U88x8x8x8, U88x8x8x8, Bool)>());
    assert_eq!(0b01, intersect::<(Bool8x8x8x8, U88x8x8x8, U88x8x8x8, U8), (Bool8x8x8x8, U88x8x8x8, U88x8x8x8, Bool)>());
    assert_eq!(0b11, intersect::<(Bool8x8x8x8, U88x8x8x8, U88x8x8x8, Bool), (Bool8x8x8x8, U88x8x8x8, U88x8x8x8, U8)>());
    assert_eq!(0b11, intersect::<(Bool8x8x8x8, U88x8x8x8, U88x8x8x8), (Bool8x8x8x8, U88x8x8x8, U88x8x8x8)>());
    assert_eq!(0b11, intersect::<(Bool, Bool8x8x8x8, U88x8x8x8, U88x8x8x8), (U8, Bool8x8x8x8, U88x8x8x8, U88x8x8x8)>());
    assert_eq!(0b11, intersect::<(U8, Bool8x8x8x8, U88x8x8x8, U88x8x8x8), (U8, Bool8x8x8x8, U88x8x8x8, U88x8x8x8)>());

    assert_eq!(true, U88x8x8x8::ANY);

    assert_eq!(0b11, intersect::<U88x8x8x8, U88x8x8x8>());
    assert_eq!(0b11, intersect::<Bool8x8x8x8, U88x8x8x8>());
    // assert_eq!(0b11, intersect::<(Bool, NonZeroU8, NonZeroU16), U32>());
    // assert_eq!(0b01, intersect::<U32, (Bool, NonZeroU8, NonZeroU16)>());
  }
}
