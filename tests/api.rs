//! The functions of the crate, on types of all sorts.

use core::num::NonZero;
use mutant::num::{u8c, u16be, u16le, u32bec};
use mutant::{
  Data, Error, as_bytes, transmute, transmute_mut, transmute_ref, truncate, truncate_ref,
  try_transmute, try_transmute_mut, try_transmute_ref, unpack,
};

/// An enum of the default representation: the compiler chooses its size and its values.
#[derive(Data, Debug, Copy, Clone, PartialEq)]
enum Level {
  Low,
  Mid,
  High,
}

#[derive(Data, Debug, Copy, Clone, PartialEq)]
#[repr(i8)]
enum Sign {
  Minus = -1,
  Zero = 0,
  Plus = 1,
}

#[derive(Data, Debug, Copy, Clone, PartialEq)]
#[repr(u16)]
enum Wide {
  A = 0x0102,
  B = 0xfffe,
}

/// A struct of the default representation: the compiler chooses the order of its fields.
#[derive(Data, Debug, Copy, Clone, PartialEq)]
struct Rust {
  flag: bool,
  word: u32,
  level: Level,
  bytes: [u8; 2],
}

/// An unsized struct with trailing padding: a value with one byte of `tail` is 8 bytes.
#[derive(Data, Debug, PartialEq)]
#[repr(C)]
struct Padded {
  word: u32,
  tail: [u8],
}

/// A count of elements, in a tuple struct.
#[derive(Data)]
#[repr(C)]
struct List(u8, #[data(len = self.0 / 2)] [u16le]);

#[derive(Data, Debug, PartialEq)]
#[repr(C)]
#[data(assert(kind == 1 || kind == 2 && value >= 0x100))]
struct Checked {
  kind: u8,
  zero: u8c<0>,
  value: u16be,
}

#[test]
fn by_value() {
  assert_eq!(transmute::<u32, [u8; 4]>(0x01020304), 0x01020304u32.to_ne_bytes());
  assert_eq!(transmute::<[bool; 2], [u8; 2]>([true, false]), [1, 0]);
  assert_eq!(transmute::<Sign, i8>(Sign::Minus), -1);
  assert_eq!(transmute::<u32bec<0xcafe>, [u8; 4]>(Default::default()), [0, 0, 0xca, 0xfe]);
  assert_eq!(transmute::<NonZero<u8>, Option<NonZero<u8>>>(NonZero::new(3).unwrap()), NonZero::new(3));
  assert_eq!(truncate::<u32, [u8; 2]>(u32::from_ne_bytes([1, 2, 3, 4])), [1, 2]);

  assert_eq!(try_transmute::<u8, Level>(1), Ok(Level::Mid));
  assert_eq!(try_transmute::<u8, Level>(3), Err(Error::Validity));
  assert_eq!(try_transmute::<u8, Sign>(0xff), Ok(Sign::Minus));
  assert_eq!(try_transmute::<i8, Sign>(2), Err(Error::Validity));
  assert_eq!(try_transmute::<u16, Wide>(0xfffe), Ok(Wide::B));
  assert_eq!(try_transmute::<[u8; 2], Wide>([0, 0]), Err(Error::Validity));
  assert_eq!(try_transmute::<u32, char>(0x1f980), Ok('🦀'));
  assert_eq!(try_transmute::<u32, char>(0xd800), Err(Error::Validity));
  assert_eq!(try_transmute::<u16, NonZero<u16>>(0), Err(Error::Validity));
  // Whatever the order the compiler chose for the fields.
  let rust = Rust { flag: true, word: 0xdeadbeef, level: Level::High, bytes: [7, 8] };
  assert_eq!(try_transmute::<[u8; 8], Rust>(transmute(rust)), Ok(rust));
  let mut bytes: [u8; 8] = transmute(rust);
  for byte in &mut bytes {
    *byte = 0xff;
  }
  assert_eq!(try_transmute::<[u8; 8], Rust>(bytes), Err(Error::Validity));
}

#[test]
fn by_reference() {
  let mut words = [0x0102u16, 0x0304];
  assert_eq!(transmute_ref::<_, [u8; 4]>(&words).len(), 4);
  assert_eq!(transmute_ref::<[u16], [u8]>(&words[..]).len(), 4);
  assert_eq!(truncate_ref::<[u16; 2], [u8; 3]>(&words).len(), 3);
  assert_eq!(truncate_ref::<[u16; 2], [[u8; 3]]>(&words).len(), 1);
  transmute_mut::<[u16], [[u8; 2]]>(&mut words[..])[1] = [0, 0];
  assert_eq!(words, [0x0102, 0]);
  *try_transmute_mut::<[u16; 2], u32>(&mut words).unwrap() = u32::MAX;
  assert_eq!(words, [0xffff; 2]);

  // A greater alignment: the address is checked.
  let bytes = transmute_ref::<_, [u8; 8]>(&[1u32, 2]);
  assert_eq!(try_transmute_ref::<[u8], [u32]>(&bytes[..]), Ok(&[1u32, 2][..]));
  assert_eq!(try_transmute_ref::<[u8], [u32]>(&bytes[1..5]), Err(Error::Alignment));
  assert_eq!(try_transmute_ref::<[u8], [u32]>(&bytes[..6]), Err(Error::Size));
  assert_eq!(try_transmute_ref::<[u8], u32>(&bytes[4..]), Ok(&2));

  // Trailing padding: 5 bytes of data, 8 bytes of value.
  let padded: &Padded = try_transmute_ref(&bytes[..]).unwrap();
  assert_eq!((padded.word, &padded.tail, size_of_val(padded)), (1, &2u32.to_ne_bytes()[..], 8));
  assert_eq!(try_transmute_ref::<[u8], Padded>(&bytes[..6]), Err(Error::Size));
  assert_eq!(try_transmute_ref::<[u8], Padded>(&bytes[..4]).map(|p| p.tail.len()), Ok(0));
  // Nothing of it is read from padding.
  assert_eq!(as_bytes(padded), bytes);
  let (padded, rest) = unpack::<Padded>(&bytes[..7]).unwrap();
  assert_eq!((padded.tail.len(), rest.len()), (0, 3));

  // Enums, in slices.
  assert_eq!(try_transmute_ref::<[u8], [Level]>(&[0, 1, 2]), Ok(&[Level::Low, Level::Mid, Level::High][..]));
  assert_eq!(try_transmute_ref::<[u8], [Level]>(&[0, 1, 3]), Err(Error::Validity));
  assert_eq!(transmute_ref::<[Level], [u8]>(&[Level::High]), &[2]);
}

#[test]
fn rules() {
  // A number of elements.
  let (list, rest) = unpack::<List>(&[4, 1, 0, 2, 0, 9]).unwrap();
  assert_eq!((list.1.len(), list.1[1].get(), rest), (2, 2, &[9][..]));
  assert_eq!(unpack::<List>(&[6, 1, 0, 2, 0, 9]).err(), Some(Error::Size));
  assert!(try_transmute_ref::<[u8], List>(&[4, 1, 0, 2, 0]).is_ok());
  assert_eq!(try_transmute_ref::<[u8], List>(&[2, 1, 0, 2, 0]).err(), Some(Error::Validity));
  assert_eq!(try_transmute_ref::<[u8], List>(&[4, 1, 0, 2]).err(), Some(Error::Size));

  // An assertion over two fields.
  let checked = |bytes: [u8; 4]| try_transmute::<_, [u8; 4]>(bytes).and_then(|bytes| {
    try_transmute_ref::<[u8; 4], Checked>(&bytes).map(|checked| checked.value.get())
  });
  assert_eq!(checked([1, 0, 0, 5]), Ok(5));
  assert_eq!(checked([2, 0, 1, 0]), Ok(0x100));
  assert_eq!(checked([2, 0, 0, 5]), Err(Error::Validity));
  assert_eq!(checked([3, 0, 1, 0]), Err(Error::Validity));
  assert_eq!(checked([1, 1, 0, 5]), Err(Error::Validity));
  // The same type proves its own rules: no check, and this would not compile otherwise.
  let checked: &Checked = try_transmute_ref(&[1u8, 0, 0, 5]).unwrap();
  assert_eq!(transmute_ref::<Checked, Checked>(checked), checked);
}

#[test]
fn in_const() {
  const BYTES: [u8; 4] = transmute(0x01020304u32.to_be());
  const WORD: &u16be = truncate_ref(&BYTES);
  const LEVEL: Result<Level, Error> = try_transmute(2u8);
  const CHECKED: Result<&Checked, Error> = try_transmute_ref(&[2u8, 0, 1, 0]);
  const LIST: Result<(&List, &[u8]), Error> = unpack(&[2, 7, 0, 1]);
  assert_eq!((BYTES, WORD.get(), LEVEL), ([1, 2, 3, 4], 0x0102, Ok(Level::High)));
  assert_eq!(CHECKED.map(|checked| checked.kind), Ok(2));
  assert_eq!(LIST.map(|(list, rest)| (list.1[0].get(), rest.len())), Ok((7, 1)));
}
