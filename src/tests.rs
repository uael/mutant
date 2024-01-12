use crate::*;

#[derive(Copy, Clone, Data)]
#[repr(C)]
#[data(crate)]
pub struct Foo<T: ?Sized> {
  a: u8,
  b: u32,
  c: NoDrop<T>,
}

#[derive(Copy, Clone, Data)]
#[repr(C, packed)]
#[data(crate)]
pub struct FooPacked<T: ?Sized> {
  a: u8,
  b: u16le,
  c: [u8; 6],
  d: NoDrop<T>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Data)]
#[repr(C)]
#[data(crate)]
pub struct Bar<T: ?Sized> {
  a: [u8; 4],
  b: T,
}

#[test]
fn test_layout_of() {
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(0));
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(1));
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(2));
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(3));
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(4));
  assert_eq!(0xff, layout_of::<Bar<[u8]>>(5));
}

#[test]
fn test_transmute() {
  let mut foo = FooPacked::<[u8; 3]> {
    a: 0x42,
    b: u16le::from_le_bytes([0x42, 0x42]),
    c: [0x42, 0xff, 0xff, 0xff, 0xff, 0xff],
    d: NoDrop::new([0xaf, 0xaf, 0xaa]),
  };

  let bar: &mut Bar<[u8]> = transmute_mut(&mut foo);

  assert_eq!([0x42; 4], bar.a);
  bar.a = [0xff; 4];

  assert_eq!(0xff, foo.a);
  assert_eq!(u16le::from_le_bytes([0xff, 0xff]), foo.b);
  assert_eq!([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], foo.c);

  pub fn pack<T: ?Sized + Data>(value: &T) -> &[u8] {
    transmute_ref(value)
  }

  pub fn pack_foo(foo: &FooPacked<[u8; 3]>) -> &[u8] {
    pack(foo)
  }

  println!("{:?}", pack_foo(&foo));

  let foo = Foo::<[u8; 3]> { a: 0x42, b: 0x42, c: NoDrop::new([0xaf, 0xaf, 0xaa]) };

  let _: &Foo<[u8; 3]> = transmute_ref(&foo);
  let _: Result<&Foo<[u8; 3]>, isize> = try_transmute_ref(&foo);
}

#[test]
fn test_try_transmute() {
  assert_eq!(Ok(&[1, 2]), try_transmute_ref::<[u8], [u8; 2]>(&[1, 2]));
  assert_eq!(Err(1), try_transmute_ref::<[u8], [u8; 3]>(&[1, 2]));
  assert_eq!(
    Ok(&Bar { a: [1, 2, 3, 4], b: () }),
    try_transmute_ref::<[u8], Bar<()>>(&[1, 2, 3, 4])
  );
  println!("{:?}", try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
  println!(
    "{:?}",
    try_transmute_ref::<Bar<[u8]>, Bar<[u8; 2]>>(
      try_transmute_ref::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()
    )
  );
  println!("{:?}", transmute_ref::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
}
