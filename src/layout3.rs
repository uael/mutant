// -------------------------------------------------------------------------------------------------\

macro_rules! inline {
  { $( ($( $args:tt )*) )* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

// -------------------------------------------------------------------------------------------------
// `Hex`

pub trait Hex4 {
  type I0: Hex; type I1: Hex; type I2: Hex; type I3: Hex;

  type And<R: Hex4>: Hex4;
  type Or<R: Hex4>: Hex4;
  type Eq<R: Hex4>: Hex;

  const U16: u16;
}

pub trait Hex16 {
  type I0: Hex; type I1: Hex; type I2: Hex; type I3: Hex;
  type I4: Hex; type I5: Hex; type I6: Hex; type I7: Hex;
  type I8: Hex; type I9: Hex; type Ia: Hex; type Ib: Hex;
  type Ic: Hex; type Id: Hex; type Ie: Hex; type If: Hex;

  type And<R: Hex16>: Hex16;
  type Or<R: Hex16>: Hex16;

  const U64: u64;
}

impl<I0: Hex, I1: Hex, I2: Hex, I3: Hex> Hex4 for (I0, I1, I2, I3) {
  type I0 = I0; type I1 = I1; type I2 = I2; type I3 = I3;

  type And<R: Hex4> = (I0::And<R::I0>, I1::And<R::I1>, I2::And<R::I2>, I3::And<R::I3>);
  type Or<R: Hex4> = (I0::Or<R::I0>, I1::Or<R::I1>, I2::Or<R::I2>, I3::Or<R::I3>);
  type Eq<R: Hex4> = <I0::Eq<R::I0> as Hex>::And<
    <I1::Eq<R::I1> as Hex>::And<
      <I2::Eq<R::I2> as Hex>::And<I3::Eq<R::I3>>
    >
  >;

  const U16: u16 = (I0::HEX as u16) << 0 | (I1::HEX as u16) << 4 | (I2::HEX as u16) << 8 | (I3::HEX as u16) << 12;
}

impl<I0: Hex, I1: Hex, I2: Hex, I3: Hex, I4: Hex, I5: Hex, I6: Hex, I7: Hex,
     I8: Hex, I9: Hex, Ia: Hex, Ib: Hex, Ic: Hex, Id: Hex, Ie: Hex, If: Hex> Hex16
  for (I0, I1, I2, I3, I4, I5, I6, I7, I8, I9, Ia, Ib, Ic, Id, Ie, If)
{
  type I0 = I0; type I1 = I1; type I2 = I2; type I3 = I3;
  type I4 = I4; type I5 = I5; type I6 = I6; type I7 = I7;
  type I8 = I8; type I9 = I9; type Ia = Ia; type Ib = Ib;
  type Ic = Ic; type Id = Id; type Ie = Ie; type If = If;

  type And<R: Hex16> = (
    I0::And<R::I0>, I1::And<R::I1>, I2::And<R::I2>, I3::And<R::I3>,
    I4::And<R::I4>, I5::And<R::I5>, I6::And<R::I6>, I7::And<R::I7>,
    I8::And<R::I8>, I9::And<R::I9>, Ia::And<R::Ia>, Ib::And<R::Ib>,
    Ic::And<R::Ic>, Id::And<R::Id>, Ie::And<R::Ie>, If::And<R::If>,
  );
  type Or<R: Hex16> = (
    I0::Or<R::I0>, I1::Or<R::I1>, I2::Or<R::I2>, I3::Or<R::I3>,
    I4::Or<R::I4>, I5::Or<R::I5>, I6::Or<R::I6>, I7::Or<R::I7>,
    I8::Or<R::I8>, I9::Or<R::I9>, Ia::Or<R::Ia>, Ib::Or<R::Ib>,
    Ic::Or<R::Ic>, Id::Or<R::Id>, Ie::Or<R::Ie>, If::Or<R::If>,
  );

  const U64: u64 =
    (I0::HEX as u64) <<  0 | (I1::HEX as u64) <<  4 | (I2::HEX as u64) <<  8 | (I3::HEX as u64) << 12 |
    (I4::HEX as u64) << 16 | (I5::HEX as u64) << 20 | (I6::HEX as u64) << 24 | (I7::HEX as u64) << 28 |
    (I8::HEX as u64) << 32 | (I9::HEX as u64) << 36 | (Ia::HEX as u64) << 40 | (Ib::HEX as u64) << 44 |
    (Ic::HEX as u64) << 48 | (Id::HEX as u64) << 52 | (Ie::HEX as u64) << 56 | (If::HEX as u64) << 60;
}

pub trait Transitions {
  type I0: Dfa; type I1: Dfa; type I2: Dfa; type I3: Dfa;
  type I4: Dfa; type I5: Dfa; type I6: Dfa; type I7: Dfa;
  type I8: Dfa; type I9: Dfa; type Ia: Dfa; type Ib: Dfa;
  type Ic: Dfa; type Id: Dfa; type Ie: Dfa; type If: Dfa;
}

impl<I0: Dfa, I1: Dfa, I2: Dfa, I3: Dfa, I4: Dfa, I5: Dfa, I6: Dfa, I7: Dfa,
     I8: Dfa, I9: Dfa, Ia: Dfa, Ib: Dfa, Ic: Dfa, Id: Dfa, Ie: Dfa, If: Dfa> Transitions
for (I0, I1, I2, I3, I4, I5, I6, I7, I8, I9, Ia, Ib, Ic, Id, Ie, If)
{
  type I0 = I0; type I1 = I1; type I2 = I2; type I3 = I3;
  type I4 = I4; type I5 = I5; type I6 = I6; type I7 = I7;
  type I8 = I8; type I9 = I9; type Ia = Ia; type Ib = Ib;
  type Ic = Ic; type Id = Id; type Ie = Ie; type If = If;
}


pub trait Hex {
  const HEX: u8;
  type Idx<T: Hex16>: Hex;
  type Apply<T: Transitions>: Dfa;
  type And<R: Hex>: Hex;
  type Or<R: Hex>: Hex;
  type Eq<R: Hex>: Hex;
}

pub trait Bit {
  const BIT: bool;
}

inline! {
  (X0 0x0 I0 (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0) (X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, Xa, Xb, Xc, Xd, Xe, Xf) (X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X1 0x1 I1 (X0, X1, X0, X1, X0, X1, X0, X1, X0, X1, X0, X1, X0, X1, X0, X1) (X1, X1, X3, X3, X5, X5, X7, X7, X9, X9, Xb, Xb, Xd, Xd, Xf, Xf) (X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X2 0x2 I2 (X0, X0, X2, X2, X0, X0, X2, X2, X0, X0, X2, X2, X0, X0, X2, X2) (X2, X3, X2, X3, X6, X7, X6, X7, Xa, Xb, Xa, Xb, Xe, Xf, Xe, Xf) (X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X3 0x3 I3 (X0, X1, X2, X3, X0, X1, X2, X3, X0, X1, X2, X3, X0, X1, X2, X3) (X3, X3, X3, X3, X7, X7, X7, X7, Xb, Xb, Xb, Xb, Xf, Xf, Xf, Xf) (X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X4 0x4 I4 (X0, X0, X0, X0, X4, X4, X4, X4, X0, X0, X0, X0, X4, X4, X4, X4) (X4, X5, X6, X7, X4, X5, X6, X7, Xc, Xd, Xe, Xf, Xc, Xd, Xe, Xf) (X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X5 0x5 I5 (X0, X1, X0, X1, X4, X5, X4, X5, X0, X1, X0, X1, X4, X5, X4, X5) (X5, X5, X7, X7, X5, X5, X7, X7, Xd, Xd, Xf, Xf, Xd, Xd, Xf, Xf) (X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X6 0x6 I6 (X0, X0, X2, X2, X4, X4, X6, X6, X0, X0, X2, X2, X4, X4, X6, X6) (X6, X7, X6, X7, X6, X7, X6, X7, Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf) (X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0, X0))
  (X7 0x7 I7 (X0, X1, X2, X3, X4, X5, X6, X7, X0, X1, X2, X3, X4, X5, X6, X7) (X7, X7, X7, X7, X7, X7, X7, X7, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf) (X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0, X0))
  (X8 0x8 I8 (X0, X0, X0, X0, X0, X0, X0, X0, X8, X8, X8, X8, X8, X8, X8, X8) (X8, X9, Xa, Xb, Xc, Xd, Xe, Xf, X8, X9, Xa, Xb, Xc, Xd, Xe, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0, X0))
  (X9 0x9 I9 (X0, X1, X0, X1, X0, X1, X0, X1, X8, X9, X8, X9, X8, X9, X8, X9) (X9, X9, Xb, Xb, Xd, Xd, Xf, Xf, X9, X9, Xb, Xb, Xd, Xd, Xf, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0, X0))
  (Xa 0xa Ia (X0, X0, X2, X2, X0, X0, X2, X2, X8, X8, Xa, Xa, X8, X8, Xa, Xa) (Xa, Xb, Xa, Xb, Xe, Xf, Xe, Xf, Xa, Xb, Xa, Xb, Xe, Xf, Xe, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0, X0))
  (Xb 0xb Ib (X0, X1, X2, X3, X0, X1, X2, X3, X8, X9, Xa, Xb, X8, X9, Xa, Xb) (Xb, Xb, Xb, Xb, Xf, Xf, Xf, Xf, Xb, Xb, Xb, Xb, Xf, Xf, Xf, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0, X0))
  (Xc 0xc Ic (X0, X0, X0, X0, X4, X4, X4, X4, X8, X8, X8, X8, Xc, Xc, Xc, Xc) (Xc, Xd, Xe, Xf, Xc, Xd, Xe, Xf, Xc, Xd, Xe, Xf, Xc, Xd, Xe, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0, X0))
  (Xd 0xd Id (X0, X1, X0, X1, X4, X5, X4, X5, X8, X9, X8, X9, Xc, Xd, Xc, Xd) (Xd, Xd, Xf, Xf, Xd, Xd, Xf, Xf, Xd, Xd, Xf, Xf, Xd, Xd, Xf, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0, X0))
  (Xe 0xe Ie (X0, X0, X2, X2, X4, X4, X6, X6, X8, X8, Xa, Xa, Xc, Xc, Xe, Xe) (Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf, Xe, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1, X0))
  (Xf 0xf If (X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, Xa, Xb, Xc, Xd, Xe, Xf) (Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf, Xf) (X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X0, X1))
  ;
  ($T:tt $HEX:tt $Idx:tt $And:tt $Or:tt $Eq:tt)
  pub struct $T;
  impl Hex for $T {
    const HEX: u8 = $HEX;
    type Idx<T: Hex16> = T::$Idx;
    type Apply<T: Transitions> = T::$Idx;
    type And<R: Hex> = R::Idx<$And>;
    type Or<R: Hex> = R::Idx<$Or>;
    type Eq<R: Hex> = R::Idx<$Eq>;
  }
}

// -------------------------------------------------------------------------------------------------

pub trait Dfa<Then = End> {
  type Transition<H: Hex>: Dfa;
  type Start: Hex4;
  type Cat<X: Dfa>: Dfa;
  const END: bool = false;
}

pub trait Dfa2 {
  type J0: Dfa2; type J1: Dfa2; type J2: Dfa2; type J3: Dfa2;
  type J4: Dfa2; type J5: Dfa2; type J6: Dfa2; type J7: Dfa2;
  type J8: Dfa2; type J9: Dfa2; type Ja: Dfa2; type Jb: Dfa2;
  type Jc: Dfa2; type Jd: Dfa2; type Je: Dfa2; type Jf: Dfa2;
}

impl<Then: Dfa> Dfa<Then> for () {
  type Transition<H: Hex> = Then::Transition<H>;
  type Start = Then::Start;
  type Cat<X: Dfa> = X;
}

pub struct End;

impl Dfa for End {
  type Transition<H: Hex> = End;
  type Start = (X0, X0, X0, X0);
  type Cat<X: Dfa> = X;
  const END: bool = true;
}

pub struct Any8;

impl NonZero for Any8 {}

impl<Then: Dfa> Dfa<Then> for Any8 {
  type Transition<H: Hex> = (Any4, Then);
  type Start = (Xf, Xf, Xf, Xf);
  type Cat<X: Dfa> = (Self, X);
}

pub struct Any4;

impl NonZero for Any4 {}

impl<Then: Dfa> Dfa<Then> for Any4 {
  type Transition<H: Hex> = Then;
  type Start = (Xf, Xf, Xf, Xf);
  type Cat<X: Dfa> = (Self, X);
}

pub struct NonZero8;

impl NonZero for NonZero8 {}

impl<Then: Dfa> Dfa<Then> for NonZero8 {
  type Transition<H: Hex> = H::Apply<(
    (NonZero4, Then), (Any4, Then), (Any4, Then), (Any4, Then),
    (Any4, Then), (Any4, Then), (Any4, Then), (Any4, Then),
    (Any4, Then), (Any4, Then), (Any4, Then), (Any4, Then),
    (Any4, Then), (Any4, Then), (Any4, Then), (Any4, Then),
  )>;
  type Start = (Xf, Xf, Xf, Xf);
  type Cat<X: Dfa> = (Self, X);
}

pub struct NonZero4;

impl NonZero for NonZero4 {}

impl<Then: Dfa> Dfa<Then> for NonZero4 {
  type Transition<H: Hex> = Then;
  type Start = (Xe, Xf, Xf, Xf);
  type Cat<X: Dfa> = (Self, X);
}

pub struct Zero;

impl NonZero for Zero {}

impl<Then: Dfa> Dfa<Then> for Zero {
  type Transition<H: Hex> = Then;
  type Start = (X1, X0, X0, X0);
  type Cat<X: Dfa> = (Self, X);
}

pub struct Bool;

impl NonZero for Bool {}

impl<Then: Dfa> Dfa<Then> for Bool {
  type Transition<H: Hex> = (Zero, Then);
  type Start = (X3, X0, X0, X0);
  type Cat<X: Dfa> = (Self, X);
}

impl<A, B> NonZero for (A, B) {}

impl<A: Dfa<B>, B: Dfa, Then: Dfa> Dfa<Then> for (A, B) {
  type Transition<H: Hex> = <A::Transition<H> as Dfa>::Cat<Then>;
  type Start = A::Start;
  type Cat<X: Dfa> = (Self, X);
}

pub struct Intersection<A, B>(A, B);

impl<A, B> NonZero for Intersection<A, B> {}

impl<A: Dfa, B: Dfa> Dfa for Intersection<A, B> {
  type Transition<H: Hex> = Intersection<A::Transition<H>, B::Transition<H>>;
  type Start = <A::Start as Hex4>::And<B::Start>;
  type Cat<X: Dfa> = Intersection<A::Cat<X>, B::Cat<X>>;
}

#[inline(always)]
const fn or(a: u8, b: u8) -> u8 {
  (a & b) | (a != 0) as u8 | (b != 0) as u8
}

#[inline(always)]
const fn start<T: Dfa>() -> Option<u16> {
  match T::END {
    true => None,
    _ => Some(T::Start::U16),
  }
}

trait Intersect<Rhs> {
  type Output;
}

// impl<A: Dfa, B: Dfa> Intersect<B> for A {
//   type Output = A::Start::And<B::Start>;
// }

trait NonZero {}

trait SubsetOf<Rhs> {}

impl SubsetOf<End> for End {}

impl<A: Dfa, B: Dfa> SubsetOf<B> for A
where
  A: NonZero, B: NonZero,
  <A::Start as Hex4>::And<B::Start>: Hex4<Eq<A::Start> = X1>,
  A::Transition<X0>: SubsetOf<B::Transition<X0>>, A::Transition<X1>: SubsetOf<B::Transition<X1>>,
  A::Transition<X2>: SubsetOf<B::Transition<X2>>, A::Transition<X3>: SubsetOf<B::Transition<X3>>,
  A::Transition<X4>: SubsetOf<B::Transition<X4>>, A::Transition<X5>: SubsetOf<B::Transition<X5>>,
  A::Transition<X6>: SubsetOf<B::Transition<X6>>, A::Transition<X7>: SubsetOf<B::Transition<X7>>,
  A::Transition<X8>: SubsetOf<B::Transition<X8>>, A::Transition<X9>: SubsetOf<B::Transition<X9>>,
  A::Transition<Xa>: SubsetOf<B::Transition<Xa>>, A::Transition<Xb>: SubsetOf<B::Transition<Xb>>,
  A::Transition<Xc>: SubsetOf<B::Transition<Xc>>, A::Transition<Xd>: SubsetOf<B::Transition<Xd>>,
  A::Transition<Xe>: SubsetOf<B::Transition<Xe>>, A::Transition<Xf>: SubsetOf<B::Transition<Xf>>,
{}

#[inline(always)]
const fn intersect_next<A: Dfa, B: Dfa, H: Hex>() -> u8 {
  intersect_impl::<A::Transition<H>, B::Transition<H>>()
}

#[inline(always)]
const fn intersect_impl<A: Dfa, B: Dfa>() -> u8 {
  const {
    match (start::<A>(), start::<B>()) {
      (None, _) => 0b11,
      (_, None) => 0b00,
      (Some(a), Some(b)) => loop {
        let i = a & b;
        if i == 0 {
          break 0b00;
        }
        let mut res = if i == a { 0b11 } else { 0b01 };
        inline! {
          (X0 0x0) (X1 0x1) (X2 0x2) (X3 0x3) (X4 0x4) (X5 0x5) (X6 0x6) (X7 0x7)
          (X8 0x8) (X9 0x9) (Xa 0xA) (Xb 0xB) (Xc 0xC) (Xd 0xD) (Xe 0xE) (Xf 0xF);
          ($T:tt $HEX:tt) if i & (1 << $HEX) != 0 { res = or(res, intersect_next::<A, B, $T>()); }
        }
        break res;
      },
    }
  }
}

pub const fn intersect<A: Dfa, B: Dfa>() -> u8 {
  intersect_impl::<A, B>()
}

#[derive(Debug)]
pub enum Tree {
  Ok,
  Nop,
  Lut([&'static Tree; 16]),
}

impl Tree {
  fn len(&self) -> usize {
    1 + match self {
      Self::Ok | Self::Nop => 0,
      Self::Lut(lut) => {
        lut.iter().map(|node| node.len()).max().unwrap_or(0)
      },
    }
  }
}

#[inline(always)]
const fn compile_next<T: Dfa, H: Hex>() -> &'static Tree {
  compile_impl::<T::Transition<H>>()
}

#[inline(always)]
const fn compile_impl<T: Dfa>() -> &'static Tree {
  const {
    match start::<T>() {
      Some(0) | None => &Tree::Ok,
      Some(_) => &const {
        let a = if let Some(a) = start::<T>() { a } else { 0 };
        let mut lut = [&Tree::Nop; 16];
        inline! {
          (X0 0x0) (X1 0x1) (X2 0x2) (X3 0x3) (X4 0x4) (X5 0x5) (X6 0x6) (X7 0x7)
          (X8 0x8) (X9 0x9) (Xa 0xA) (Xb 0xB) (Xc 0xC) (Xd 0xD) (Xe 0xE) (Xf 0xF);
          ($T:tt $HEX:tt) if a & (1 << $HEX) != 0 { lut[$HEX] = compile_next::<T, $T>(); }
        }
        Tree::Lut(lut)
      },
    }
  }
}

pub const fn compile<T: Dfa>() -> &'static Tree {
  compile_impl::<T>()
}

#[cfg(test)]
mod tests {
  use super::*;

  type U8 = Any8;
  type NonZeroU8 = NonZero8;

  #[test]
  fn test_intersect() {
    // assert_eq!(Some(0xffff), start::<Any4>());
    // assert_eq!(Some(0), start::<()>());
    // assert_eq!(None, start::<End>());
    //
    // assert_eq!(Some(0b11), start::<Bool>());
    // assert_eq!(Some(0b11), start::<Intersection<Bool, Any4>>());
    //
    // assert_eq!(Some(0b11), start::<(Bool, Bool)>());
    // assert_eq!(Some(0b1), start::<<(Bool, Bool) as Dfa>::Transition<X0>>());
    //
    // assert_eq!(0b11, intersect::<U8, U8>());
    // assert_eq!(0b11, intersect::<Bool, Bool>());
    // assert_eq!(0b11, intersect::<Bool, U8>());
    // assert_eq!(0b01, intersect::<U8, Bool>());
    // assert_eq!(0b11, intersect::<Bool, U8>());
    // assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    // assert_eq!(0b11, intersect::<NonZeroU8, U8>());

    fn assert_subset<A: Dfa, B: Dfa>() where A: SubsetOf<B> {}

    assert_subset::<Any4, Any4>();
    assert_subset::<Any8, Any8>();
    assert_subset::<Bool, Bool>();
    assert_subset::<Bool, U8>();
    assert_subset::<NonZeroU8, U8>();

    // assert_eq!(0b11, intersect::<(U8, Bool), (U8, Bool)>());
    // assert_eq!(0b11, intersect::<(Bool, U8), (Bool, U8)>());
    // assert_eq!(0b11, intersect::<(U8, (U8, U8)), (U8, (U8, U8))>());
    // assert_eq!(0b11, intersect::<(U8, (Bool, NonZeroU8)), (U8, (Bool, NonZeroU8))>());
    // assert_eq!(0b01, intersect::<(U8, U8), (NonZeroU8, U8)>());
    // assert_eq!(0b01, intersect::<(U8, NonZeroU8), (Bool, NonZeroU8)>());
    // assert_eq!(0b01, intersect::<(U8, U8), (Bool, U8)>());
    // assert_eq!(0b01, intersect::<(U8, (U8, U8)), (U8, (Bool, U8))>());
    // assert_eq!(0b01, intersect::<(U8, U8), (Bool, NonZeroU8)>());

    type Bool8 = (Bool, (Bool, (Bool, (Bool, (Bool, (Bool, (Bool, Bool)))))));
    type Bool8x8 = (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, (Bool8, Bool8)))))));
    type Bool8x8x8 = (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, (Bool8x8, Bool8x8)))))));
    type Bool8x8x8x8 = (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, (Bool8x8x8, Bool8x8x8)))))));
    // type Bool8x8x8x8x8 = (Bool8x8x8x8, (Bool8x8x8x8, (Bool8x8x8x8, (Bool8x8x8x8, (Bool8x8x8x8, (Bool8x8x8x8, (Bool8x8x8x8, Bool8x8x8x8)))))));

    type U88 = (U8, (U8, (U8, (U8, (U8, (U8, (U8, U8)))))));
    type U88x8 = (U88, (U88, (U88, (U88, (U88, (U88, (U88, U88)))))));
    type U88x8x8 = (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, (U88x8, U88x8)))))));
    type U88x8x8x8 = (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, (U88x8x8, U88x8x8)))))));
    // type U88x8x8x8x8 = (U88x8x8x8, (U88x8x8x8, (U88x8x8x8, (U88x8x8x8, (U88x8x8x8, (U88x8x8x8, (U88x8x8x8, U88x8x8x8)))))));

    // assert_subset::<Bool8, Bool8>();
    // assert_subset::<Bool8x8, Bool8x8>();
    // assert_subset::<Bool8x8x8, Bool8x8x8>();

    // assert_eq!(0b11, intersect::<Bool8, Bool8>());
    // assert_eq!(0b11, intersect::<Bool8x8, Bool8x8>());
    // assert_eq!(0b11, intersect::<Bool8x8x8, Bool8x8x8>());
    // assert_eq!(0b11, intersect::<Bool8x8x8x8x8, U88x8x8x8x8>());
    // assert_eq!(0b01, intersect::<U88x8x8x8x8, Bool8x8x8x8x8>());
    // assert_eq!(0b11, intersect::<(Bool8x8x8x8x8, Bool), (Bool8x8x8x8x8, U8)>());
    // assert_eq!(0b01, intersect::<(U8, Bool8x8x8x8x8), (Bool, Bool8x8x8x8x8)>());

    // println!("{:?}", compile::<Bool8>().len());
    // println!("{:?}", compile::<Bool8x8>().len());
  }
}
