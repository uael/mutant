//! Patterns that a struct requires of its bytes (`matches`), and numbers of a given byte order.

use puns::{
  Pun, from_bytes, from_head,
  layout::{Dfa, DfaState},
  num::{f64be, i16be, i32le},
};

/// A tar header, reduced: a name of ASCII, a size of octal digits ended by a space or a zero.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(Dfa::ASCII, 0, 8)))]
#[pun(assert(matches(Dfa::OCTAL, 8, 19) && (be(19, 1) == 0 || be(19, 1) == 32)))]
struct Tar {
  name: [u8; 8],
  size: [u8; 12],
}

/// An MQTT string: a length, then UTF-8 without `U+0000`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + length)]
#[pun(assert(matches(Dfa::UTF8_NO_NUL, 2, size_of_val(self))))]
struct MqttString {
  length: puns::num::u16be,
  text: [u8],
}

/// An identifier: a letter or `_`, then letters, digits or `_`. A pattern of the user.
const IDENTIFIER: &Dfa = &Dfa {
  id: 1000,
  states: &[
    DfaState { accept: false, edges: &[(b'A', b'Z', 1), (b'_', b'_', 1), (b'a', b'z', 1)] },
    DfaState {
      accept: true,
      edges: &[(b'0', b'9', 1), (b'A', b'Z', 1), (b'_', b'_', 1), (b'a', b'z', 1)],
    },
  ],
};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(IDENTIFIER, 0, size_of_val(self))))]
struct Identifier {
  bytes: [u8],
}

#[test]
fn patterns() {
  assert!(from_bytes::<Tar>(b"file.txt00000000017\0").is_ok());
  assert!(from_bytes::<Tar>(b"file.txt00000000017 ").is_ok());
  assert!(from_bytes::<Tar>(b"file.txt00000000018\0").is_err());
  assert!(from_bytes::<Tar>(b"fil\xe9.txt00000000017\0").is_err());

  let (string, rest) = from_head::<MqttString>(b"\0\x04ab\xc3\xa9!").unwrap();
  assert_eq!((&string.text, rest), (&b"ab\xc3\xa9"[..], &b"!"[..]));
  // Half of `é`, or a zero.
  assert!(from_head::<MqttString>(b"\0\x03ab\xc3\xa9").is_err());
  assert!(from_head::<MqttString>(b"\0\x02a\0").is_err());

  assert!(from_bytes::<Identifier>(b"_puns2").is_ok());
  assert!(from_bytes::<Identifier>(b"2puns").is_err());
  assert!(from_bytes::<Identifier>(b"").is_err());
}

/// A bitmap's height is signed (negative: top to bottom), a GPS position is a big-endian `f64`.
#[derive(Pun)]
#[repr(C)]
struct Numbers {
  height: i32le,
  latitude: f64be,
  offset: i16be,
}

#[test]
fn numbers_of_a_byte_order() {
  let mut bytes = (-3i32).to_le_bytes().to_vec();
  bytes.extend(48.85f64.to_be_bytes());
  bytes.extend((-2i16).to_be_bytes());
  let numbers = from_bytes::<Numbers>(&bytes).unwrap();
  assert_eq!((numbers.height.get(), numbers.latitude.get(), numbers.offset.get()), (-3, 48.85, -2));
  assert_eq!((i32le::new(-3).to_bytes(), i16be::from(-2).get()), ([0xfd, 0xff, 0xff, 0xff], -2));
}
