//! Sizes, lengths and regions read from the value: `be`, `le` and `walk` anywhere, several walks,
//! walks in walks.

use puns::{Error, Pun, from_bytes, from_head};

/// The length of a WebSocket frame: 7 bits, or 16 or 64 more if they are 126 or 127.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + if length & 0x7f == 126 {
  2 + be(2, 2)
} else if length & 0x7f == 127 {
  8 + be(2, 8)
} else {
  length & 0x7f
})]
struct Frame {
  head: u8,
  length: u8,
  rest: [u8],
}

#[test]
fn a_size_read_from_the_value() {
  let (frame, rest) = from_head::<Frame>(&[0x81, 3, 1, 2, 3, 9]).unwrap();
  assert_eq!((&frame.rest, rest), (&[1, 2, 3][..], &[9][..]));
  let (frame, rest) = from_head::<Frame>(&[0x81, 126, 0, 2, 1, 2, 9]).unwrap();
  assert_eq!((&frame.rest, rest), (&[0, 2, 1, 2][..], &[9][..]));
  let mut long = vec![0x82, 127, 0, 0, 0, 0, 0, 0, 1, 0];
  long.extend(core::iter::repeat_n(7, 256));
  let (frame, rest) = from_head::<Frame>(&long).unwrap();
  assert_eq!((frame.rest.len(), rest.len()), (264, 0));
  // What the length says is not there.
  assert_eq!(from_head::<Frame>(&[0x81, 126, 0, 9, 1]).err(), Some(Error::Size));
  // The length itself is not there: it cannot be read.
  assert_eq!(from_head::<Frame>(&[0x81, 126, 0]).err(), Some(Error::Validity));
  // The same rule, checked against the bytes that are the value.
  assert!(from_bytes::<Frame>(&[0x81, 126, 0, 2, 1, 2]).is_ok());
  assert!(from_bytes::<Frame>(&[0x81, 126, 0, 2, 1, 2, 3]).is_err());
}

/// A string that ends with a zero, and what follows it: a walk in a size, in a region.
#[derive(Pun)]
#[repr(C)]
#[pun(size = walk(0, |at| be(at, 1) != 0, |at| at + 1) + 1)]
struct CString {
  bytes: [u8],
}

#[derive(Pun)]
#[repr(C)]
struct Named {
  kind: u8,
  // The walk gives an offset in the value, the region a number of bytes after `kind`.
  #[pun(regions(name = walk(1, |at| be(at, 1) != 0, |at| at + 1) - 1, terminator = 1, value))]
  rest: [u8],
}

#[test]
fn a_walk_in_a_size_and_in_a_region() {
  let (string, rest) = from_head::<CString>(b"abc\0def").unwrap();
  assert_eq!((&string.bytes, rest), (&b"abc\0"[..], &b"def"[..]));
  assert_eq!(from_head::<CString>(b"abc").err(), Some(Error::Validity));
  let named = from_bytes::<Named>(b"\x07key\0value").unwrap();
  assert_eq!((named.kind, named.name(), named.value()), (7, &b"key"[..], &b"value"[..]));
}

/// Two walks, and a walk in a walk: "type, length, value"s whose values are themselves made of
/// "length, value"s, down to a type 0. Each walk adds the lengths that the other one does not.
#[derive(Pun)]
#[repr(C)]
struct Twice {
  count: u8,
  #[pun(len = walk(0, |at| at < count, |at| at + 1) + walk(0, |at| at < count, |at| at + 1))]
  items: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(
  walk(0, |at| be(at, 1) != 0, |at| {
    let end = at + 2 + be(at + 1, 1);
    let inner = walk(at + 2, |i| i < end, |i| i + 1 + be(i, 1));
    if inner == end { end } else { 1 / 0 }
  }) + 1 == size_of_val(self)
))]
struct Nested {
  values: [u8],
}

#[test]
fn walks_and_walks_in_walks() {
  let (twice, rest) = from_head::<Twice>(&[2, 1, 2, 3, 4, 5]).unwrap();
  assert_eq!((&twice.items, rest), (&[1, 2, 3, 4][..], &[5][..]));
  // Type 1, 4 bytes: "1 byte: 7", "1 byte: 8"; type 2, 0 bytes; the end.
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 1, 8, 2, 0, 0]).is_ok());
  // A value whose inner lengths do not fill it.
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 2, 8, 2, 0, 0]).is_err());
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 1, 8, 2, 0]).is_err());
}

/// A step of 70 operations, and an expression of more than 80: longer than what the code of the
/// walks of a struct applies, so the evaluator applies them.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < 4, |at| at + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
  + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 - 33)
  == a + a + a + a))]
struct LongStep {
  a: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < a, |at| at + 1) + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0
  + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0
  + 0 + 0 + 0 + 0 + 0 == a))]
struct LongExpression {
  a: u8,
}

#[test]
fn long_walks() {
  assert!(from_bytes::<LongStep>(&[1]).is_ok());
  assert!(from_bytes::<LongStep>(&[2]).is_err());
  assert!(from_bytes::<LongExpression>(&[3]).is_ok());
  // More steps than 8 per bit of the value, and 128.
  assert!(from_bytes::<LongExpression>(&[200]).is_err());
}
