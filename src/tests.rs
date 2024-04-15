use crate::internal::*;
use crate::*;

#[repr(u32)]
#[allow(unused)]
#[derive(Copy, Clone)]
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

macro_rules! impl_pack {
  ($($name:ident ($($t:ident)+))+) => {
    $(
      #[repr(C, packed)]
      #[derive(Data)]
      #[data(crate)]
      struct $name<$($t),+: ?Sized>($(pub Dropless<$t>),+);
    )+
  };
}

impl_pack!(
  Pack1 (A)
  Pack2 (A B)
  Pack3 (A B C)
  Pack4 (A B C D)
  Pack5 (A B C D E)
  Pack6 (A B C D E F)
  Pack7 (A B C D E F G)
  Pack8 (A B C D E F G H)
  Pack9 (A B C D E F G H I)
);

#[allow(unused)]
#[derive(Copy, Clone)]
struct Uninit(u8);

unsafe impl Word for Uninit {
  type Raw = Self;
  const WORD_LAYOUT: &'static WordLayout = WordLayout::from_ref::<0, 0>(&(1, 0, []));
}

#[test]
fn test_num() {
  use crate::num::*;
  use core::array::from_fn;

  #[rustfmt::skip]
  macro_rules! test {
      ($sz:literal, [$($ty:ident),+], $min:literal, $max:literal, $val:literal) => {$(
        assert_eq!($sz, mem::size_of::<$ty>());
        assert_eq!($max, $ty::MAX.into());
        assert_eq!($min, $ty::from($min).into());
        assert_eq!($max, $ty::from($max).into());
        assert_eq!($val, $ty::from($val).into());
        assert_eq!(from_fn(|i| i as u8), $ty::from_ne_bytes(from_fn(|i| i as u8)).to_ne_bytes());
        assert_eq!(from_fn(|i| i as u8), $ty::from_le_bytes(from_fn(|i| i as u8)).to_le_bytes());
        assert_eq!(from_fn(|i| i as u8), $ty::from_be_bytes(from_fn(|i| i as u8)).to_be_bytes());
      )+};
    }

  test!(2, [u16, u16le, u16be], 0u16, 0xffffu16, 0x0f1fu16);
  test!(3, [u24, u24le, u24be], 0u32, 0xffffffu32, 0x0f1f2fu32);
  test!(4, [u32, u32le, u32be], 0u32, 0xffffffffu32, 0x0f1f2f3fu32);
  test!(5, [u40, u40le, u40be], 0u64, 0xffffffffffu64, 0x0f1f2f3f4fu64);
  test!(6, [u48, u48le, u48be], 0u64, 0xffffffffffffu64, 0x0f1f2f3f4f5fu64);
  test!(7, [u56, u56le, u56be], 0u64, 0xffffffffffffffu64, 0x0f1f2f3f4f5f6fu64);
  test!(8, [u64, u64le, u64be], 0u64, 0xffffffffffffffffu64, 0x0f1f2f3f4f5f6f7fu64);
}

#[test]
fn test_static_analyze() {
  use MetadataChecks::*;

  macro_rules! test {
    (@ ($a:tt)) => { test!(@$a) };
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
}
