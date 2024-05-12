mod marker;

use core::fmt::{Display, Formatter};

pub trait Num {
  const USIZE: usize;
  type Inc: Num;
  type Dec: Num;
  type If0<Then, Else>;
  type If0N<Then: IntoNum, Else: IntoNum>: Num;
  type If0L<Then: Layout, Else: Layout>: Layout;
}

impl Num for () {
  const USIZE: usize = 0;
  type Inc = Number<Self>;
  type Dec = ();
  type If0<Then, Else> = Then;
  type If0N<Then: IntoNum, Else: IntoNum> = Then::N;
  type If0L<Then: Layout, Else: Layout> = Then;
}

pub struct Number<T>(T);

impl<T: Num> Num for Number<T> {
  const USIZE: usize = 1 + T::USIZE;
  type Inc = Number<Self>;
  type Dec = T;
  type If0<Then, Else> = Else;
  type If0N<Then: IntoNum, Else: IntoNum> = Else::N;
  type If0L<Then: Layout, Else: Layout> = Else;
}

pub type N0 = ();
pub type N1 = Number<N0>;
pub type N2 = Number<N1>;
pub type N3 = Number<N2>;
pub type N4 = Number<N3>;
pub type N5 = Number<N4>;
pub type N6 = Number<N5>;
pub type N7 = Number<N6>;
pub type N8 = Number<N7>;
pub type N9 = Number<N8>;

pub trait IntoNum {
  type N: Num;
}

impl<T: Num> IntoNum for T {
  type N = T;
}

pub struct _Sub<T, U>(T, U);
type Sub<T, U> = <_Sub<T, U> as IntoNum>::N;

impl<T: Num, U: Num> IntoNum for _Sub<T, U> {
  type N = U::If0N<T, _Sub<T::Dec, U::Dec>>;
}

pub struct _Add<T, U>(T, U);
type Add<T, U> = <_Add<T, U> as IntoNum>::N;

impl<T: Num, U: Num> IntoNum for _Add<T, U> {
  type N = U::If0N<T, _Add<T::Inc, U::Dec>>;
}

type IfEq<A, B, Then, Else> = <Sub<A, B> as Num>::If0<
  <Sub<B, A> as Num>::If0<Then, Else>,
  Else
>;

type IfLt<A, B, Then, Else> = <Sub<B, A> as Num>::If0<Else, Then>;
type IfGt<A, B, Then, Else> = <Sub<A, B> as Num>::If0<Else, Then>;

type IfLe<A, B, Then, Else> = IfLt<A, B, Then, IfEq<A, B, Then, Else>>;
type IfGe<A, B, Then, Else> = IfGt<A, B, Then, IfEq<A, B, Then, Else>>;

type IfLtN<A, B, Then, Else> = <Sub<B, A> as Num>::If0N<Else, Then>;
type IfGtN<A, B, Then, Else> = <Sub<A, B> as Num>::If0N<Else, Then>;

type Max<A, B> = IfGtN<A, B, A, B>;
type Min<A, B> = IfLtN<A, B, A, B>;

#[test]
fn test_num() {
  assert_eq!(3, Add::<N1, N2>::USIZE);
  assert_eq!(0, Sub::<N1, N2>::USIZE);

  type True = N1;
  type False = N0;

  assert_eq!(1, IfEq::<N1, N1, True, False>::USIZE);
  assert_eq!(0, IfLt::<N1, N1, True, False>::USIZE);
  assert_eq!(0, IfGt::<N1, N1, True, False>::USIZE);
  assert_eq!(1, IfLe::<N1, N1, True, False>::USIZE);
  assert_eq!(1, IfGe::<N1, N1, True, False>::USIZE);

  assert_eq!(0, IfEq::<N1, N2, True, False>::USIZE);
  assert_eq!(1, IfLt::<N1, N2, True, False>::USIZE);
  assert_eq!(0, IfGt::<N1, N2, True, False>::USIZE);
  assert_eq!(1, IfLe::<N1, N2, True, False>::USIZE);
  assert_eq!(0, IfGe::<N1, N2, True, False>::USIZE);
}

pub trait Layout {
  type Size: Num;
  type Skip<N: Num>;
  type Take<N: Num>;
}

trait Union<Rhs = Self> {
  type Output;
}

pub trait Intersects<Dst> {
  type Output;
}

// -------------------------------------------------------------------------------------------------
// Unreachable

pub struct Unreachable;

impl Display for Unreachable {
  fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
    write!(f, "Ø")
  }
}

impl<Rhs> Union<Rhs> for Unreachable {
  type Output = Rhs;
}

// -------------------------------------------------------------------------------------------------
// Uninit

/// Only intersects into `Uninit` of the sane amount of bytes is allowed:
/// ```rust
/// use mutant2::*;
/// let _: <Interval<N0, N1> as Intersects<Uninit>>::Output;
/// let _: <Uninit<N3> as Intersects<Uninit<N3>>>::Output;
/// ```
///
/// But not into `Uninit` of a different amount of bytes:
/// ```rust,compile_fail
/// use mutant2::*;
/// let _: <Uninit<N2> as Intersects<Uninit<N3>>>::Output;
/// ```
///
/// And not from `Uninit`:
/// ```rust,compile_fail
/// use mutant2::*;
/// let _: <Uninit as Intersects<Interval<N0, N1>>>::Output;
/// ```
pub struct Uninit<N: Num = N1>(N);

impl<U: Num> Layout for Uninit<U> {
  type Size = U;
  type Skip<N: Num> = Uninit<Sub<U, N>>;
  type Take<N: Num> = Uninit<Sub<U, Sub<U, N>>>;
}

impl<U: Num> Union for Uninit<U> {
  type Output = Uninit<U>;
}

impl<U: Num> Union<Unreachable> for Uninit<U> {
  type Output = Uninit<U>;
}

impl<U: Num> Intersects<Uninit<U>> for Uninit<U> {
  type Output = Uninit<U>;
}

impl<U: Num, S, E> Intersects<Uninit<U>> for Interval<S, E> {
  type Output = Uninit<U>;
}

// -------------------------------------------------------------------------------------------------
// Interval

pub struct Interval<S, E>(S, E);

impl<S: Num, E: Num> Display for Interval<S, E> {
  fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
    write!(f, "{}..={}", S::USIZE, E::USIZE)
  }
}

impl<S, E> Layout for Interval<S, E> {
  type Size = N1;
  type Skip<N: Num> = N::If0L<Self, Uninit<N1>>;
  type Take<N: Num> = N::If0L<Uninit<N1>, Self>;
}

impl<S, E> Union<Unreachable> for Interval<S, E> {
  type Output = Interval<S, E>;
}

impl<S0: Num, E0: Num, S1: Num, E1: Num> Union<Interval<S1, E1>> for Interval<S0, E0> {
  type Output = IfLt<E0, S1,
    Any<Self, Interval<S1, E1>>,
    IfLt<E1, S0,
      Any<Self, Interval<S1, E1>>,
      Interval<Min<S0, S1>, Max<E0, E1>>,
    >
  >;
}

impl<S0: Num, E0: Num, S1: Num, E1: Num> Intersects<Interval<S1, E1>> for Interval<S0, E0> {
  type Output = IfLt<E0, S1,
    Unreachable,
    IfLt<E1, S0,
      Unreachable,
      Interval<Max<S0, S1>, Min<E0, E1>>,
    >
  >;
}

// -------------------------------------------------------------------------------------------------
// Any

struct Any<L, R>(L, R);

impl<L: Layout, R: Layout<Size = L::Size>> Layout for Any<L, R> {
  type Size = L::Size;
  type Skip<N: Num> = Any<L::Skip<N>, R::Skip<N>>;
  type Take<N: Num> = Any<L::Take<N>, R::Take<N>>;
}

impl<A: Display, B: Display> Display for Any<A, B> {
  fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
    write!(f, "{} | {}", self.0, self.1)
  }
}

impl<L: Union<Rhs>, R: Union<<L as Union<Rhs>>::Output>, Rhs> Union<Rhs> for Any<L, R> {
  type Output = R::Output;
}

impl<L, R, Dst> Intersects<Dst> for Any<L, R>
where
  L: Intersects<Dst>,
  R: Intersects<Dst>,
  <L as Intersects<Dst>>::Output: Union<<R as Intersects<Dst>>::Output>,
{
  type Output = <<L as Intersects<Dst>>::Output as Union<<R as Intersects<Dst>>::Output>>::Output;
}

// -------------------------------------------------------------------------------------------------
// Cat

struct Cat<L, R>(L, R);

impl<L: Layout, R: Layout> Layout for Cat<L, R> {
  type Size = Add<L::Size, R::Size>;
  type Skip<N: Num> = N::If0<
    L,
    <Sub<N::Inc, L::Size> as Num>::If0<
      L::Skip<N>,
      <Sub<N, L::Size> as Num>::If0<
        R,
        R::Skip<Sub<N, L::Size>>
      >
    >
  >;
  type Take<N: Num> = N::If0<
    (),
    <Sub<N, L::Size> as Num>::If0<
      L::Take<N>,
      <Sub<N, Self::Size> as Num>::If0<
        Cat<L, R>,
        Cat<L, R::Take<Sub<N, L::Size>>>
      >
    >
  >;
}

impl<L: Layout, R: Layout, Rhs: Layout<Size = Add<L::Size, R::Size>>> Union<Rhs> for Cat<L, R>
where
  L: Union<Rhs::Take<L::Size>>,
  R: Union<Rhs::Skip<L::Size>>,
{
  type Output = Cat<
    <L as Union<Rhs::Take<L::Size>>>::Output,
    <R as Union<Rhs::Skip<L::Size>>>::Output,
  >;
}

impl<L: Layout, R: Layout, Dst: Layout<Size = Add<L::Size, R::Size>>> Intersects<Dst> for Cat<L, R>
where
  L: Intersects<Dst::Take<L::Size>>,
  R: Intersects<Dst::Skip<L::Size>>,
  Dst::Take<L::Size>: Union<Dst::Skip<L::Size>>,
{
  type Output = Cat<
    <L as Intersects<Dst::Take<L::Size>>>::Output,
    <R as Intersects<Dst::Skip<L::Size>>>::Output,
  >;
}

impl<A: Display, B: Display> Display for Cat<A, B> {
  fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
    write!(f, "{} + {}", self.0, self.1)
  }
}

#[test]
fn test_intersects_interval() {
  fn union<T: Union<U>, U>() -> &'static <T as Union<U>>::Output {
    unsafe { core::mem::transmute(&()) }
  }

  fn intersects<T: Intersects<U>, U>() -> &'static <T as Intersects<U>>::Output {
    unsafe { core::mem::transmute(&()) }
  }

  println!("{}", union::<Interval<N1, N6>, Unreachable>());
  println!("{}", union::<Unreachable, Interval<N1, N6>>());
  println!("{}", union::<Interval<N0, N8>, Interval<N1, N6>>());
  println!("{}", union::<Interval<N1, N2>, Interval<N4, N6>>());
  println!("{}", intersects::<Interval<N1, N2>, Interval<N4, N6>>());
  println!("{}", intersects::<Any<Interval<N4, N6>, Interval<N0, N1>>, Interval<N1, N2>>());
  println!("{}", intersects::<Interval<N1, N6>, Interval<N1, N6>>());
  println!("{}", intersects::<Interval<N1, N6>, Interval<N1, N5>>());
  println!("{}", intersects::<Interval<N1, N5>, Interval<N1, N6>>());
  println!("{}", intersects::<Interval<N2, N5>, Interval<N1, N5>>());
  println!("{}", intersects::<Interval<N1, N5>, Interval<N2, N5>>());
  println!("{}", intersects::<Interval<N1, N5>, Interval<N7, N7>>());
  println!("{}", intersects::<Interval<N6, N8>, Interval<N2, N2>>());

  println!("{}", intersects::<
    Cat<Any<Interval<N4, N6>, Interval<N0, N1>>, Interval<N1, N2>>,
    Cat<Interval<N1, N2>, Interval<N2, N3>>
  >());
}
