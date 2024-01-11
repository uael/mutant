use crate::*;

#[macro_export]
macro_rules! layout {
  ($T:ty { $($field:ident: $field_ty:ty),+ $(,)? }) => {{
    let mut layout: $T = ::core::mem::zeroed();
    $(layout.$field = $crate::layout_of::<$field_ty>();)+
    $crate:Layoutt::new(layout)
  }};
}

#[macro_export]
macro_rules! assert_panic {
  ($msg:expr, $e:expr) => {
    assert_eq!(
      Some($msg),
      std::panic::catch_unwind(|| { $e })
        .err()
        .and_then(|a| a.downcast_ref::<&'static str>().cloned())
    )
  };
}

#[derive(Copy, Clone, DataType)]
#[repr(C, packed)]
#[mutant(crate)]
pub struct Foo<T: ?Sized> {
  a: u8,
  b: u16le,
  c: [u8; 6],
  d: Undrop<T>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, DataType)]
#[repr(C)]
#[mutant(crate)]
pub struct Bar<T: ?Sized> {
  a: [u8; 4],
  b: T,
}

#[test]
fn test_transmute() {
  let mut foo = Foo::<[u8; 3]> {
    a: 0x42,
    b: u16le::from_le_bytes([0x42, 0x42]),
    c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
    d: Undrop::new([0xaf, 0xaf, 0xaa]),
  };

  let bar: &mut Bar<[u8]> = cast_mut(&mut foo);

  assert_eq!([0x42; 4], bar.a);
  bar.a = [0xff; 4];

  assert_eq!(0xff, foo.a);
  assert_eq!(u16le::from_le_bytes([0xff, 0xff]), foo.b);
  assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

  pub fn pack<T: ?Sized + DataType>(value: &T) -> &[u8] {
    cast(value)
  }

  pub fn pack_foo(foo: &Foo<[u8; 3]>) -> &[u8] {
    pack(foo)
  }

  println!("{:?}", pack_foo(&foo));
}

#[test]
fn test_try_transmute() {
  assert_eq!(Ok(&[1, 2]), try_cast::<[u8], [u8; 2]>(&[1, 2]));
  assert_eq!(Err(1), try_cast::<[u8], [u8; 3]>(&[1, 2]));
  assert_eq!(Ok(&Bar { a: [1, 2, 3, 4], b: () }), try_cast::<[u8], Bar<()>>(&[1, 2, 3, 4]));
  println!("{:?}", try_cast::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
  // println!(
  //   "{:?}",
  //   try_cast::<Bar<[u8]>, Bar<[u8; 2]>>(
  //     try_cast::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
  //   )
  // );
  println!("{:?}", cast::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
}
