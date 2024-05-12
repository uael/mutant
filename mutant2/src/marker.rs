use core::marker::PhantomData;
use std::ffi::c_ushort;

/// The **marker trait** for compile time literals.
pub trait Lit<T> {
  const VALUE: T;
}

pub struct B0;
pub struct B1;

impl Lit<bool> for B0 {
  const VALUE: bool = false;
}

impl Lit<bool> for B1 {
  const VALUE: bool = true;
}

pub struct PhantomUint<U, B>(PhantomData<(U, B)>);

impl Lit<usize> for () {
  const VALUE: usize = 0;
}

impl<U: Lit<usize>, B: Lit<bool>> Lit<usize> for PhantomUint<U, B> {
  const VALUE: usize = (B::VALUE as usize) | U::VALUE << 1;
}

// ---------------------------------------------------------------------------------------
// Add operation

pub trait Add<T, Rhs>: Lit<T> {
  type Output: Lit<T>;
}

type Inc<T, A> = Sum<T, A, B1>;
type Sum<T, A, B> = <A as Add<T, B>>::Output;

// ---------------------------------------------------------------------------------------
// Adding bits to unsigned integers

/// `() + B0 = ()`
impl Add<usize, B0> for () {
  type Output = ();
}

/// `U + B0 = U`
impl<U: Lit<usize>, B: Lit<bool>> Add<usize, B0> for PhantomUint<U, B> {
  type Output = PhantomUint<U, B>;
}

/// `() + B1 = PhantomUint<(), B1>`
impl Add<usize, B1> for () {
  type Output = PhantomUint<(), B1>;
}

/// `PhantomUint<U, B0> + B1 = PhantomUint<U + B1>`
impl<U: Lit<usize>> Add<usize, B1> for PhantomUint<U, B0> {
  type Output = PhantomUint<U, B1>;
}

/// `PhantomUint<U, B1> + B1 = PhantomUint<U + B1, B0>`
impl<U: Add<usize, B1>> Add<usize, B1> for PhantomUint<U, B1> {
  type Output = PhantomUint<U::Output, B0>;
}

// ---------------------------------------------------------------------------------------
// Adding unsigned integers

/// `() + U = U`
impl<U: Lit<usize>> Add<usize, U> for () where Self: Lit<usize> {
  type Output = U;
}

/// `PhantomUint<U, B> + () = PhantomUint<U, B>`
impl<U: Lit<usize>, B: Lit<bool>> Add<usize, ()> for PhantomUint<U, B> {
  type Output = PhantomUint<U, B>;
}

/// `PhantomUint<Ul, B0> + PhantomUint<Ur, B0> = PhantomUint<Ul + Ur, B0>`
impl<Ul: Add<usize, Ur>, Ur: Lit<usize>> Add<usize, PhantomUint<Ur, B0>> for PhantomUint<Ul, B0> {
  type Output = PhantomUint<Ul::Output, B0>;
}

/// `PhantomUint<Ul, B0> + PhantomUint<Ur, B1> = PhantomUint<Ul + Ur, B1>`
impl<Ul: Add<usize, Ur>, Ur: Lit<usize>> Add<usize, PhantomUint<Ur, B1>> for PhantomUint<Ul, B0> {
  type Output = PhantomUint<Ul::Output, B1>;
}

/// `PhantomUint<Ul, B1> + PhantomUint<Ur, B0> = PhantomUint<Ul + Ur, B1>`
impl<Ul: Add<usize, Ur>, Ur: Lit<usize>> Add<usize, PhantomUint<Ur, B0>> for PhantomUint<Ul, B1> {
  type Output = PhantomUint<Ul::Output, B1>;
}

/// `PhantomUint<Ul, B1> + PhantomUint<Ur, B1> = PhantomUint<(Ul + Ur) + B1, B0>`
impl<Ul: Add<usize, Ur>, Ur: Lit<usize>> Add<usize, PhantomUint<Ur, B1>> for PhantomUint<Ul, B1>
where Ul::Output: Add<usize, B1>
{
  type Output = PhantomUint<Inc<usize, Ul::Output>, B0>;
}

// ---------------------------------------------------------------------------------------
// TODO

pub trait IntoLit<T> {
  type Output: Lit<T>;
}

impl<T, L: Lit<T>> IntoLit<T> for L {
  type Output = L;
}

pub struct PhantomEq<L, R>(PhantomData<(L, R)>);

pub type Eq<L, R> = <PhantomEq<L, R> as IntoLit<bool>>::Output;

impl IntoLit<bool> for PhantomEq<(), ()> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<B0, B0> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<B1, B1> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<B0, B1> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomEq<B1, B0> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomEq<B0, ()> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<(), B0> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<B1, PhantomUint<(), B1>> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomEq<PhantomUint<(), B1>, B1> {
  type Output = B1;
}

impl<U, B> IntoLit<bool> for PhantomEq<(), PhantomUint<U, B>> {
  type Output = B0;
}

impl<U, B> IntoLit<bool> for PhantomEq<PhantomUint<U, B>, ()> {
  type Output = B0;
}

impl<Ul, Ur> IntoLit<bool> for PhantomEq<PhantomUint<Ur, B0>, PhantomUint<Ul, B1>> {
  type Output = B0;
}

impl<Ul, Ur> IntoLit<bool> for PhantomEq<PhantomUint<Ur, B1>, PhantomUint<Ul, B0>> {
  type Output = B0;
}

impl<Ul, Ur> IntoLit<bool> for PhantomEq<PhantomUint<Ur, B0>, PhantomUint<Ul, B0>> where PhantomEq<Ul, Ur>: IntoLit<bool> {
  type Output = <PhantomEq<Ul, Ur> as IntoLit<bool>>::Output;
}

impl<Ul, Ur> IntoLit<bool> for PhantomEq<PhantomUint<Ur, B1>, PhantomUint<Ul, B1>> where PhantomEq<Ul, Ur>: IntoLit<bool> {
  type Output = <PhantomEq<Ul, Ur> as IntoLit<bool>>::Output;
}

pub struct PhantomAnd<L, R>(PhantomData<(L, R)>);

pub type And<L, R> = <PhantomAnd<<L as IntoLit<bool>>::Output, <R as IntoLit<bool>>::Output> as Op>::Output;

impl IntoLit<bool> for PhantomAnd<B0, B0> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomAnd<B0, B1> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomAnd<B1, B0> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomAnd<B1, B1> {
  type Output = B1;
}

pub struct PhantomOr<L, R>(PhantomData<(L, R)>);

pub type Or<L, R> = <PhantomOr<<L as IntoLit<bool>>::Output, <R as IntoLit<bool>>::Output> as Op>::Output;

impl IntoLit<bool> for PhantomOr<B0, B0> {
  type Output = B0;
}

impl IntoLit<bool> for PhantomOr<B0, B1> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomOr<B1, B0> {
  type Output = B1;
}

impl IntoLit<bool> for PhantomOr<B1, B1> {
  type Output = B1;
}

pub trait Op {
  type Output;
}

impl<C: IntoLit<bool>> Op for C {
  type Output = C::Output;
}

struct PhantomIf<C, Then, Else>(PhantomData<(C, Then, Else)>);

impl<Then, Else> Op for PhantomIf<B0, Then, Else> {
  type Output = Else;
}

impl<Then, Else> Op for PhantomIf<B1, Then, Else> {
  type Output = Then;
}

type If<C, Then, Else> = <PhantomIf<<C as IntoLit<bool>>::Output, Then, Else> as Op>::Output;

macro_rules! tyx {
  (if $c:tt $l:tt else $r:tt) => { If<tyx!($c), tyx!($l), tyx!($r)> };
  ($l:tt == $r:tt) => { PhantomEq<tyx!($l), tyx!($r)> };
  ($l:tt && $r:tt) => { And<tyx!($l), tyx!($r)> };
  ($l:tt || $r:tt) => { Or<tyx!($l), tyx!($r)> };
  ($l:tt + $r:tt) => { Sum<usize, tyx!($l), tyx!($r)> };
  (++$l:tt) => { Inc<usize, tyx!($l)> };
  ({ $($tt:tt)* }) => { tyx!($($tt)*) };
  ($($tt:tt)+) => { $($tt)+ };
}

#[test]
fn test_literal() {
  type U0 = ();
  type U1 = PhantomUint<(), B1>;

  let _: If<PhantomEq<(), ()>, (), u8> = ();
  let _: If<PhantomEq<(), PhantomUint<(), B1>>, (), u8> = 1;
  let _: If<PhantomEq<Inc<usize, ()>, PhantomUint<(), B1>>, (), u8> = ();
  let _: If<And<<PhantomEq<Inc<usize, ()>, PhantomUint<(), B1>> as IntoLit<bool>>::Output, B1>, (), u8> = ();
  let _: If<And<<PhantomEq<Inc<usize, ()>, PhantomUint<(), B1>> as IntoLit<bool>>::Output, B0>, (), u8> = 1;
  let _: tyx!(if {{U0 + U1} == U1} () else u8) = ();
  let _: tyx!(if {{{U0 + U1} == U1} && B1} () else u8) = ();
  let _: tyx!(if {{{U0 + U1} == U1} && B0} () else u8) = 1;
}