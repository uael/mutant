use crate::internal::*;
use crate::num::*;
use crate::*;
use core::num::{NonZeroU128, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};

#[repr(u32)]
#[allow(unused)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum TestEnum {
  A = 0x01010101,
  B = 0x02020202,
  C = 0x03030303,
}

unsafe impl Word for TestEnum {
  type Raw = u32;
  const WORD_LAYOUT: &'static WordLayout = layout!(0x01010101 | 0x02020202 | 0x03030303);
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo1u8 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Word for UpTo1u8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1);
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo1u16 {
  _0 = 0,
  _1 = 1,
}

unsafe impl Word for UpTo1u16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1);
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum UpTo2u8 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Word for UpTo2u8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1 | 2);
}

#[repr(u16)]
#[derive(Copy, Clone)]
enum UpTo2u16 {
  _0 = 0,
  _1 = 1,
  _2 = 2,
}

unsafe impl Word for UpTo2u16 {
  type Raw = u16;
  const WORD_LAYOUT: &'static WordLayout = layout!(0 | 1 | 2);
}

#[repr(transparent)]
#[derive(Copy, Clone)]
struct NonMaxU8(u8);

unsafe impl Word for NonMaxU8 {
  type Raw = u8;
  const WORD_LAYOUT: &'static WordLayout = layout!(!u8::MAX);
}

#[repr(transparent)]
#[derive(Copy, Clone)]
struct Uninit(u8);

unsafe impl Word for Uninit {
  type Raw = Self;
  const WORD_LAYOUT: &'static WordLayout = WordLayout::from_ref::<0, 0>(&(1, 0, []));
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
}

#[test]
#[allow(unused_parens)]
fn test_static_analyze() {
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
  test!(u8 u32 => "source too small");

  // Test with UpTo2u8 type
  test!(UpTo2u8 UpTo2u8 => infallible_mutable);
  test!(UpTo2u8 u8 => infallible);
  test!(u8 UpTo2u8 => fallible_mutable);
  test!(UpTo2u8 u16 => "source too small");
  test!(u16 UpTo2u8 => fallible_mutable_truncate);

  // Test with TestEnum type
  test!(TestEnum TestEnum => infallible_mutable);
  test!(TestEnum u32 => infallible);
  test!(TestEnum u16 => infallible_truncate);
  test!(TestEnum u8 => infallible_truncate);
  test!(u32 TestEnum => fallible_mutable);
  test!(TestEnum u64 => "source too small");
  test!(u64 TestEnum => fallible_mutable_truncate);

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
  test!((T<0, 1>) (T<42, 1>) => infallible_mutable(IsAtLeast(42)));

  // Test cases for alignment and complex transmutations
  test!((T<42, 1>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 3)));
  test!((T<42, 2>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<43, 2>) (T<39, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<43, 2>) (T<39, 4>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<1, 2>) (T<5, 2>) => infallible_mutable(IsAtLeast(2)));
  test!((T<45, 2>) (T<38, 3>) => infallible_mutable(IsAtLeastThenMultipleOf(1, 3)));
  test!((T<40, 1>) (T<38, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<44, 5>) (T<38, 4>) => infallible_mutable(IsAtLeastThenMultipleOf(2, 4)));
  test!((T<38, 3>) (T<41, 3>) => infallible_mutable(IsAtLeast(1)));

  // Test cases for incompatible transmutations
  test!((T<1>) (T<2>) => "source too small");
  test!((T<35, 2>) (T<42>) => "missing bytes to fit the target is not a multiple of the source DST trailing slice item size");
  test!((T<40, 3>) (T<42>) => "missing bytes to fit the target is not a multiple of the source DST trailing slice item size");
  test!((T<42>) (T<38, 3>) => "remaining bytes to fit in the target is not a multiple of the target DST trailing slice item size");
  test!((T<42, 3>) (T<38, 3>) => "DST source cannot catchup on DST target");
  test!((T<41, 3>) (T<39, 3>) => "DST source cannot catchup on DST target");
  test!((T<38, 3>) (T<42, 3>) => "DST source cannot catchup on DST target");
  test!((T<39, 3>) (T<41, 3>) => "DST source cannot catchup on DST target");
  test!((T<39, 3>) (T<41, 6>) => "DST source cannot catchup on DST target");
  test!((T<39, 6>) (T<41, 3>) => "DST source cannot catchup on DST target");
  test!((T<39, 6>) (T<41, 9>) => "DST source cannot catchup on DST target");
  test!((T<39, 3>) (T<41, 9>) => "DST source cannot catchup on DST target");

  // Test cases for transmuting to and from empty head
  test!((T<0, 1>) (T<0, 1>) => infallible_mutable);
  test!((T<0, 1>) (T<0>) => infallible_mutable(IsEmpty));
  test!((T<0, 1>) (T<0, 2>) => infallible_mutable(IsMultipleOf(2)));
  test!((T<0, 1>) (T<0, 3>) => infallible_mutable(IsMultipleOf(3)));
  test!((T<0, 1>) (T<0, 4>) => infallible_mutable(IsMultipleOf(4)));
  test!((T<0, 3>) (T<0, 5>) => infallible_mutable(IsMultipleOf(5)));

  // Test cases for transmuting to and from empty head and tail
  test!((T<0, 0>) (T<0, 0>) => infallible_mutable);

  // Test cases for transmuting to and from empty padded types
  test!(_ u8 => "uninit bytes");
  test!((_, _, u16) u32 => "uninit bytes");
  test!(((u8, _), u16) u32 => "uninit bytes");
  test!(((u8, _), (u8, _)) ((bool, _), (bool, _)) => fallible_mutable);
  test!((u32, [(u8, _)]) (u32, [u16]) => "uninit bytes");
  test!((u32, [(u8, _)]) (u32, [(bool, _)]) => fallible_mutable);
  test!((u16, [(u8, _)]) (u32, [u16]) => "uninit bytes");
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
  test!((usizec<0>) usize => infallible);
  test!((usizec<0>) NonZeroUsize => "none of the bit patterns in source are valid in target");
  test!((u128c<0>) u128 => infallible);
  test!((u128c<0>) NonZeroU128 => "none of the bit patterns in source are valid in target");
  test!((u128c<0>) (Option<NonZeroU128>) => infallible);
  test!(TestEnum NonZeroU32 => infallible);
  test!(NonZeroU32 TestEnum => fallible_mutable);
  test!(TestEnum (u32c<0>) => "none of the bit patterns in source are valid in target");
  test!((u32c<0>) TestEnum => "none of the bit patterns in source are valid in target");

  // Test for NonMaxU8 type
  test!(NonMaxU8 NonMaxU8 => infallible_mutable);
  test!(NonMaxU8 NonZeroU8 => fallible);

  test!([u8; 16] (TestEnum, (u16lec<0xcafe>), [((u8c<0x42>), TestEnum); 2]) => fallible_mutable);
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

  type DstPacked = Pack![TestEnum, u16lec<0xcafe>, [Pack![u8c<0x42>, Unalign<TestEnum>]]];

  let bytes = [
    0x01u8, 0x01, 0x01, 0x01, 0xfe, 0xca, 0x42, 0x02, 0x02, 0x02, 0x02, 0x42, 0x03, 0x03, 0x03,
    0x03,
  ];

  let packed = pack!(
    TestEnum::A,
    u16lec::<0xcafe>::new(),
    [
      pack!(u8c::<0x42>::new(), Unalign::new(TestEnum::B)),
      pack!(u8c::<0x42>::new(), Unalign::new(TestEnum::C))
    ]
  );

  assert_eq!(Some(packed), try_transmute(bytes));

  let dst_packed: &DstPacked = &packed;
  assert_eq!(Ok(dst_packed), try_transmute_ref(&bytes));
  assert_eq!(Ok(dst_packed), try_transmute_ref(bytes.as_slice()));
}
