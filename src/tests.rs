// use crate::internal::*;
// use crate::num::*;
// use crate::*;
// use core::num::{NonZero<u128>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u8>};

use crate::num::LiteralUint;
use crate::*;
use core::mem::ManuallyDrop;

#[derive(Debug, Copy, Clone, Type)]
#[mutant(crate)]
struct TestDerive {
  a: bool,
  b: u8,
  c: NonZero<u16>,
}

#[repr(u32)]
#[allow(unused)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum TestEnum {
  A = 0x01010101,
  B = 0x02020202,
  C = 0x03030303,
}

impl layout::Raw for TestEnum {
  type Repr = u32;
  type Satisfy = layout::Or<
    layout::Equ<u32, LiteralUint<u32, 0x01010101>>,
    layout::Or<
      layout::Equ<u32, LiteralUint<u32, 0x02020202>>,
      layout::Equ<u32, LiteralUint<u32, 0x03030303>>,
    >,
  >;
  const RAW_DEFAULT: Self = Self::A;
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo1u8 {
  _0 = 0,
  _1 = 1,
}

impl layout::Raw for UpTo1u8 {
  type Repr = u8;
  type Satisfy =
    layout::Or<layout::Equ<u8, LiteralUint<u8, 0>>, layout::Equ<u8, LiteralUint<u8, 1>>>;
  const RAW_DEFAULT: Self = Self::_0;
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo1u16 {
  _0 = 0,
  _1 = 1,
}

impl layout::Raw for UpTo1u16 {
  type Repr = u16;
  type Satisfy =
    layout::Or<layout::Equ<u16, LiteralUint<u16, 0>>, layout::Equ<u16, LiteralUint<u16, 1>>>;
  const RAW_DEFAULT: Self = Self::_0;
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo2u8 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

impl layout::Raw for UpTo2u8 {
  type Repr = u8;
  type Satisfy = layout::Or<
    layout::Equ<u8, LiteralUint<u8, 0>>,
    layout::Or<layout::Equ<u8, LiteralUint<u8, 1>>, layout::Equ<u8, LiteralUint<u8, 2>>>,
  >;
  const RAW_DEFAULT: Self = Self::_0;
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo2u16 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

impl layout::Raw for UpTo2u16 {
  type Repr = u16;
  type Satisfy = layout::Or<
    layout::Equ<u16, LiteralUint<u16, 0>>,
    layout::Or<layout::Equ<u16, LiteralUint<u16, 1>>, layout::Equ<u16, LiteralUint<u16, 2>>>,
  >;
  const RAW_DEFAULT: Self = Self::_0;
}

#[repr(C)]
#[derive(Type, Debug, PartialEq)]
#[mutant(crate)]
struct TailSliceDst {
  info: u32,
  info_1: u32,
  info_2: u32,
  info_3: u32,
  data: [u8], // `tail slice` of type `[u8]`.
}

#[repr(C)]
#[derive(Type, Debug, PartialEq)]
#[mutant(crate)]
struct TailDst {
  version: u32,
  payload: TailSliceDst, // `tail` DST of type `TailSliceDst`.
}

#[repr(C)]
#[derive(Type, Debug, PartialEq)]
#[mutant(crate)]
struct MaybeDst<T: ?Sized> {
  info: u32,
  data: T, // `tail` of type `T`.
}

#[derive(Type, Copy, Clone, Debug, PartialEq)]
#[mutant(crate)]
struct NotDstRust {
  info: u32,
  data: [u32; 21],
}

// #[test]
// fn test_internal_convert_metadata() {
//   assert_eq!(convert_metadata::<u8, u8>(0), 0);
//   assert_eq!(convert_metadata::<u8, [u8]>(0), 1);
//   assert_eq!(convert_metadata::<u32, [u8]>(0), 4);
//   assert_eq!(convert_metadata::<u32, [u32]>(0), 1);
//   assert_eq!(convert_metadata::<u64, [u32]>(0), 2);
//   assert_eq!(convert_metadata::<[u8; 21], TailSliceDst>(0), 21 - 4);
//   assert_eq!(convert_metadata::<[u8; 21], TailDst>(0), 21 - 4 - 4);
//   assert_eq!(convert_metadata::<[u8; 21], MaybeDst<[u8]>>(0), 21 - 4);
//   assert_eq!(convert_metadata::<[u8; 21], MaybeDst<u32>>(0), 0);
//   assert_eq!(convert_metadata::<[u32; 42], NotDstRust>(0), 0);
// }
//
// #[test]
// fn test_internal_try_convert_metadata() {
//   // Same as `test_internal_convert_metadata`:
//   assert_eq!(try_convert_metadata::<u8, u8>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<u8, [u8]>(0), Ok(1));
//   assert_eq!(try_convert_metadata::<u32, [u8]>(0), Ok(4));
//   assert_eq!(try_convert_metadata::<[u8; 21], TailSliceDst>(0), Ok(21 - 4));
//   assert_eq!(try_convert_metadata::<[u8; 21], TailDst>(0), Ok(21 - 4 - 4));
//   assert_eq!(try_convert_metadata::<[u8; 21], MaybeDst<[u8]>>(0), Ok(21 - 4));
//   assert_eq!(try_convert_metadata::<[u8; 21], MaybeDst<u32>>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<[u32; 42], NotDstRust>(0), Ok(0));
//   // New ones:
//   assert_eq!(try_convert_metadata::<[u8], u8>(0), Err(1));
//   assert_eq!(try_convert_metadata::<[u8], u8>(1), Ok(0));
//   assert_eq!(try_convert_metadata::<[u8], u8>(2), Err(-1));
//   assert_eq!(try_convert_metadata::<[u8], u32>(2), Err(2));
//   assert_eq!(try_convert_metadata::<[u8], u32>(4), Ok(0));
//   assert_eq!(try_convert_metadata::<[u8], u32>(6), Err(-2));
//   assert_eq!(try_convert_metadata::<[u8], [u8]>(5), Ok(5));
//   assert_eq!(try_convert_metadata::<[u8], [u32]>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<[u8], [u32]>(4), Ok(1));
//   assert_eq!(try_convert_metadata::<[u8], [u32]>(8), Ok(2));
//   assert_eq!(try_convert_metadata::<[u8], [u32]>(1), Err(3));
//   assert_eq!(try_convert_metadata::<[u8], [u32]>(5), Err(3));
//   assert_eq!(try_convert_metadata::<[u32], [u8]>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<[u32], [u8]>(1), Ok(4));
//   assert_eq!(try_convert_metadata::<[u32], [u8]>(2), Ok(8));
//   assert_eq!(try_convert_metadata::<[u32], [u16]>(1), Ok(2));
//   assert_eq!(try_convert_metadata::<[u32], [u16]>(2), Ok(4));
//   assert_eq!(try_convert_metadata::<[u32], [u64]>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<[u32], [u64]>(1), Err(4));
//   assert_eq!(try_convert_metadata::<TailSliceDst, u32>(0), Ok(0));
//   assert_eq!(try_convert_metadata::<TailSliceDst, u32>(4), Err(-4));
//   assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 20), Ok(0));
//   assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 19), Err(4));
//   assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 21), Err(-4));
// }
//
// #[test]
// fn test_internal_tail_offset() {
//   assert_eq!(tail_offset::<u8>(), 0);
//   assert_eq!(tail_offset::<[u8]>(), 0);
//   assert_eq!(tail_offset::<[u8; 1]>(), 0);
//   assert_eq!(tail_offset::<TailSliceDst>(), 4);
//   assert_eq!(tail_offset::<TailDst>(), 4);
//   assert_eq!(tail_offset::<MaybeDst<[u8]>>(), 4);
//   assert_eq!(tail_offset::<MaybeDst<u32>>(), 4);
//   assert_eq!(tail_offset::<NotDstRust>(), 4);
// }
//
// #[test]
// fn test_internal_tail_item_size() {
//   assert_eq!(tail_item_size::<u8>(), 0);
//   assert_eq!(tail_item_size::<[u8]>(), 0);
//   assert_eq!(tail_item_size::<[u8; 1]>(), 0);
//   assert_eq!(tail_item_size::<TailSliceDst>(), 1);
//   assert_eq!(tail_item_size::<TailDst>(), 0);
//   assert_eq!(tail_item_size::<MaybeDst<[u8]>>(), 1);
//   assert_eq!(tail_item_size::<MaybeDst<[u32]>>(), 4);
//   assert_eq!(tail_item_size::<MaybeDst<[u8; 21]>>(), 1);
//   assert_eq!(tail_item_size::<MaybeDst<[u32; 21]>>(), 4);
//   assert_eq!(tail_item_size::<NotDstRust>(), 4);
// }
//
// #[test]
// fn test_internal_validate() {
//   // TODO
// }

#[test]
fn a() {
  println!("{:?}", internal::transmutation::<Pack2<u8, [u8]>, Pack2<u8, [bool]>>());
}

macro_rules! decl_pack {
  ($($name:ident ($($t:ident)* =$tt:ident))+) => {$(
    #[doc(hidden)]
    #[repr(C, packed)]
    #[derive(Type, Copy, Clone)]
    #[mutant(crate)]
    pub struct $name<$($t,)* $tt: Type + ?Sized>($(pub $t,)* pub core::mem::ManuallyDrop<$tt>);
  )+};
}

decl_pack! {
  Pack1 (=A)
  Pack2 (A =B)
  Pack3 (A B =C)
  Pack4 (A B C =D)
  Pack5 (A B C D =E)
  Pack6 (A B C D E =F)
  Pack7 (A B C D E F =G)
  Pack8 (A B C D E F G =H)
  Pack9 (A B C D E F G H =I)
}

unsafe impl<T: ptr::Pointee + ?Sized> ptr::Pointee for ManuallyDrop<T> {
  type Metadata = T::Metadata;
}

impl<T: Type + ?Sized> Type for ManuallyDrop<T>
where
  Self: MinimallySized<T::Sizing>,
{
  type Sizing = T::Sizing;
  type Layout = T::Layout;
}

impl<T: Type + ?Sized> MinimallySized<sizing::Unsized> for ManuallyDrop<T> {
  const MINSIZE: usize = T::MINSIZE;
}

#[test]
#[allow(unused_parens)]
fn test_internal_static_analyze() {
  use internal::transmutation;
  use internal::MetadataChecks::*;
  use internal::Transmutation;

  macro_rules! test {
    (@ ($a:tt)) => { test!(@$a) };
    (@ [$a:tt; $n:expr]) => { [test!(@$a); $n] };
    (@ [$a:tt]) => { [test!(@$a)] };
    (@ ($a:tt, $b:tt)) => { Pack2<test!(@$a), test!(@$b)> };
    (@ ($a:tt, $b:tt, $c:tt)) => { Pack3<test!(@$a), test!(@$b), test!(@$c)> };
    (@ ($a:tt, $b:tt, $c:tt, $d:tt)) => { Pack4<test!(@$a), test!(@$b), test!(@$c), test!(@$d)> };
    (@ ($a:tt, $b:tt, $c:tt, $d:tt, $e:tt)) => { Pack5<test!(@$a), test!(@$b), test!(@$c), test!(@$d), test!(@$e)> };
    (@ _) => { Uninit };
    (@ $a:tt) => { $a };
    ($src:tt $dst:tt => $res:literal) => (
      assert_eq!(transmutation::<test!(@$src), test!(@$dst)>(), Err($res))
    );
    ($src:tt $dst:tt => $res:ident) => (
      assert_eq!(transmutation::<test!(@$src), test!(@$dst)>(), Ok(Transmutation::$res(None)))
    );
    ($src:tt $dst:tt => $res:ident ($check:expr)) => (
      assert_eq!(transmutation::<test!(@$src), test!(@$dst)>(), Ok(Transmutation::$res(Some($check))))
    );
  }

  test!(u8 u8 => infallible_mutable);
  test!((u8, u8) (u8, u8) => infallible_mutable);
  test!((u8, [u8]) (u8, [u8]) => infallible_mutable);

  test!(u8 u8 => infallible_mutable);
  test!(bool bool => infallible_mutable);
  test!(char char => infallible_mutable);
  test!(u8 bool => fallible_mutable);
  test!(bool u8 => infallible);

  test!((bool, _, _, bool) (u8, _, _, u8) => infallible);

  test!(u32 [u8] => infallible_mutable);
  test!(u32 (u8, [u8]) => infallible_mutable);
  test!(u32 (u32, [u8]) => infallible_mutable);

  test!([u8] u8 => infallible_mutable(Is(1)));
  test!([u8] u16 => infallible_mutable(Is(2)));

  test!((u8, [u8]) u8 => infallible_mutable(IsEmpty));
  test!((u16, [u8]) u16 => infallible_mutable(IsEmpty));

  test!([u8] [bool] => fallible_mutable);
  test!((bool) [bool] => infallible_mutable);
  test!((bool, bool) [bool] => infallible_mutable);
  test!((bool, bool, bool) [bool] => infallible_mutable);
  test!((bool, bool, bool, bool) [bool] => infallible_mutable);
  test!((bool, bool, bool, bool) [bool; 4] => infallible_mutable);

  test!(u32 u8 => infallible_mutable);
  test!(u8 u32 => "source too small to fit target");

  test!(char char => infallible_mutable);

  test!(UpTo1u8 UpTo2u8 => infallible);
  test!(UpTo2u8 UpTo1u8 => fallible_mutable);

  // Test with UpTo2u8 type
  test!(UpTo2u8 UpTo2u8 => infallible_mutable);
  test!(UpTo2u8 u8 => infallible);
  test!(u8 UpTo2u8 => fallible_mutable);
  test!(UpTo2u8 u16 => "source too small to fit target");
  test!(u16 UpTo2u8 => fallible_mutable);

  // Test with TestEnum type
  test!(TestEnum TestEnum => infallible_mutable);
  test!(TestEnum u32 => infallible);
  test!(TestEnum u16 => infallible);
  test!(TestEnum u8 => infallible);
  test!(u32 TestEnum => fallible_mutable);
  test!(TestEnum u64 => "source too small to fit target");
  test!(u64 TestEnum => fallible_mutable);

  test!((LiteralUint<u8, 1>) (NonZero<u8>) => infallible);
  test!((LiteralUint<u16, 1>) (NonZero<u16>) => infallible);
  test!((LiteralUint<u32, 1>) (NonZero<u32>) => infallible);
  test!((LiteralUint<u64, 1>) (NonZero<u64>) => infallible);
  test!((LiteralUint<u128, 1>) (NonZero<u128>) => infallible);

  #[derive(Type)]
  #[repr(C, packed)]
  #[mutant(crate)]
  struct T<const HEAD: usize, const TAIL: usize = 0>([u8; HEAD], [[u8; TAIL]]);

  // Basic tests for equal head and tail sizes
  test!((T<1>) (T<1>) => infallible_mutable);
  test!((T<16>) (T<16>) => infallible_mutable);
  test!((T<42, 1>) (T<42, 1>) => infallible_mutable);

  // Test for equal head sizes with different tail sizes
  test!((T<42, 3>) (T<42, 1>) => infallible_mutable);
  test!((T<50, 10>) (T<50, 100>) => infallible_mutable(IsMultipleOf(10)));
  test!((T<50, 5>) (T<50, 100>) => infallible_mutable(IsMultipleOf(20)));

  // Test cases for transmuting to and from empty tail
  test!((T<42, 1>) (T<42>) => infallible_mutable(IsEmpty));
  test!((T<35, 1>) (T<42>) => infallible_mutable(Is(7)));
  test!((T<2>) (T<1>) => infallible_mutable);

  // Test cases for large difference in head sizes
  test!((T<20, 1>) (T<100>) => infallible_mutable(Is(80)));

  // Test cases for tail sizes as multiples
  test!((T<42, 1>) (T<42, 2>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<42, 1>) (T<42, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<42, 2>) (T<42, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<42, 4>) (T<42, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<42, 5>) (T<42, 8>) => infallible_mutable(IsMultipleOf(8)));
  test!((T<42, 2>) (T<42, 6>) => infallible_mutable(IsMultipleOf(3)));

  // Test zero heads
  test!((T<0, 1>) (T<42>) => infallible_mutable(Is(42)));
  test!((T<0, 1>) (T<42, 1>) => infallible_mutable(IsAtLeast(42)));

  // Test cases for alignment and complex transmutations
  test!((T<4, 1>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 3)));
  test!((T<4, 2>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<4, 2>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<4, 2>) (T<0, 4>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<42, 1>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 3)));
  test!((T<42, 2>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<43, 2>) (T<39, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<43, 2>) (T<39, 4>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<1, 2>) (T<5, 2>) => infallible_mutable(IsAtLeast(2)));
  test!((T<45, 2>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<40, 1>) (T<38, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<44, 5>) (T<38, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<38, 3>) (T<42, 1>) => infallible_mutable(IsAtLeast(2)));
  test!((T<38, 3>) (T<41, 3>) => infallible_mutable(IsAtLeast(1)));
  test!((T<1, 2>) (T<1, 4>) => infallible_mutable(IsMultipleOf(2)));

  test!((T<4, 1>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 3)));
  test!((T<4, 2>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<4, 2>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<4, 2>) (T<0, 4>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<1, 2>) (T<5, 2>) => infallible_mutable(IsAtLeast(2)));
  test!((T<7, 2>) (T<0, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<8, 2>) (T<1, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<2, 1>) (T<0, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<6, 5>) (T<0, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<0, 3>) (T<4, 1>) => infallible_mutable(IsAtLeast(2)));
  test!((T<0, 3>) (T<3, 3>) => infallible_mutable(IsAtLeast(1)));

  // Test cases for incompatible transmutations
  test!((T<1>) (T<2>) => "source too small to fit target");
  test!((T<35, 2>) (T<42>) => "missing bytes to fit the target is not a multiple of the source DST trailing slice item size");
  test!((T<40, 3>) (T<42>) => "missing bytes to fit the target is not a multiple of the source DST trailing slice item size");
  test!((T<42>) (T<38, 3>) => "remaining bytes to fit in the target is not a multiple of the target DST trailing slice item size");
  test!((T<42, 3>) (T<38, 3>) => "DST source cannot catch up on DST target");
  test!((T<43, 3>) (T<39, 3>) => "DST source cannot catch up on DST target");
  test!((T<4, 3>) (T<0, 6>) => "DST source cannot catch up on DST target");
  test!((T<5, 3>) (T<1, 6>) => "DST source cannot catch up on DST target");
  test!((T<43, 3>) (T<39, 6>) => "DST source cannot catch up on DST target");
  test!((T<41, 3>) (T<39, 3>) => "DST source cannot catch up on DST target");
  test!((T<38, 3>) (T<42, 3>) => "DST source cannot catch up on DST target");
  test!((T<39, 3>) (T<41, 3>) => "DST source cannot catch up on DST target");
  test!((T<39, 3>) (T<41, 6>) => "DST source cannot catch up on DST target");
  test!((T<0, 6>) (T<2, 3>) => "DST source cannot catch up on DST target");
  test!((T<1, 6>) (T<3, 3>) => "DST source cannot catch up on DST target");
  test!((T<39, 6>) (T<41, 3>) => "DST source cannot catch up on DST target");
  test!((T<40, 6>) (T<42, 3>) => "DST source cannot catch up on DST target");
  test!((T<39, 6>) (T<41, 9>) => "DST source cannot catch up on DST target");
  test!((T<39, 3>) (T<41, 9>) => "DST source cannot catch up on DST target");
  test!((T<38, 6>) (T<42, 3>) => "DST source cannot catch up on DST target");
  test!((T<39, 9>) (T<41, 3>) => "DST source cannot catch up on DST target");
  test!((T<2, 2>) (T<5, 2>) => "DST source cannot catch up on DST target");
  test!((T<3, 2>) (T<6, 2>) => "DST source cannot catch up on DST target");
  test!((T<4, 2>) (T<7, 2>) => "DST source cannot catch up on DST target");

  // Test cases for transmuting to and from empty head
  test!((T<0, 1>) (T<0, 1>) => infallible_mutable);
  test!((T<0, 1>) (T<0>) => infallible_mutable(IsEmpty));
  test!((T<0, 1>) (T<0, 2>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<0, 1>) (T<0, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<0, 1>) (T<0, 4>) => infallible_mutable(IsMultipleOf(4)));
  test!((T<0, 3>) (T<0, 5>) => infallible_mutable(IsMultipleOf(5)));
  test!((T<0, 2>) (T<0, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<0, 2>) (T<0, 4>) => infallible_mutable(IsMultipleOf(2)));

  // Test cases for transmuting to and from empty head and tail
  test!((T<0, 0>) (T<0, 0>) => infallible_mutable);

  //   // Test cases for transmuting to and from empty padded types
  //   test!(_ u8 => "uninitialized byte in source");
  //   test!((_, _, u16) u32 => "uninitialized byte in source");
  //   test!(((u8, _), u16) u32 => "uninitialized byte in source");
  test!(((u8, _), (u8, _)) ((bool, _), (bool, _)) => fallible_mutable);
  //   test!((u32, [(u8, _)]) (u32, [u16]) => "uninitialized byte in source");
  test!((u32, [(u8, _)]) (u32, [(bool, _)]) => fallible_mutable);
  //   test!((u16, [(u8, _)]) (u32, [u16]) => "uninitialized byte in source");
  test!(((u8, u8), u16) ((u8, bool), u16) => fallible_mutable);

  // Test for unreachable transmutations
  test!((LiteralUint<u8, 2>) bool => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u8, 0>) u8 => infallible);
  test!((LiteralUint<u8, 0>) (NonZero<u8>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u16, 0>) u16 => infallible);
  test!((LiteralUint<u16, 0>) (NonZero<u16>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u32, 0>) u32 => infallible);
  test!((LiteralUint<u32, 0>) (NonZero<u32>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u64, 0>) u64 => infallible);
  test!((LiteralUint<u64, 0>) (NonZero<u64>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u128, 0>) u128 => infallible);
  test!((LiteralUint<u128, 0>) (NonZero<u128>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u128, 0>) (Option<NonZero<u128>>) => infallible);
  test!(TestEnum (NonZero<u32>) => infallible);
  test!((NonZero<u32>) TestEnum => fallible_mutable);
  test!(TestEnum (LiteralUint<u32, 0>) => "none of the bit patterns in source are valid in target");
  test!((LiteralUint<u32, 0>) TestEnum => "none of the bit patterns in source are valid in target");

  test!([u8; 16] (TestEnum, (LiteralUint<num::le16, 0xcafe>), [((LiteralUint<u8, 0x42>), TestEnum); 2]) => fallible_mutable);
}

// #[test]
// fn test_size_field() {
//   type T0 = Pack!(Size<u32>, ());
//   type T1 = Pack!(u64, Size<u32>, ());
//   type T2 = Pack!([u8; 0xcafebeef], Size<u32>, ());
//   type T3 = Pack!([u8; 0xcafebeef], Size<u32be>, ());
//   type T4 = Pack!([u8; 0xcafebeef], Size<u32>, [u8]);
//   type T5 = Pack!([u8; 0xcafebeef], Size<u32be>, [u8]);
//
//   type SizeType<T> = <<<T as Data>::TailField as Field>::PrevField as Field>::Type;
//
//   pub const fn pat_of<T: ?Sized + Data>() -> BytesPattern<'static> {
//     match <SizeType<T> as Data>::LAYOUT {
//       Layout::Integer { spec } => spec.bytes_pattern,
//       _ => unreachable!(),
//     }
//   }
//
//   pub const fn default_of<T: ?Sized + Data>() -> u32 {
//     match <SizeType<T> as Data>::LAYOUT {
//       Layout::Integer { spec } => cast(spec.default()),
//       _ => unreachable!(),
//     }
//   }
//
//   assert_eq!(default_of::<T0>(), 4);
//   assert!(!pat_of::<T0>().matches(3u32));
//   assert!(pat_of::<T0>().matches(4u32));
//   assert!(!pat_of::<T0>().matches(5u32));
//
//   assert_eq!(default_of::<T1>(), 12);
//   assert!(!pat_of::<T1>().matches(11u32));
//   assert!(pat_of::<T1>().matches(12u32));
//   assert!(!pat_of::<T1>().matches(13u32));
//
//   assert_eq!(default_of::<T2>(), 0xcafebeefu32 + 4);
//   assert!(!pat_of::<T2>().matches(0xcafebeefu32 + 4 - 1));
//   assert!(pat_of::<T2>().matches(0xcafebeefu32 + 4));
//   assert!(!pat_of::<T2>().matches(0xcafebeefu32 + 4 + 1));
//
//   assert_eq!(default_of::<T3>(), 0xcafebeefu32 + 4);
//   assert!(!pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4 - 1)));
//   assert!(pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4)));
//   assert!(!pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4 + 1)));
//
//   assert_eq!(default_of::<T4>(), 0xcafebeefu32 + 4);
//   assert!(!pat_of::<T4>().matches(0xcafebeefu32 + 4 - 1));
//   assert!(pat_of::<T4>().matches(0xcafebeefu32 + 4));
//   assert!(pat_of::<T4>().matches(0xcafebeefu32 + 4 + 1));
//   assert!(pat_of::<T4>().matches(u32::MAX));
//
//   assert_eq!(default_of::<T5>(), 0xcafebeefu32 + 4);
//   assert!(!pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4 - 1)));
//   assert!(pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4)));
//   assert!(pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4 + 1)));
//   assert!(pat_of::<T4>().matches(u32be::MAX));
// }

#[test]
fn test_num() {
  use num::*;
  use core::array::from_fn;
  use rand::Rng;

  #[rustfmt::skip]
  macro_rules! test {
    ($sz:literal, [$($ty:ident),+], $min:literal, $max:literal) => {$(
      let val = rand::thread_rng().gen_range($min..=$max);
      assert_eq!($sz, size_of::<$ty>());
      assert_eq!($max, $ty::MAX.into());
      assert_eq!($min, $ty::from($min).into());
      assert_eq!($max, $ty::from($max).into());
      assert_eq!(val, $ty::from(val).into());
      assert_eq!(from_fn(|i| i as u8), $ty::from_ne_bytes(from_fn(|i| i as u8)).to_ne_bytes());
      assert_eq!(from_fn(|i| i as u8), $ty::from_le_bytes(from_fn(|i| i as u8)).to_le_bytes());
      assert_eq!(from_fn(|i| i as u8), $ty::from_be_bytes(from_fn(|i| i as u8)).to_be_bytes());
    )+};
  }

  test!(2, [u16, u16le, u16be], 0u16, 0xffffu16);
  test!(3, [u24, u24le, u24be], 0u32, 0xffffffu32);
  test!(4, [u32, u32le, u32be], 0u32, 0xffffffffu32);
  test!(5, [u40, u40le, u40be], 0u64, 0xffffffffffu64);
  test!(6, [u48, u48le, u48be], 0u64, 0xffffffffffffu64);
  test!(7, [u56, u56le, u56be], 0u64, 0xffffffffffffffu64);
  test!(8, [u64, u64le, u64be], 0u64, 0xffffffffffffffffu64);
  test!(9, [u72, u72le, u72be], 0u128, 0xffffffffffffffffffu128);
  test!(10, [u80, u80le, u80be], 0u128, 0xffffffffffffffffffffu128);
  test!(11, [u88, u88le, u88be], 0u128, 0xffffffffffffffffffffffu128);
  test!(12, [u96, u96le, u96be], 0u128, 0xffffffffffffffffffffffffu128);
  test!(13, [u104, u104le, u104be], 0u128, 0xffffffffffffffffffffffffffu128);
  test!(14, [u112, u112le, u112be], 0u128, 0xffffffffffffffffffffffffffffu128);
  test!(15, [u120, u120le, u120be], 0u128, 0xffffffffffffffffffffffffffffffu128);
  test!(16, [u128, u128le, u128be], 0u128, 0xffffffffffffffffffffffffffffffffu128);

  let big: u128 = 0x112233445566778899aabbccddeeff;
  assert_eq!(0xffu64, cast::<_, u8>(big).into());
  assert_eq!(0xeeffu64, cast::<_, u16>(big).into());
  assert_eq!(0xeeffu64, cast::<_, u16le>(big).into());
  assert_eq!(0xeeffu64, cast::<_, u16be>(big).into());
  assert_eq!(0xddeeffu64, cast::<_, u24>(big).into());
  assert_eq!(0xddeeffu64, cast::<_, u24le>(big).into());
  assert_eq!(0xddeeffu64, cast::<_, u24be>(big).into());
  assert_eq!(0xccddeeffu64, cast::<_, u32>(big).into());
  assert_eq!(0xccddeeffu64, cast::<_, u32le>(big).into());
  assert_eq!(0xccddeeffu64, cast::<_, u32be>(big).into());
  assert_eq!(0xbbccddeeffu64, cast::<_, u40>(big).into());
  assert_eq!(0xbbccddeeffu64, cast::<_, u40le>(big).into());
  assert_eq!(0xbbccddeeffu64, cast::<_, u40be>(big).into());
  assert_eq!(0xaabbccddeeffu64, cast::<_, u48>(big).into());
  assert_eq!(0xaabbccddeeffu64, cast::<_, u48le>(big).into());
  assert_eq!(0xaabbccddeeffu64, cast::<_, u48be>(big).into());
  assert_eq!(0x99aabbccddeeffu64, cast::<_, u56>(big).into());
  assert_eq!(0x99aabbccddeeffu64, cast::<_, u56le>(big).into());
  assert_eq!(0x99aabbccddeeffu64, cast::<_, u56be>(big).into());
  assert_eq!(0x8899aabbccddeeffu64, cast::<_, u64>(big).into());
  assert_eq!(0x8899aabbccddeeffu64, cast::<_, u64le>(big).into());
  assert_eq!(0x8899aabbccddeeffu64, cast::<_, u64be>(big).into());

  assert_eq!(0xffu128, cast::<u8, u128>(0xff));
  assert_eq!(0xffu128, cast::<u8, u128le>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u128be>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u64>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u64le>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u64be>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u32>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u32le>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u32be>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u16>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u16le>(0xff).into());
  assert_eq!(0xffu128, cast::<u8, u16be>(0xff).into());

  assert_eq!(0xeeffu128, cast::<u16, u128>(0xeeff));
  assert_eq!(0xeeffu128, cast::<u16, u128le>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u128be>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u64>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u64le>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u64be>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u32>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u32le>(0xeeff).into());
  assert_eq!(0xeeffu128, cast::<u16, u32be>(0xeeff).into());
  assert_eq!(0xffu128, cast::<u16, u8>(0xeeff).into());

  assert_eq!(0, cast::<(), u8>(()));
  assert_eq!(0, cast::<(), u16>(()));
  assert_eq!(0, cast::<(), u32>(()));
  assert_eq!(0, cast::<(), u64>(()));
  assert_eq!(0, cast::<(), u128>(()));
}

#[test]
#[allow(clippy::bool_assert_comparison)]
fn test_default() {
  assert_eq!(0, default::<u8>());
  assert_eq!(0, default::<u16>());
  assert_eq!(false, default::<bool>());
  assert_eq!(None, default::<Option<NonZero<u8>>>());
  assert_eq!(NonZero::<u8>::new(1).unwrap(), default::<NonZero<u8>>());
  assert_eq!(NonZero::<u16>::new(1).unwrap(), default::<NonZero<u16>>());
  assert_eq!(NonZero::<u32>::new(1).unwrap(), default::<NonZero<u32>>());
  assert_eq!([false, false, false, false], default::<[bool; 4]>());
  // assert_eq!([0xca; 4], transmute::<_, [u8; 4]>(default::<[LiteralUint<u8, 0xca>; 4]>()));
  // assert_eq!([0xcafe; 4], transmute::<_, [u16; 4]>(default::<[LiteralUint<u16, 0xcafe>; 4]>()));
  // assert_eq!([0xcafebeef; 4], transmute::<_, [u32; 4]>(default::<[LiteralUint<u32, 0xcafebeef>; 4]>()));
  // assert_eq!([0xcafebeef; 4], transmute::<_, [u32; 4]>(default::<[LiteralUint<u32, 0xcafebeef>; 4]>()));
}

// #[test]
// #[allow(clippy::bool_assert_comparison)]
// fn test_zeroed() {
//   assert_eq!(0, zeroed::<u8>());
//   assert_eq!(0, zeroed::<u16>());
//   assert_eq!(false, zeroed::<bool>());
//   assert_eq!(None, zeroed::<Option<NonZero<u8>>>());
//   assert_eq!([false, false, false, false], zeroed::<[bool; 4]>());
// }
//
// #[test]
// #[allow(clippy::bool_assert_comparison)]
// fn test_transmute() {
//   assert_eq!(0, transmute::<u8, u8>(0));
//   assert_eq!(1, transmute::<u8, u8>(1));
//   assert_eq!(u8::MAX, transmute::<u8, u8>(u8::MAX));
//   assert_eq!(u32le::new(0xbeefcafe), transmute(pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
//   assert_eq!(u32be::new(0xcafebeef), transmute(pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
//   assert_eq!(0u8, transmute(u8c::<0>::new()));
//   assert_eq!(false, transmute(u8c::<0>::new()));
//   assert_eq!(true, transmute(u8c::<1>::new()));
//   assert_eq!(TestEnum::A, transmute(u32c::<0x01010101>::new()));
//   assert_eq!(TestEnum::B, transmute(u32c::<0x02020202>::new()));
//   assert_eq!(TestEnum::C, transmute(u32c::<0x03030303>::new()));
// }
//
// #[test]
// fn test() {
//   assert_eq!(0, truncate::<u16, u8>(0));
//   assert_eq!(1, truncate::<u16le, u8>(u16le::new(1)));
//   assert_eq!(0, truncate::<u16be, u8>(u16be::new(1)));
//   assert_eq!(0xfe, truncate::<u16le, u8>(u16le::new(0xcafe)));
//   assert_eq!(0xca, truncate::<u16be, u8>(u16be::new(0xcafe)));
//   assert_eq!(u8::MAX, truncate::<u16be, u8>(u16be::new(u16::MAX)));
//   assert_eq!(u16le::new(0xcafe), truncate(pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
//   assert_eq!(u16be::new(0xcafe), truncate(pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
// }
//
// #[test]
// fn test_transmute_copy() {
//   assert_eq!(0, transmute_copy::<u8, u8>(&0));
//   assert_eq!(1, transmute_copy::<u8, u8>(&1));
//   assert_eq!(u8::MAX, transmute_copy::<u8, u8>(&u8::MAX));
//   assert_eq!(u32le::new(0xbeefcafe), transmute_copy(&pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
//   assert_eq!(u32be::new(0xcafebeef), transmute_copy(&pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
// }
//
// #[test]
// fn test_try_transmute() {
//   assert_eq!(Some(false), try_transmute(0u8));
//   assert_eq!(Some(true), try_transmute(1u8));
//   assert_eq!(None, try_transmute::<_, bool>(2u8));
//   assert_eq!(Some(NonZero<u8>::new(1).unwrap()), try_transmute(1u8));
//   assert_eq!(None, try_transmute::<_, NonZero<u8>>(0u8));
//   assert_eq!(Some(0x01010101), try_transmute(TestEnum::A));
//   assert_eq!(Some(0x02020202), try_transmute(TestEnum::B));
//   assert_eq!(Some(0x03030303), try_transmute(TestEnum::C));
//   assert_eq!(Some(TestEnum::A), try_transmute(0x01010101));
//   assert_eq!(Some(TestEnum::B), try_transmute(0x02020202));
//   assert_eq!(Some(TestEnum::C), try_transmute(0x03030303));
//   assert_eq!(None, try_transmute::<_, TestEnum>(0x00000000));
//   assert_eq!(None, try_transmute::<_, TestEnum>(0x04040404));
//
//   type DstPacked = Pack![TestEnum, u16lec<0xcafe>, [Pack![LiteralUint<u8, 0x42>, TestEnum]]];
//
//   let bytes = [
//     0x01u8, 0x01, 0x01, 0x01, 0xfe, 0xca, 0x42, 0x02, 0x02, 0x02, 0x02, 0x42, 0x03, 0x03, 0x03,
//     0x03,
//   ];
//
//   let packed = pack!(
//     TestEnum::A,
//     u16lec::<0xcafe>::new(),
//     [pack!(u8c::<0x42>::new(), TestEnum::B), pack!(u8c::<0x42>::new(), TestEnum::C)]
//   );
//
//   assert_eq!(Some(packed), try_transmute(bytes));
//
//   let dst_packed: &DstPacked = &packed;
//   assert_eq!(Ok(dst_packed), try_transmute_ref(&bytes));
//   assert_eq!(Ok(dst_packed), try_transmute_ref(bytes.as_slice()));
// }
