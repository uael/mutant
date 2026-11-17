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
  // (Aligned for the `u32` it is seen as below: nothing tells where a `[u16; 2]` is otherwise,
  // and `try_transmute_mut` would rightly answer `Error::Alignment` half of the time.)
  #[repr(align(4))]
  struct Aligned([u16; 2]);
  let mut words = Aligned([0x0102u16, 0x0304]).0;
  let mut aligned = Aligned(words);
  assert_eq!(transmute_ref::<_, [u8; 4]>(&words).len(), 4);
  assert_eq!(transmute_ref::<[u16], [u8]>(&words[..]).len(), 4);
  assert_eq!(truncate_ref::<[u16; 2], [u8; 3]>(&words).len(), 3);
  assert_eq!(truncate_ref::<[u16; 2], [[u8; 3]]>(&words).len(), 1);
  transmute_mut::<[u16], [[u8; 2]]>(&mut words[..])[1] = [0, 0];
  assert_eq!(words, [0x0102, 0]);
  *try_transmute_mut::<[u16; 2], u32>(&mut aligned.0).unwrap() = u32::MAX;
  assert_eq!(aligned.0, [0xffff; 2]);
  // The second of two `u16`s that are where a `u32` can be is not where a `u32` can be.
  let mut pairs = Aligned([0; 2]);
  let second = try_transmute_mut::<[u16], [u8; 2]>(&mut pairs.0[1..]).unwrap();
  assert!(try_transmute_ref::<[u8; 2], u16>(second).is_ok());
  assert_eq!(try_transmute_ref::<[u8], [u32]>(&second[..]).err(), Some(Error::Alignment));

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

/// A header, a body, and the sum of both: for a tail that is a slice or an array.
#[derive(Data)]
#[repr(C)]
#[data(assert(sum16(0, size_of_val(self) - 2) == be(size_of_val(self) - 2, 2)))]
#[data(get(kind: u8 = self.0 >> 4, flag: bool = self.0 & 1 == 1, twice: u32 = self.0 * 2))]
struct Summed<T: ?Sized>(u8, #[data(regions(body = size_of_val(self) - 3, sum))] T);

#[derive(Data)]
#[repr(C)]
struct Counted<T: ?Sized>(u8, #[data(len = self.0)] T);

#[test]
fn rules_on_sized_and_unsized() {
  // `0x51`, `0x02 0x03`: the words `0x5102` and `0x0300` sum to `0x5402`.
  let bytes: [u8; 5] = [0x51, 0x02, 0x03, 0x54, 0x02];
  let summed: &Summed<[u8]> = try_transmute_ref(&bytes[..]).unwrap();
  assert_eq!((summed.kind(), summed.flag(), summed.twice()), (5, true, 0xa2));
  assert_eq!((summed.body(), summed.sum()), (&[2, 3][..], &[0x54, 2][..]));
  assert!(try_transmute_ref::<_, Summed<[u8]>>(&[0x51u8, 0x02, 0x03, 0x54, 0x03][..]).is_err());
  assert!(try_transmute_ref::<_, Summed<[u8]>>(&[0x51u8][..]).is_err());
  // The same, of a known size.
  let summed: &Summed<[u8; 4]> = try_transmute_ref(&bytes).unwrap();
  assert_eq!((summed.body(), summed.sum(), summed.kind()), (&[2, 3][..], &[0x54, 2][..], 5));
  assert!(try_transmute_ref::<_, Summed<[u8; 4]>>(&[0x51u8, 0x02, 0x03, 0x54, 0x03]).is_err());

  // A count of elements, of a slice or of an array.
  assert!(try_transmute_ref::<_, Counted<[u8]>>(&[2u8, 7, 8][..]).is_ok());
  assert!(try_transmute_ref::<_, Counted<[u8]>>(&[3u8, 7, 8][..]).is_err());
  assert!(try_transmute_ref::<_, Counted<[u8; 2]>>(&[2u8, 7, 8]).is_ok());
  assert!(try_transmute_ref::<_, Counted<[u8; 2]>>(&[1u8, 7, 8]).is_err());
  let (counted, rest) = unpack::<Counted<[u8]>>(&[1, 7, 8]).unwrap();
  assert_eq!((&counted.1, rest), (&[7][..], &[8][..]));

  assert_eq!(mutant::crc32(b"123456789"), 0xcbf43926);
  assert_eq!(mutant::sum16(&[0xff, 0xff, 0x00, 0x01, 0x80]), 0x8001);
}

/// What `&&` and `||` do not need may be out of the value.
#[derive(Data)]
#[repr(C)]
#[data(assert(self.0 != 7 || be(3, 2) == 0xcafe))]
#[data(assert(self.0 == 7 && be(3, 2) == 0xcafe || 10 - self.0 >= 3))]
struct Lazy(u8, [u8]);

#[test]
fn what_is_not_needed_is_not_read() {
  let lazy = |bytes: &'static [u8]| try_transmute_ref::<[u8], Lazy>(bytes).is_ok();
  // Not of kind 7: nothing is read at 3, where there is nothing.
  assert!(lazy(&[1]));
  assert!(lazy(&[1, 0, 0, 0xff, 0xff]));
  // Of kind 7: it is.
  assert!(lazy(&[7, 0, 0, 0xca, 0xfe]));
  assert!(!lazy(&[7, 0, 0, 0xca, 0xff]));
  assert!(!lazy(&[7, 0, 0, 0xca]));
  assert!(!lazy(&[7]));
  // `10 - 11` cannot be computed, and nothing else decides.
  assert!(!lazy(&[11]));
  assert!(!lazy(&[8]));
}
