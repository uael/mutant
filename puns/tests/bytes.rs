//! Values from bytes: the whole of them (`from_bytes`), their front (`from_head`), their back
//! (`from_tail`), by reference and by mutable reference.

use puns::{
  Error, Pun, from_bytes, from_bytes_mut,
  num::{u16be, u16bec},
  try_pun_mut, try_pun_ref,
};

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Position {
  x: u8,
  visible: bool,
}

/// A size in the data.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
#[pun(size = length)]
struct Chunk {
  kind: u8,
  length: u16be,
  data: [u8],
}

/// Something with a greater alignment.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Word {
  value: u32,
}

/// A magic number: not every value is one.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Magic {
  magic: u16bec<0xcafe>,
  flag: bool,
}

#[repr(align(4))]
struct Aligned<T>(T);

#[test]
fn whole() {
  // The same as `try_pun_ref::<[u8], _>`, without naming the source.
  assert_eq!(from_bytes(&[7, 1]), Ok(&Position { x: 7, visible: true }));
  assert_eq!(from_bytes::<Position>(&[7, 1]), try_pun_ref::<[u8], Position>(&[7, 1]));
  assert_eq!(from_bytes::<Position>(&[7, 2]), Err(Error::Validity));
  assert_eq!(from_bytes::<Position>(&[7]), Err(Error::Size));
  assert_eq!(from_bytes::<str>(b"caf\xc3\xa9"), Ok("caf\u{e9}"));
  assert_eq!(from_bytes::<str>(b"caf\xc3"), Err(Error::Validity));
  let chunk: &Chunk = from_bytes(&[1, 0, 5, b'h', b'i']).unwrap();
  assert_eq!((chunk.kind, &chunk.data), (1, &b"hi"[..]));
  // The whole: the rule says 5 bytes, not 6.
  assert_eq!(from_bytes::<Chunk>(&[1, 0, 5, b'h', b'i', 0]).err(), Some(Error::Validity));
  // Alignment.
  let words = Aligned([0u8, 0, 0, 1, 0, 0, 0, 2]);
  assert_eq!(
    from_bytes::<Word>(&words.0[..4]).map(|w| w.value),
    Ok(u32::from_ne_bytes([0, 0, 0, 1]))
  );
  assert_eq!(from_bytes::<Word>(&words.0[1..5]).err(), Some(Error::Alignment));
  assert_eq!(from_bytes::<[Word]>(&words.0[..]).map(<[Word]>::len), Ok(2));
  assert_eq!(from_bytes::<Magic>(&[0xca, 0xfe, 1]).map(|m| m.flag), Ok(true));
  assert_eq!(from_bytes::<Magic>(&[0xca, 0xff, 1]).err(), Some(Error::Validity));
}

#[test]
fn whole_mut() {
  let mut bytes = [7u8, 1, 0];
  let position = from_bytes_mut::<Position>(&mut bytes[..2]).unwrap();
  position.visible = false;
  position.x = 9;
  assert_eq!(bytes, [9, 0, 0]);
  assert_eq!(from_bytes_mut::<Position>(&mut [7, 2]).err(), Some(Error::Validity));
  assert_eq!(from_bytes_mut::<Position>(&mut [7, 1, 0]).err(), Some(Error::Size));
  let mut same = [7u8, 1];
  assert!(from_bytes_mut::<Position>(&mut same).is_ok());
  assert!(try_pun_mut::<[u8], Position>(&mut same).is_ok());
  let mut text = *b"abc";
  from_bytes_mut::<str>(&mut text).unwrap().make_ascii_uppercase();
  assert_eq!(&text, b"ABC");
}

#[test]
fn whole_in_const() {
  const POSITION: Result<&Position, Error> = from_bytes(&[3, 0]);
  assert_eq!(POSITION.map(|p| (p.x, p.visible)), Ok((3, false)));
}
