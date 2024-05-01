use crate::internal::*;
use crate::num::*;
use crate::*;
use core::num::{NonZeroU128, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8};

#[repr(u32)]
#[allow(unused)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum TestEnum {
  A = 0x01010101,
  B = 0x02020202,
  C = 0x03030303,
}

unsafe impl Integer for TestEnum {
  type Repr = u32;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0x01010101u32,
    BytePattern::Any(&[
      BytePattern::Equ(&0x01010101u32.to_ne_bytes()),
      BytePattern::Equ(&0x02020202u32.to_ne_bytes()),
      BytePattern::Equ(&0x03030303u32.to_ne_bytes()),
    ]),
  );
}

#[repr(u16)]
#[allow(unused)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum _0or4 {
  A = 0,
  B = 4,
}

unsafe impl Integer for _0or4 {
  type Repr = u16;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[
      BytePattern::Equ(&0u16.to_ne_bytes()),
      BytePattern::Equ(&4u16.to_ne_bytes()),
    ]),
  );
}

#[repr(u16)]
#[allow(unused)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum _0or4or8 {
  A = 0,
  B = 4,
  C = 8,
}

unsafe impl Integer for _0or4or8 {
  type Repr = u16;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[
      BytePattern::Equ(&0u16.to_ne_bytes()),
      BytePattern::Equ(&4u16.to_ne_bytes()),
      BytePattern::Equ(&8u16.to_ne_bytes()),
    ]),
  );
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo1u8 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Integer for UpTo1u8 {
  type Repr = u8;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[BytePattern::Equ(&0u8.to_ne_bytes()), BytePattern::Equ(&1u8.to_ne_bytes())]),
  );
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo1u16 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Integer for UpTo1u16 {
  type Repr = u16;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[
      BytePattern::Equ(&0u16.to_ne_bytes()),
      BytePattern::Equ(&1u16.to_ne_bytes()),
    ]),
  );
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo2u8 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Integer for UpTo2u8 {
  type Repr = u8;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[
      BytePattern::Equ(&0u8.to_ne_bytes()),
      BytePattern::Equ(&1u8.to_ne_bytes()),
      BytePattern::Equ(&2u8.to_ne_bytes()),
    ]),
  );
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo2u16 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Integer for UpTo2u16 {
  type Repr = u16;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(
    0,
    BytePattern::Any(&[
      BytePattern::Equ(&0u16.to_ne_bytes()),
      BytePattern::Equ(&1u16.to_ne_bytes()),
      BytePattern::Equ(&2u16.to_ne_bytes()),
    ]),
  );
}

#[repr(transparent)]
#[derive(Copy, Clone)]
struct Uninit(u8);

unsafe impl Integer for Uninit {
  type Repr = u8;
  const SPEC: IntegerSpec<Self::Repr> = IntegerSpec::new(0, BytePattern::Uninit);
}

#[repr(C)]
#[derive(Data, Debug, PartialEq)]
#[data(crate)]
struct TailSliceDst {
  info: u32,
  data: [u8], // `tail slice` of type `[u8]`.
}

#[repr(C)]
#[derive(Data, Debug, PartialEq)]
#[data(crate)]
struct TailDst {
  version: u32,
  payload: TailSliceDst, // `tail` DST of type `TailSliceDst`.
}

#[repr(C)]
#[derive(Data, Debug, PartialEq)]
#[data(crate)]
struct MaybeDst<T: ?Sized> {
  info: u32,
  data: T, // `tail` of type `T`.
}

#[derive(Data, Copy, Clone, Debug, PartialEq)]
#[data(crate)]
struct NotDstRust {
  info: u32,
  data: [u32; 21],
}

#[test]
fn test_internal_default_at() {
  assert_eq!(default_at::<u8>(0), Some(0));
  assert_eq!(default_at::<u8>(1), None);
  assert_eq!(default_at::<u8>(usize::MAX), None);
  assert_eq!(default_at::<u32>(0), Some(0));
  assert_eq!(default_at::<u32>(1), Some(0));
  assert_eq!(default_at::<u32>(2), Some(0));
  assert_eq!(default_at::<u32>(3), Some(0));
  assert_eq!(default_at::<u32>(4), None);
  assert_eq!(default_at::<u32>(usize::MAX), None);
  assert_eq!(default_at::<TailSliceDst>(0), Some(0));
  assert_eq!(default_at::<TailSliceDst>(1), Some(0));
  assert_eq!(default_at::<TailSliceDst>(2), Some(0));
  assert_eq!(default_at::<TailSliceDst>(3), Some(0));
  assert_eq!(default_at::<TailSliceDst>(4), Some(0));
  assert_eq!(default_at::<TailSliceDst>(5), Some(0)); // etc..
  assert_eq!(default_at::<TailDst>(0), Some(0));
  assert_eq!(default_at::<TailDst>(1), Some(0));
  assert_eq!(default_at::<TailDst>(2), Some(0));
  assert_eq!(default_at::<TailDst>(3), Some(0));
  assert_eq!(default_at::<TailDst>(4), Some(0));
  assert_eq!(default_at::<TailDst>(5), Some(0));
  assert_eq!(default_at::<TailDst>(6), Some(0));
  assert_eq!(default_at::<TailDst>(7), Some(0));
  assert_eq!(default_at::<TailDst>(8), Some(0));
  assert_eq!(default_at::<TailDst>(9), Some(0)); // etc..
}

#[test]
fn test_internal_convert_metadata() {
  assert_eq!(convert_metadata::<u8, u8>(0), 0);
  assert_eq!(convert_metadata::<u8, [u8]>(0), 1);
  assert_eq!(convert_metadata::<u32, [u8]>(0), 4);
  assert_eq!(convert_metadata::<u32, [u32]>(0), 1);
  assert_eq!(convert_metadata::<u64, [u32]>(0), 2);
  assert_eq!(convert_metadata::<[u8; 21], TailSliceDst>(0), 21 - 4);
  assert_eq!(convert_metadata::<[u8; 21], TailDst>(0), 21 - 4 - 4);
  assert_eq!(convert_metadata::<[u8; 21], MaybeDst<[u8]>>(0), 21 - 4);
  assert_eq!(convert_metadata::<[u8; 21], MaybeDst<u32>>(0), 0);
  assert_eq!(convert_metadata::<[u32; 42], NotDstRust>(0), 0);
}

#[test]
fn test_internal_try_convert_metadata() {
  // Same as `test_internal_convert_metadata`:
  assert_eq!(try_convert_metadata::<u8, u8>(0), Ok(0));
  assert_eq!(try_convert_metadata::<u8, [u8]>(0), Ok(1));
  assert_eq!(try_convert_metadata::<u32, [u8]>(0), Ok(4));
  assert_eq!(try_convert_metadata::<[u8; 21], TailSliceDst>(0), Ok(21 - 4));
  assert_eq!(try_convert_metadata::<[u8; 21], TailDst>(0), Ok(21 - 4 - 4));
  assert_eq!(try_convert_metadata::<[u8; 21], MaybeDst<[u8]>>(0), Ok(21 - 4));
  assert_eq!(try_convert_metadata::<[u8; 21], MaybeDst<u32>>(0), Ok(0));
  assert_eq!(try_convert_metadata::<[u32; 42], NotDstRust>(0), Ok(0));
  // New ones:
  assert_eq!(try_convert_metadata::<[u8], u8>(0), Err(1));
  assert_eq!(try_convert_metadata::<[u8], u8>(1), Ok(0));
  assert_eq!(try_convert_metadata::<[u8], u8>(2), Err(-1));
  assert_eq!(try_convert_metadata::<[u8], u32>(2), Err(2));
  assert_eq!(try_convert_metadata::<[u8], u32>(4), Ok(0));
  assert_eq!(try_convert_metadata::<[u8], u32>(6), Err(-2));
  assert_eq!(try_convert_metadata::<[u8], [u8]>(5), Ok(5));
  assert_eq!(try_convert_metadata::<[u8], [u32]>(0), Ok(0));
  assert_eq!(try_convert_metadata::<[u8], [u32]>(4), Ok(1));
  assert_eq!(try_convert_metadata::<[u8], [u32]>(8), Ok(2));
  assert_eq!(try_convert_metadata::<[u8], [u32]>(1), Err(3));
  assert_eq!(try_convert_metadata::<[u8], [u32]>(5), Err(3));
  assert_eq!(try_convert_metadata::<[u32], [u8]>(0), Ok(0));
  assert_eq!(try_convert_metadata::<[u32], [u8]>(1), Ok(4));
  assert_eq!(try_convert_metadata::<[u32], [u8]>(2), Ok(8));
  assert_eq!(try_convert_metadata::<[u32], [u16]>(1), Ok(2));
  assert_eq!(try_convert_metadata::<[u32], [u16]>(2), Ok(4));
  assert_eq!(try_convert_metadata::<[u32], [u64]>(0), Ok(0));
  assert_eq!(try_convert_metadata::<[u32], [u64]>(1), Err(4));
  assert_eq!(try_convert_metadata::<TailSliceDst, u32>(0), Ok(0));
  assert_eq!(try_convert_metadata::<TailSliceDst, u32>(4), Err(-4));
  assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 20), Ok(0));
  assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 19), Err(4));
  assert_eq!(try_convert_metadata::<TailSliceDst, [u32; 21]>(4 * 21), Err(-4));
}

#[test]
fn test_internal_tail_offset() {
  assert_eq!(tail_offset::<u8>(), 0);
  assert_eq!(tail_offset::<[u8]>(), 0);
  assert_eq!(tail_offset::<[u8; 1]>(), 0);
  assert_eq!(tail_offset::<TailSliceDst>(), 4);
  assert_eq!(tail_offset::<TailDst>(), 4);
  assert_eq!(tail_offset::<MaybeDst<[u8]>>(), 4);
  assert_eq!(tail_offset::<MaybeDst<u32>>(), 4);
  assert_eq!(tail_offset::<NotDstRust>(), 4);
}

#[test]
fn test_internal_tail_item_size() {
  assert_eq!(tail_item_size::<u8>(), 0);
  assert_eq!(tail_item_size::<[u8]>(), 0);
  assert_eq!(tail_item_size::<[u8; 1]>(), 0);
  assert_eq!(tail_item_size::<TailSliceDst>(), 1);
  assert_eq!(tail_item_size::<TailDst>(), 0);
  assert_eq!(tail_item_size::<MaybeDst<[u8]>>(), 1);
  assert_eq!(tail_item_size::<MaybeDst<[u32]>>(), 4);
  assert_eq!(tail_item_size::<MaybeDst<[u8; 21]>>(), 1);
  assert_eq!(tail_item_size::<MaybeDst<[u32; 21]>>(), 4);
  assert_eq!(tail_item_size::<NotDstRust>(), 4);
}

#[test]
fn test_internal_validate() {
  // TODO
}

#[test]
#[allow(unused_parens)]
fn test_internal_static_analyze() {
  use MetadataChecks::*;

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
      assert_eq!(static_analyze::<test!(@$src), test!(@$dst)>(), Err($res))
    );
    ($src:tt $dst:tt => $res:ident) => (
      assert_eq!(static_analyze::<test!(@$src), test!(@$dst)>(), Ok(StaticAnalyze::$res(None)))
    );
    ($src:tt $dst:tt => $res:ident ($check:expr)) => (
      assert_eq!(static_analyze::<test!(@$src), test!(@$dst)>(), Ok(StaticAnalyze::$res(Some($check))))
    );
  }

  test!(u8 u8 => infallible_mutable);
  test!((u8, u8) (u8, u8) => infallible_mutable);
  test!((u8, [u8]) (u8, [u8]) => infallible_mutable);

  test!(u8 u8 => infallible_mutable);
  test!(bool bool => infallible_mutable);
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

  test!(u32 u8 => infallible_mutable_truncate);
  test!(u8 u32 => "source too small to fit target");

  // Test with UpTo2u8 type
  test!(UpTo2u8 UpTo2u8 => infallible_mutable);
  test!(UpTo2u8 u8 => infallible);
  test!(u8 UpTo2u8 => fallible_mutable);
  test!(UpTo2u8 u16 => "source too small to fit target");
  test!(u16 UpTo2u8 => fallible_mutable_truncate);

  // Test with TestEnum type
  test!(TestEnum TestEnum => infallible_mutable);
  test!(TestEnum u32 => infallible);
  test!(TestEnum u16 => infallible_truncate);
  test!(TestEnum u8 => infallible_truncate);
  test!(u32 TestEnum => fallible_mutable);
  test!(TestEnum u64 => "source too small to fit target");
  test!(u64 TestEnum => fallible_mutable_truncate);

  // TODO: test!(_0or4 _0or4 => infallible_mutable);
  // TODO: test!(_0or4or8 _0or4or8 => infallible_mutable);
  // TODO: test!(_0or4or8 _0or4 => fallible_mutable);
  // TODO: test!(_0or4 _0or4or8 => infallible);

  test!((u8c<1>) NonZeroU8 => infallible);
  test!((u16c<1>) NonZeroU16 => infallible);
  test!((u32c<1>) NonZeroU32 => infallible);
  test!((u64c<1>) NonZeroU64 => infallible);
  test!((u128c<1>) NonZeroU128 => infallible);

  #[derive(Data)]
  #[repr(C, packed)]
  #[data(crate)]
  struct T<const HEAD: usize, const TAIL: usize = 0>([u8; HEAD], [[u8; TAIL]]);

  // Basic tests for equal head and tail sizes
  test!((T<1>) (T<1>) => infallible_mutable);
  test!((T<42>) (T<42>) => infallible_mutable);
  test!((T<42, 1>) (T<42, 1>) => infallible_mutable);

  // Test for equal head sizes with different tail sizes
  test!((T<42, 3>) (T<42, 1>) => infallible_mutable);
  test!((T<50, 10>) (T<50, 100>) => infallible_mutable(IsMultipleOf(10)));
  test!((T<50, 5>) (T<50, 100>) => infallible_mutable(IsMultipleOf(20)));

  // Test cases for transmuting to and from empty tail
  test!((T<42, 1>) (T<42>) => infallible_mutable(IsEmpty));
  test!((T<35, 1>) (T<42>) => infallible_mutable(Is(7)));
  test!((T<2>) (T<1>) => infallible_mutable_truncate);

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
  // test!((T<0, 1>) (T<42, 1>) => infallible_mutable(IsAtLeast(42)));

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

  // Test cases for transmuting to and from empty padded types
  test!(_ u8 => "uninitialized byte in source");
  test!((_, _, u16) u32 => "uninitialized byte in source");
  test!(((u8, _), u16) u32 => "uninitialized byte in source");
  test!(((u8, _), (u8, _)) ((bool, _), (bool, _)) => fallible_mutable);
  test!((u32, [(u8, _)]) (u32, [u16]) => "uninitialized byte in source");
  test!((u32, [(u8, _)]) (u32, [(bool, _)]) => fallible_mutable);
  test!((u16, [(u8, _)]) (u32, [u16]) => "uninitialized byte in source");
  test!(((u8, u8), u16) ((u8, bool), u16) => fallible_mutable);

  // Test for unreachable transmutations
  test!((u8c<2>) bool => "none of the bit patterns in source are valid in target");
  test!((u8c<0>) u8 => infallible);
  test!((u8c<0>) NonZeroU8 => "none of the bit patterns in source are valid in target");
  test!((u16c<0>) u16 => infallible);
  test!((u16c<0>) NonZeroU16 => "none of the bit patterns in source are valid in target");
  test!((u32c<0>) u32 => infallible);
  test!((u32c<0>) NonZeroU32 => "none of the bit patterns in source are valid in target");
  test!((u64c<0>) u64 => infallible);
  test!((u64c<0>) NonZeroU64 => "none of the bit patterns in source are valid in target");
  test!((u128c<0>) u128 => infallible);
  test!((u128c<0>) NonZeroU128 => "none of the bit patterns in source are valid in target");
  test!((u128c<0>) (Option<NonZeroU128>) => infallible);
  test!(TestEnum NonZeroU32 => infallible);
  test!(NonZeroU32 TestEnum => fallible_mutable);
  test!(TestEnum (u32c<0>) => "none of the bit patterns in source are valid in target");
  test!((u32c<0>) TestEnum => "none of the bit patterns in source are valid in target");

  test!([u8; 16] (TestEnum, (u16lec<0xcafe>), [((u8c<0x42>), TestEnum); 2]) => fallible_mutable);
}

#[test]
fn test_size_field() {
  type T0 = Pack!(Size<u32>, ());
  type T1 = Pack!(u64, Size<u32>, ());
  type T2 = Pack!([u8; 0xcafebeef], Size<u32>, ());
  type T3 = Pack!([u8; 0xcafebeef], Size<u32be>, ());
  type T4 = Pack!([u8; 0xcafebeef], Size<u32>, [u8]);
  type T5 = Pack!([u8; 0xcafebeef], Size<u32be>, [u8]);

  type SizeType<T> = <<<T as Data>::TailField as Field>::PrevField as Field>::Type;

  pub const fn pat_of<T: ?Sized + Data>() -> BytePattern<'static> {
    match <SizeType<T> as Data>::LAYOUT {
      Layout::Integer { spec } => spec.byte_pattern,
      _ => unreachable!(),
    }
  }

  pub const fn default_of<T: ?Sized + Data>() -> u32 {
    match <SizeType<T> as Data>::LAYOUT {
      Layout::Integer { spec } => cast(spec.default()),
      _ => unreachable!(),
    }
  }

  assert_eq!(default_of::<T0>(), 4);
  assert!(!pat_of::<T0>().matches(3u32));
  assert!(pat_of::<T0>().matches(4u32));
  assert!(!pat_of::<T0>().matches(5u32));

  assert_eq!(default_of::<T1>(), 12);
  assert!(!pat_of::<T1>().matches(11u32));
  assert!(pat_of::<T1>().matches(12u32));
  assert!(!pat_of::<T1>().matches(13u32));

  assert_eq!(default_of::<T2>(), 0xcafebeefu32 + 4);
  assert!(!pat_of::<T2>().matches(0xcafebeefu32 + 4 - 1));
  assert!(pat_of::<T2>().matches(0xcafebeefu32 + 4));
  assert!(!pat_of::<T2>().matches(0xcafebeefu32 + 4 + 1));

  assert_eq!(default_of::<T3>(), 0xcafebeefu32 + 4);
  assert!(!pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4 - 1)));
  assert!(pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4)));
  assert!(!pat_of::<T3>().matches(u32be::from_raw(0xcafebeefu32 + 4 + 1)));

  assert_eq!(default_of::<T4>(), 0xcafebeefu32 + 4);
  assert!(!pat_of::<T4>().matches(0xcafebeefu32 + 4 - 1));
  assert!(pat_of::<T4>().matches(0xcafebeefu32 + 4));
  assert!(pat_of::<T4>().matches(0xcafebeefu32 + 4 + 1));
  assert!(pat_of::<T4>().matches(u32::MAX));

  assert_eq!(default_of::<T5>(), 0xcafebeefu32 + 4);
  assert!(!pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4 - 1)));
  assert!(pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4)));
  assert!(pat_of::<T5>().matches(u32be::from_raw(0xcafebeefu32 + 4 + 1)));
  assert!(pat_of::<T4>().matches(u32be::MAX));
}

#[test]
fn test_num() {
  use core::array::from_fn;
  use rand::Rng;

  #[rustfmt::skip]
  macro_rules! test {
    ($sz:literal, [$($ty:ident),+], $min:literal, $max:literal) => {$(
      let val = rand::thread_rng().gen_range($min..=$max);
      assert_eq!($sz, mem::size_of::<$ty>());
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
  assert_eq!(0xff, cast::<_, u8>(big));
  assert_eq!(0xeeff, cast::<_, u16>(big));
  assert_eq!(0xeeff, cast::<_, u16le>(big).into_raw());
  assert_eq!(0xeeff, cast::<_, u16be>(big).into_raw());
  assert_eq!(0xddeeff, cast::<_, u24>(big).into_raw());
  assert_eq!(0xddeeff, cast::<_, u24le>(big).into_raw());
  assert_eq!(0xddeeff, cast::<_, u24be>(big).into_raw());
  assert_eq!(0xccddeeff, cast::<_, u32>(big));
  assert_eq!(0xccddeeff, cast::<_, u32le>(big).into_raw());
  assert_eq!(0xccddeeff, cast::<_, u32be>(big).into_raw());
  assert_eq!(0xbbccddeeff, cast::<_, u40>(big).into_raw());
  assert_eq!(0xbbccddeeff, cast::<_, u40le>(big).into_raw());
  assert_eq!(0xbbccddeeff, cast::<_, u40be>(big).into_raw());
  assert_eq!(0xaabbccddeeff, cast::<_, u48>(big).into_raw());
  assert_eq!(0xaabbccddeeff, cast::<_, u48le>(big).into_raw());
  assert_eq!(0xaabbccddeeff, cast::<_, u48be>(big).into_raw());
  assert_eq!(0x99aabbccddeeff, cast::<_, u56>(big).into_raw());
  assert_eq!(0x99aabbccddeeff, cast::<_, u56le>(big).into_raw());
  assert_eq!(0x99aabbccddeeff, cast::<_, u56be>(big).into_raw());
  assert_eq!(0x8899aabbccddeeff, cast::<_, u64>(big));
  assert_eq!(0x8899aabbccddeeff, cast::<_, u64le>(big).into_raw());
  assert_eq!(0x8899aabbccddeeff, cast::<_, u64be>(big).into_raw());

  assert_eq!(0xff, cast::<u8, u128>(0xff));
  assert_eq!(0xff, cast::<u8, u128le>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u128be>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u64>(0xff));
  assert_eq!(0xff, cast::<u8, u64le>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u64be>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u32>(0xff));
  assert_eq!(0xff, cast::<u8, u32le>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u32be>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u16>(0xff));
  assert_eq!(0xff, cast::<u8, u16le>(0xff).into_raw());
  assert_eq!(0xff, cast::<u8, u16be>(0xff).into_raw());

  assert_eq!(0xeeff, cast::<u16, u128>(0xeeff));
  assert_eq!(0xeeff, cast::<u16, u128le>(0xeeff).into_raw());
  assert_eq!(0xeeff, cast::<u16, u128be>(0xeeff).into_raw());
  assert_eq!(0xeeff, cast::<u16, u64>(0xeeff));
  assert_eq!(0xeeff, cast::<u16, u64le>(0xeeff).into_raw());
  assert_eq!(0xeeff, cast::<u16, u64be>(0xeeff).into_raw());
  assert_eq!(0xeeff, cast::<u16, u32>(0xeeff));
  assert_eq!(0xeeff, cast::<u16, u32le>(0xeeff).into_raw());
  assert_eq!(0xeeff, cast::<u16, u32be>(0xeeff).into_raw());
  assert_eq!(0xff, cast::<u16, u8>(0xeeff));

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
  assert_eq!(None, default::<Option<NonZeroU8>>());
  assert_eq!(NonZeroU8::new(1).unwrap(), default::<NonZeroU8>());
  assert_eq!(NonZeroU16::new(1).unwrap(), default::<NonZeroU16>());
  assert_eq!(NonZeroU32::new(1).unwrap(), default::<NonZeroU32>());
  assert_eq!([false, false, false, false], default::<[bool; 4]>());
  assert_eq!([0xca; 4], transmute::<_, [u8; 4]>(default::<[u8c<0xca>; 4]>()));
  assert_eq!([0xcafe; 4], transmute::<_, [u16; 4]>(default::<[u16c<0xcafe>; 4]>()));
  assert_eq!([0xcafebeef; 4], transmute::<_, [u32; 4]>(default::<[u32c<0xcafebeef>; 4]>()));
  assert_eq!([0xcafebeef; 4], transmute::<_, [u32; 4]>(default::<[u32c<0xcafebeef>; 4]>()));
}

#[test]
#[allow(clippy::bool_assert_comparison)]
fn test_zeroed() {
  assert_eq!(0, zeroed::<u8>());
  assert_eq!(0, zeroed::<u16>());
  assert_eq!(false, zeroed::<bool>());
  assert_eq!(None, zeroed::<Option<NonZeroU8>>());
  assert_eq!([false, false, false, false], zeroed::<[bool; 4]>());
}

#[test]
#[allow(clippy::bool_assert_comparison)]
fn test_transmute() {
  assert_eq!(0, transmute::<u8, u8>(0));
  assert_eq!(1, transmute::<u8, u8>(1));
  assert_eq!(u8::MAX, transmute::<u8, u8>(u8::MAX));
  assert_eq!(u32le::new(0xbeefcafe), transmute(pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
  assert_eq!(u32be::new(0xcafebeef), transmute(pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
  assert_eq!(0u8, transmute(u8c::<0>::new()));
  assert_eq!(false, transmute(u8c::<0>::new()));
  assert_eq!(true, transmute(u8c::<1>::new()));
  assert_eq!(TestEnum::A, transmute(u32c::<0x01010101>::new()));
  assert_eq!(TestEnum::B, transmute(u32c::<0x02020202>::new()));
  assert_eq!(TestEnum::C, transmute(u32c::<0x03030303>::new()));
}

#[test]
fn test_truncate() {
  assert_eq!(0, truncate::<u16, u8>(0));
  assert_eq!(1, truncate::<u16le, u8>(u16le::new(1)));
  assert_eq!(0, truncate::<u16be, u8>(u16be::new(1)));
  assert_eq!(0xfe, truncate::<u16le, u8>(u16le::new(0xcafe)));
  assert_eq!(0xca, truncate::<u16be, u8>(u16be::new(0xcafe)));
  assert_eq!(u8::MAX, truncate::<u16be, u8>(u16be::new(u16::MAX)));
  assert_eq!(u16le::new(0xcafe), truncate(pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
  assert_eq!(u16be::new(0xcafe), truncate(pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
}

#[test]
fn test_transmute_copy() {
  assert_eq!(0, transmute_copy::<u8, u8>(&0));
  assert_eq!(1, transmute_copy::<u8, u8>(&1));
  assert_eq!(u8::MAX, transmute_copy::<u8, u8>(&u8::MAX));
  assert_eq!(u32le::new(0xbeefcafe), transmute_copy(&pack!(u16le::new(0xcafe), 0xefu8, 0xbeu8)));
  assert_eq!(u32be::new(0xcafebeef), transmute_copy(&pack!(u16be::new(0xcafe), 0xbeu8, 0xefu8)));
}

#[test]
fn test_try_transmute() {
  assert_eq!(Some(false), try_transmute(0u8));
  assert_eq!(Some(true), try_transmute(1u8));
  assert_eq!(None, try_transmute::<_, bool>(2u8));
  assert_eq!(Some(NonZeroU8::new(1).unwrap()), try_transmute(1u8));
  assert_eq!(None, try_transmute::<_, NonZeroU8>(0u8));
  assert_eq!(Some(0x01010101), try_transmute(TestEnum::A));
  assert_eq!(Some(0x02020202), try_transmute(TestEnum::B));
  assert_eq!(Some(0x03030303), try_transmute(TestEnum::C));
  assert_eq!(Some(TestEnum::A), try_transmute(0x01010101));
  assert_eq!(Some(TestEnum::B), try_transmute(0x02020202));
  assert_eq!(Some(TestEnum::C), try_transmute(0x03030303));
  assert_eq!(None, try_transmute::<_, TestEnum>(0x00000000));
  assert_eq!(None, try_transmute::<_, TestEnum>(0x04040404));

  type DstPacked = Pack![TestEnum, u16lec<0xcafe>, [Pack![u8c<0x42>, TestEnum]]];

  let bytes = [
    0x01u8, 0x01, 0x01, 0x01, 0xfe, 0xca, 0x42, 0x02, 0x02, 0x02, 0x02, 0x42, 0x03, 0x03, 0x03,
    0x03,
  ];

  let packed = pack!(
    TestEnum::A,
    u16lec::<0xcafe>::new(),
    [pack!(u8c::<0x42>::new(), TestEnum::B), pack!(u8c::<0x42>::new(), TestEnum::C)]
  );

  assert_eq!(Some(packed), try_transmute(bytes));

  let dst_packed: &DstPacked = &packed;
  assert_eq!(Ok(dst_packed), try_transmute_ref(&bytes));
  assert_eq!(Ok(dst_packed), try_transmute_ref(bytes.as_slice()));
}
