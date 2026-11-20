# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)][mit]

**Type punning, proven at compile time.** `puns` reads a value of one type as a value of another
type, in place, without `unsafe`. When the program is built, it compares the two types byte by
byte. Each pun is then **proven** and compiles to nothing, **checked** at run time for what could
not be proven, or **refused** with a compile error that says why.

## A tour in one example

A dungeon game saves its hero to a file. Version 2 of the game adds keys, and must still load the
saves of version 1. Every type below is declared once, and `#[derive(Pun)]` is all it needs.

```rust
use core::num::NonZero;

use puns::{Pun, num::{u8c, u16le, u32bec}};

/// An item, as version 1 of a dungeon game saves it: a tag, then two bytes.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0, // little-endian, at any address
  Potion { heal: u8, cursed: bool } = 1,
}

/// Version 2 adds keys. A key without a door opens every door of its floor.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: NonZero<u8>, door: Option<NonZero<u8>> } = 2,
}

// Proven: an item is its three bytes. Reading them compiles to no instruction.
assert_eq!(puns::as_bytes(&OldItem::Sword { damage: 300.into() }), [0, 0x2c, 0x01]);
// Checked: three bytes are an item if the tag is one, and here if the floor is not 0.
let key: &Item = puns::from_bytes(&[2, 1, 0]).unwrap();
// Proven: the constant 1 is not 0, so it is a `NonZero`, without `unwrap`. And door 0 is `None`.
assert_eq!(key, &Item::Key { floor: puns::pun(u8c::<1>::new()), door: None });
// A mimic: there is no floor 0.
assert_eq!(puns::from_bytes::<Item>(&[2, 0, 0]), Err(puns::Error::Validity));

/// A save file of version `V`, whose bag is a `B`. The declaration is the parser.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // 10 hp per level, no more
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // "SAVE"
  version: V,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: B, // to the end of the file
}

// Version 1 saves the game. Proven: a save is its bytes, so writing it is copying them.
let bag = [OldItem::Sword { damage: 9.into() }, OldItem::Potion { heal: 25, cursed: false }];
let old = Save { magic: u32bec::new(), version: u8c::<1>::new(), level: 7, hp: 42.into(), bag };
let mut file = puns::as_bytes(&old).to_vec();
// Proven: a save starts with four bytes. The magic number is their big-endian value.
assert_eq!(puns::head_ref::<_, [u8; 4]>(&old), b"SAVE");
// The hero gains a level, right in the file, through a typed `&mut` to its bytes.
puns::from_bytes_mut::<Save<u8c<1>, [OldItem]>>(&mut file).unwrap().level += 1;

/// A save file is one version or the other: the first that its bytes are.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

/// Loads a save of either version as a save of today.
fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  // Checked, once: the magic, the version, the level, the rule, each item.
  Ok(match SaveFile::from_bytes(file)? {
    SaveFile::V1(old) => puns::pun_ref(old), // Proven: an old item is an item. No instruction.
    SaveFile::V2(new) => puns::pun_ref(new), // Proven: a 2 is a `u8`.
  })
}

// Version 2 of the game loads the old save, with the level gained in the file.
assert_eq!(load(&file).unwrap().level, 8);
// The hero takes the key: three more bytes. Version 1 has no keys: this is not a save any more.
file.extend_from_slice(puns::as_bytes(key));
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
// Upgrading it is writing one byte, since every old item is an item.
puns::from_bytes_mut::<Save>(&mut file).unwrap().version = 2;
let save = load(&file).unwrap();
let upgraded = [Item::Sword { damage: 9.into() }, Item::Potion { heal: 25, cursed: false }];
assert_eq!((&save.bag[..2], &save.bag[2]), (&upgraded[..], key));
// Checked: the bag goes back to version 1 only without a key. This one has one.
assert_eq!(puns::try_pun_ref::<_, [OldItem]>(&save.bag).err(), Some(puns::Error::Validity));
// Refused: `puns::pun_mut::<[OldItem], [Item]>(..)` does not compile, a key could be put in.
// A cheater gives the hero 999 hp. The edit compiles: the rule is checked when bytes become
// a `Save`, so the save does not load any more.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```

In this example, `puns` never parses anything. The declarations are the file format, and every
read happens where the bytes lie.

## Proven, checked, refused

`puns` decides what each pun costs when the program is built.

- **Proven.** Every value of the source is a [valid] value of the target. The pun compiles to no
  instruction: `as_bytes`, `head_ref` and `pun_ref` in the tour.
- **Checked.** Only some values of the source are valid targets. A `try_` or `from_` function
  checks at run time what the types could not prove, and nothing else. It returns an error for
  bytes that are not a valid target, and never panics.
- **Refused.** The pun can never succeed, would read [padding], or would let safe code write an
  invalid value through a `&mut`. It does not compile.

A refusal is raised when a generic function is monomorphized, so `cargo build` reports it and
`cargo check` does not. [Compile-time errors](https://docs.rs/puns/latest/puns/#compile-time-errors)
explains why.

## A network packet, as its declaration

A format often says more than the types of its fields: a size stored in a header, a checksum, a
value that tells what follows. These facts are attributes of the derive. The example below reads
UDP over IPv4 over Ethernet with no parsing code: the declarations are the format.

```rust
use puns::{
  Pun, Tag,
  num::{u8c, u16be, u16bec},
};

/// An Ethernet frame. A `Frame` carries bytes, whatever its EtherType; a `Frame<Ipv4>` is a
/// frame whose EtherType is the one of IPv4.
#[derive(Pun)]
#[repr(C)]
struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  destination: [u8; 6],
  source: [u8; 6],
  ether_type: P::Tag,
  payload: P,
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct EtherType(u16be);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0800>))] // the EtherType it comes with
#[pun(size = total_length)] // its size is in its header
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))] // the checksum of the header
#[pun(get(header_length: usize = (version_ihl & 0x0f) * 4))] // a bit field, as a method
struct Ipv4<P: ?Sized + Tag<Protocol> = [u8]> {
  #[pun(valid(0x45..=0x4f))] // version 4, a header of 5 to 15 words
  version_ihl: u8,
  type_of_service: u8,
  total_length: u16be,
  identification: u16be,
  flags_fragment_offset: u16be,
  time_to_live: u8,
  protocol: P::Tag,
  checksum: u16be,
  source: [u8; 4],
  destination: [u8; 4],
  // The options, whose size is in the header too, then what the packet carries.
  #[pun(regions(options = (version_ihl & 0x0f) * 4 - 20, payload: P))]
  data: [u8],
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct Protocol(u8);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<17>))]
#[pun(size = length)]
struct Udp {
  source_port: u16be,
  destination_port: u16be,
  length: u16be,
  checksum: u16be,
  payload: [u8],
}

/// A frame is one thing or another: the first of these that the bytes are.
#[derive(Pun)]
enum Packet<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Ipv4(&'a Frame<Ipv4>),
  Other(&'a Frame),
}

let bytes: [u8; 47] = [
  2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x08, 0x00, // Ethernet: to, from, the EtherType of IPv4
  0x45, 0, 0, 33, 0, 1, 0x40, 0, 64, 17, 0x26, 0xc9, 10, 0, 0, 1, 10, 0, 0, 2, // IPv4
  0x30, 0x39, 0, 53, 0, 13, 0, 0, b'h', b'e', b'l', b'l', b'o', // UDP, to port 53
];

// One call checks the EtherType, the version, the sizes, the checksum of the header and the
// protocol. The frame is read where it is: nothing is copied, nothing is allocated.
let frame: &Frame<Ipv4<Udp>> = puns::from_bytes(&bytes).unwrap();
let ip = &frame.payload;
assert_eq!((ip.header_length(), ip.time_to_live), (20, 64));
assert!(ip.options().is_empty());
let udp = ip.payload().unwrap();
assert_eq!((udp.destination_port.get(), &udp.payload), (53, &b"hello"[..]));

// What is known is not checked again. A frame that carries UDP is a frame: no instruction.
let any: &Frame = puns::pun_ref(frame);
assert_eq!(any.payload.len(), 33);
// And an IPv4 frame of any protocol: the rules of `Ipv4<Udp>` (its size, its checksum) are those
// of `Ipv4`, over the same bytes, so they prove them. No instruction either.
let ip: &Frame<Ipv4> = puns::pun_ref(frame);
assert_eq!(ip.payload.header_length(), 20);

// One bit of the header is flipped: its checksum does not hold, this is not IPv4.
let mut corrupted = bytes;
corrupted[22] ^= 1;
assert_eq!(puns::from_bytes::<Frame<Ipv4>>(&corrupted).err(), Some(puns::Error::Validity));
assert!(matches!(Packet::from_bytes(&corrupted), Ok(Packet::Other(_))));
assert!(matches!(Packet::from_bytes(&bytes), Ok(Packet::Udp(_))));

// From a stream: the header tells where the packet ends, `from_head` returns what follows.
let stream = [&bytes[14..], &[0xaa, 0xbb]].concat();
let (packet, rest) = puns::from_head::<Ipv4>(&stream).unwrap();
assert_eq!((size_of_val(packet), rest), (33, &[0xaa, 0xbb][..]));
```

| What the format says                                    | Told by                                      |
|---------------------------------------------------------|----------------------------------------------|
| numbers are big-endian                                  | `u16be`                                      |
| the EtherType tells what the frame carries              | `P::Tag`, `tag(EtherType = u16bec<0x0800>)`  |
| version 4, a header of 5 to 15 words                    | `valid(0x45..=0x4f)`                         |
| the size of a packet is in its header                   | `size = total_length`                        |
| the header has a checksum                               | `assert(sum16(..) == 0xffff)`                |
| options, whose size is in the header, then the payload  | `regions(options = .., payload: P)`          |
| bit fields                                              | `get(header_length: usize = ..)`             |
| a frame is one thing or another                         | an enum of references                        |

[`puns/examples/ethernet.rs`][ethernet] is the complete version. It adds the checksums of UDP
and TCP, IPv6 behind any chain of extension headers, VLAN tags, and the frame check sequence.

## What the derive is told

| Attribute or type                          | Meaning                                                    |
|--------------------------------------------|------------------------------------------------------------|
| `u16be`, `u32le`, `i64be`, `f32le`...      | the byte order of a field (types of `puns::num`)           |
| `u8c<17>`, `u32bec<0x89504e47>`...         | a field that has one value: a magic number, a tag          |
| `#[pun(valid(1..=9, 0x80))]`               | the values that an integer field takes                     |
| `#[pun(size = EXPR)]`                      | the size of the value, in bytes                            |
| `#[pun(len = EXPR)]`                       | the number of elements of the slice that ends it           |
| `#[pun(assert(EXPR))]`                     | anything else: bits, relations between fields, checksums   |
| `#[pun(regions(a = EXPR, b: Type))]`       | consecutive regions of the tail, and a method for each     |
| `#[pun(get(name: Type = EXPR))]`           | a method that computes from fields: a bit field            |
| `#[pun(tag(Key = Type))]`                  | the value of the `Key` that comes with this type           |
| an enum whose variants hold references     | one thing or another: `from_bytes`, `from_head`            |

An `EXPR` is a Rust expression over the unsigned integer fields of the struct and
`size_of_val(self)`. It may use operators, `if`, `let` and constants. It may also read the bytes
of the value with these functions.

- `le(offset, n)` and `be(offset, n)` read the integer of `n` bytes at `offset`.
- `sum16`, `crc32`, `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>`, `fletcher16`, `adler32` and `sum`
  compute checksums over a range of bytes.
- `leb128`, `quic` and `vlq` read variable-length integers.
- `matches(DFA, start, end)` matches a pattern of bytes: ASCII, digits, UTF-8, or your own
  automaton.
- `walk(start, |at| condition, |at| next)` follows a chain step by step, such as a chain of
  headers or a list of options.

Where Rust would panic, for example on an overflow, a division by zero or a read past the end,
the rule does not hold. A rule cannot fail in any other way. The
[documentation of the derive](https://docs.rs/puns/latest/puns/derive.Pun.html) is the complete
reference.

## The functions

The functions differ by what they read, a typed value or bytes, and by how much of it.

|               | Whole                                   | Front                          | Back                           |
|---------------|-----------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut`, `try_pun*` | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `as_bytes`                | `from_head`                    | `from_tail`                    |

- **A typed value** is read by value (`pun`), by reference (`pun_ref`) or by mutable reference
  (`pun_mut`). These functions are proven or refused. `try_pun`, `try_pun_ref` and `try_pun_mut`
  are checked and return a `Result`.
- **Bytes** are always checked: the size, the address, and whatever the bytes do not prove about
  the type. `from_bytes` requires all the bytes to form the value. `from_head` and `from_tail`
  take a value from the front or the back and return the remaining bytes. Each of the three has a
  `_mut` form. `as_bytes` goes the other way and is proven.
- **A part** is the first or last bytes of a value. For example, `head_ref::<Ipv4, [u8; 20]>`
  reads the fixed header of a packet.

```rust
let mut bytes = [0u8, 1, 2, 3, 4];
let (first, rest) = puns::from_head::<[u8; 2]>(&bytes).unwrap();
let (_, last) = puns::from_tail::<[u8; 2]>(rest).unwrap();
assert_eq!((first, last), (&[0, 1], &[3, 4]));

assert_eq!(puns::tail::<[u8; 5], [u8; 2]>(bytes), [3, 4]);
*puns::head_mut::<_, [u8; 2]>(&mut bytes) = [7, 7];
assert_eq!(bytes, [7, 7, 2, 3, 4]);

// By value, sizes are equal; by reference, a slice is as long as its bytes allow.
assert_eq!(puns::pun::<[bool; 4], u32>([true; 4]), 0x0101_0101);
assert_eq!(puns::pun_ref::<[u16], [[u8; 2]]>(&[1, 2, 3]).len(), 3);
assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("café"));
```

All of these functions are `const fn`.

## Through `&mut`, both ways

A `&mut` lets safe code write any value of the target, which the source then holds. So a pun by
`&mut` needs more than a pun by `&`: each type must accept every value of the other.

```rust
// Two pairs of `bool`s are four `bool`s, and the other way: proven both ways.
let mut flags = [true, false, true, true];
let pairs: &mut [[bool; 2]; 2] = puns::pun_mut(&mut flags);
pairs[1] = [false, false];
assert_eq!(flags, [true, false, false, false]);
```

```rust,compile_fail,E0080
// Refused: every `bool` is a `u8`, but a `u8` written through the `&mut` could be 2.
let mut flag = true;
let byte: &mut u8 = puns::pun_mut(&mut flag);
```

## What is refused

A refusal is a compile error. It says why the pun is refused, at which byte, and often what to
write instead. Here is a classic mistake with `transmute`: reading a struct with padding as bytes.

```rust,compile_fail,E0080
#[derive(puns::Pun)]
#[repr(C)]
struct Header {
  kind: u8,
  length: u32,
}

let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
```

```text
error[E0080]: evaluation panicked:
              cannot pun: a byte that the target requires to be initialized is padding in the source.
              This is at byte 1 of the target.

   --> puns-0.1.0/src/lib.rs
    |
    |   const BY_REF: () = Self::require(Self::check(Need::REF));
    |                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ evaluation of `puns::Plan::<Header, [u8; 8]>::BY_REF` failed inside this call
    |
note: inside `puns::Plan::<Header, [u8; 8]>::require`
   --> puns-0.1.0/src/lib.rs
    |
    |       message.panic();
    |       ^^^^^^^^^^^^^^^
note: inside `puns::Message::panic`
   --> core/src/panic.rs
    |
    = note: the failure occurred here

note: erroneous constant encountered
   --> puns-0.1.0/src/lib.rs
    |
    |   const { Plan::<S, D>::BY_REF };
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

note: the above error was encountered while instantiating `fn pun_ref::<Header, [u8; 8]>`
  --> src/main.rs:10:25
   |
10 |   let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
   |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The first lines say why the pun is refused and where in the type. Byte 1 is the first of the
three bytes of padding after `kind`. The last note says where in your code. Between them, `rustc`
shows the constant of `puns` whose evaluation failed, with the two types it was evaluated for.
The fix is to declare those bytes as a field, so that they are initialized.

```rust
#[derive(puns::Pun)]
#[repr(C)]
struct Header {
  kind: u8,
  reserved: [u8; 3],
  length: u32,
}

let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, reserved: [0; 3], length: 2 });
assert_eq!(bytes[..4], [1, 0, 0, 0]);
```

The other refusals follow, each with its message and with code that works.

**A target that needs a greater alignment than the source.** A `[u8; 4]` can be at any address,
but a `u32` cannot.

```rust,compile_fail,E0080
let n: &u32 = puns::pun_ref(&[0u8; 4]);
```

```text
cannot pun: the target has a greater alignment than the source.
Use `try_pun_ref` or `try_pun_mut` to check the address at run time.
```

```rust
use puns::num::u32le;

// By value there is no address: the four bytes are copied into a `u32`.
assert_eq!(puns::pun::<[u8; 4], u32>([1, 0, 0, 0]), u32::from_ne_bytes([1, 0, 0, 0]));
// By reference, to a type that has no alignment: a `u32le` is four bytes.
assert_eq!(puns::pun_ref::<[u8; 4], u32le>(&[1, 0, 0, 0]).get(), 1);
// Or the address is checked at run time: it is aligned, or it is not.
match puns::try_pun_ref::<[u8; 4], u32>(&[0; 4]) {
  Ok(n) => assert_eq!(*n, 0),
  Err(error) => assert_eq!(error, puns::Error::Alignment),
}
```

**A value that may be invalid, without a check.**

```rust,compile_fail,E0080
let text: &str = puns::pun_ref(&[0xffu8; 4]);
```

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

```rust
assert_eq!(puns::try_pun_ref::<[u8; 4], str>(b"text"), Ok("text"));
assert_eq!(puns::try_pun_ref::<[u8; 4], str>(&[0xff; 4]), Err(puns::Error::Validity));
```

**A `&mut` through which an invalid value could be written.** Here, the value would be bytes
that are not UTF-8.

```rust,compile_fail,E0080
let mut text = String::from("text");
let bytes: &mut [u8] = puns::pun_mut(text.as_mut_str());
```

```text
cannot pun: some values of the target are not valid for the source (or are padding where the
source is not), and could be written through the `&mut`.
```

```rust
// By shared reference nothing is written: every `str` is bytes.
let text = String::from("text");
let bytes: &[u8] = puns::pun_ref(text.as_str());
assert_eq!(bytes, b"text");
```

**An enum read as bytes.** `puns` knows where its padding is.

```rust,compile_fail,E0080
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Small {
  A(u8),
  B(u16),
}

let bytes: [u8; 4] = puns::pun(Small::B(1));
```

```text
cannot pun: a byte that the target requires to be initialized is padding in the source.
This is at byte 1 of the target.
```

**A check that can never succeed.** A byte that is always 2 is never a `bool`.

```rust,compile_fail,E0080
let never = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
```

```text
cannot pun: no value of the source is valid for the target.
This is at byte 0 of the target.
```

A refusal is raised when a generic function is monomorphized. **`cargo build` reports it, but
`cargo check` does not.** [Compile-time errors](https://docs.rs/puns/latest/puns/#compile-time-errors)
explains why.

## Layouts that Rust leaves to the compiler, and the `unstable` feature

Rust leaves some layouts to the compiler: the order of the fields of a struct or tuple without
`repr`, and the place and values of the tag of an enum without `repr`. The next version of `rustc`
may choose differently, and `-Zrandomize-layout` chooses at random. A pun is always correct for
the build that compiles it. Without the `unstable` feature, `puns` also guarantees that
**`-Zrandomize-layout` changes nothing that `puns` does**. A pun compiles only if what it
compiles to, what it checks and which values it accepts are the same under every layout that Rust
allows. Only the message of a refusal may differ.

This depends on the pun, not only on the type. The bytes of a `(bool, u8)` are initialized
whichever byte holds the `bool`. But bytes read as a `(bool, u8)` would need a check of one byte
or the other, and which one is not known.

```rust
// Bytes of any value, or both `bool`s, in either order.
assert_eq!(puns::pun::<[u8; 2], (u8, u8)>([7, 7]), (7, 7));
assert!(puns::try_pun::<[u8; 2], (bool, bool)>([1, 2]).is_err());
// Initialized bytes, wherever the `bool` is.
let _: &[u8] = puns::as_bytes(&(true, 1u8));
// The `bool` may be either byte: two `bool`s prove it wherever it is.
let _: (bool, u8) = puns::pun([true, false]);
// A value that `core` documents: `None` is zeros.
assert_eq!(puns::pun::<u32, Option<core::num::NonZero<u32>>>(0), None);
```

```rust,ignore
// Without `unstable`, this does not compile: the byte that must be 0 or 1 is one or the other.
let _ = puns::try_pun::<[u8; 2], (bool, u8)>([1, 1]);
```

With `features = ["unstable"]`, `puns` uses the layout that `rustc` chose for this build. Every pun
that compiles without the feature compiles with it and behaves the same, and more puns compile.
[`puns/DESIGN.md`][design] lists the layouts that Rust specifies, with their sources.

With the `unstable` feature, enums without `repr`, `Option` included, are supported too. Their
layout is found by experiments when the program is built, rather than declared.

```rust
# #[cfg(feature = "unstable")]
# fn main() {
use core::num::NonZero;

use puns::Pun;

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
enum Ticket {
  Free,
  Paid(NonZero<u32>),
}

// A niche: `Free` is the 0 that no `NonZero` is. Every `u32` is a `Ticket`, both ways proven.
assert_eq!(puns::pun::<Ticket, u32>(Ticket::Free), 0);
assert_eq!(puns::pun::<u32, Ticket>(7), Ticket::Paid(NonZero::new(7).unwrap()));
assert_eq!(puns::pun::<u32, Option<NonZero<u32>>>(7), NonZero::new(7));

// Not every byte is an `Option<bool>`: 0 and 1 are `Some`, `None` is where `rustc` put it.
let none: u8 = puns::pun(None::<bool>);
assert_eq!(puns::try_pun::<u8, Option<bool>>(none), Ok(None));
assert_eq!(puns::try_pun::<u8, Option<bool>>(1), Ok(Some(true)));
assert!(puns::try_pun::<u8, Option<bool>>(none + 1).is_err());
# }
# #[cfg(not(feature = "unstable"))]
# fn main() {}
```

`puns` knows a type only by its bytes. Two consequences follow, by design. Types that differ only
by a `PhantomData` or a lifetime convert to each other freely. Deriving `Pun` makes every field as
accessible as a public one. A type whose parameter is a brand, or whose private fields keep an
invariant that the derive is not told about, must not derive `Pun`. See
[What deriving `Pun` means](https://docs.rs/puns/latest/puns/#what-deriving-pun-means).

## Performance

The cost of a pun is decided when the program is built. A proven pun compiles to no instruction,
because the analysis ran in the compiler. A checked pun runs only the checks that the pair of
types requires. Reading three bytes as an `Item` of the tour checks its tag, then the `bool` of a
potion or the floor of a key, and nothing else.

### Machine code

On x86-64 with `opt-level = 3`. The functions and the complete table are in
[`puns/examples/codegen.rs`][codegen]. There, `Position` is a struct of a `u8` and a `bool`, and
`Shape` and `Drawing` are two enums like `OldItem` and `Item` of the tour.

| Function                                           | Machine code                         |
|----------------------------------------------------|--------------------------------------|
| `&Position -> &[u8; 2]`                            | `mov rax, rdi; ret`                  |
| `[bool; 4] -> u32`                                 | `mov eax, edi; ret`                  |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`    |
| `&Frame<Ipv4> -> &[u8]`                            | `lea rdx, [rsi + 34]` (the length)   |
| `&Frame<Ipv4<Udp>> -> &Frame<Ipv4>` (same rules)   | `mov rdx, rsi; mov rax, rdi; ret`    |
| `&[u8; 2] -> Option<&Position>`                    | one `cmp byte ptr [rdi + 1], 2`      |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool` |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 29 instructions, no loop, no call    |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, without a branch |

### Time

[`puns/benches/checks.rs`][checks] times each check next to the same check written by hand, with
the same algorithm for the checksums (`cargo bench -p puns --features unstable --bench checks`).
The numbers are nanoseconds per check, in release with LTO, on a Xeon at 2.1 GHz. Each number is
the median of 5 runs, and each run is the best of 7 samples. The rows for IPv4 in 64 bytes, IPv6
behind 0 and 3 headers, and the view enum are the median of 3 runs.

| Check                                               | `puns` | by hand | ratio |
|-----------------------------------------------------|-------:|--------:|------:|
| IPv4, UDP, 64 bytes (two checksums)                 |     11 |      10 |   1.1 |
| IPv4, UDP, 1472 bytes                               |     53 |      51 |   1.0 |
| IPv4, TCP, 1460 bytes                               |     53 |      53 |   1.0 |
| IPv6, TCP, no extension header                      |    1.1 |     1.5 |   0.8 |
| IPv6, TCP behind 3 extension headers                |    3.4 |     4.8 |   0.7 |
| IPv6, TCP behind 12 extension headers               |     12 |      15 |   0.7 |
| IPv6, UDP, 1400 bytes (its checksum)                |     50 |      42 |   1.2 |
| UDP, `from_head`                                    |    1.3 |     1.3 |   1.0 |
| a frame that is one of 6 things: UDP over IPv4      |     13 |      11 |   1.1 |
| a frame that is one of 6 things: ARP                |    1.3 |     2.2 |   0.6 |
| a frame that is one of 6 things: the last one       |    1.0 |     2.1 |   0.5 |
| a frame that is one of 6 things: a wrong checksum   |     12 |      11 |   1.1 |
| `[u8]` as `[bool]`, 64 KiB                          |  1 550 |  21 980 |  0.07 |
| `[u8]` as `[Kind]` (1, 2, 3 or 7), 4 KiB            |    152 |   5 241 |  0.03 |
| `[u32]` as `[char]`, 4 KiB                          |    186 |     336 |   0.6 |
| CRC-32, 1518 bytes (by hand: 16 bytes at a time)    |    710 |     594 |   1.2 |
| CRC-16/Modbus, 1518 bytes (by hand: a byte at once) |    701 |   3 711 |   0.2 |
| `sum16`, 1500 bytes                                 |     48 |      48 |   1.0 |

The hand-written slice checks are written the obvious way, `bytes.iter().all(|&b| b <= 1)`.
`puns` checks 64 bytes at a time, without a branch inside a block. A rule is compiled to its
arithmetic, one function per operation, which the optimizer inlines. No expression is
interpreted at run time.

A view enum returns what trying its variants in order would return, but it decides each variant
only once. Bytes whose EtherType does not match a variant never reach that variant. A variant
whose values are all values of a later variant, such as `Frame<Ipv4<Udp>>` of `Frame<Ipv4>`, is
decided after that variant, and then checked only for what it adds: the protocol. So a UDP
packet with a wrong checksum computes that checksum once, for `Frame<Ipv4>`. The optimizer
inlines the view where it is called, while its checksums stay calls. It does not inline the
hand-written check, which is why some frames take less time than by hand. The documentation of
[`puns/src/view.rs`][view] explains how a view enum decides its variants.

### What to keep in mind

- **A check never allocates and never panics, and its time is bounded by the size of the
  value.** Where Rust would panic, the rule fails. A `walk` takes at most `8 * size + 128` steps.
  Untrusted bytes may be given to `from_bytes` and `from_head`.
- **A slice is checked in blocks** of 64 bytes, without a branch inside a block. An invalid
  element is found when its block is done, not at its own byte.
- **A CRC adds 32 KiB of tables to the binary** for each set of parameters used. It processes 16
  bytes at a time, and reads 16 KiB of the tables for a CRC of up to 32 bits. `sum16` adds 8 bytes
  at a time and needs no table.
- **A rule has the meaning it would have on 128-bit integers, but computes in a register what
  fits in one.** The range of each value is known when the program is built, from the widths of
  the fields and of what the rule reads. A sum of two 16-bit fields is one register addition,
  without an overflow test. A `walk` whose state holds two things, such as the kind of a header
  and where it starts, keeps each in one half: `(kind << 64) + start`. An addition to the low
  half is computed there alone when it cannot carry into the high half. A counter that only a
  comparison bounds is computed on 128 bits. The documentation of [`puns/src/narrow.rs`][narrow]
  explains how.
- **Each checked pun is code** specialized for its pair of types. A proven pun adds nothing to
  the binary. A checked pun adds its checks, inlined where it is called.
- **Measure in release.** A debug build inlines nothing, so each operation of a rule is a call.
- **The analysis costs build time.** It runs in the compiler, for each new pair of types. The
  Ethernet example (32 types, about 80 puns, a view enum of 19 variants) builds in a few seconds
  in debug, and in about a minute in release with LTO and one codegen unit.
- **The functions are `const fn`.** A pun of constants is done when the program is built.

## Next to `transmute`, `bytemuck` and `zerocopy`

`transmute` checks that the sizes are equal. Everything else must be proven by hand, in a
comment.

[`bytemuck`](https://crates.io/crates/bytemuck) and [`zerocopy`](https://crates.io/crates/zerocopy)
describe each type on its own with marker traits. Roughly, the traits say "it can be read as
bytes", "any bytes make one", and "bytes can be checked to make one". A conversion is allowed when
the source has one trait and the target another. `puns` instead compares the two types with each
other, byte by byte. This has four consequences.

- **What the source proves is not checked again.** An old item is an item, a version 1 save is a
  save, and a UDP frame is a frame. Each of these is proven and costs no instruction, where a
  trait of the target alone would require a check of its bytes.
- **Types with invalid values convert to each other,** by `&mut` too. A `&mut [bool; 4]` can be
  read as a `&mut [[bool; 2]; 2]`, and one enum as another when each accepts what the other holds.
- **Enums without `repr` are supported,** `Option` and `Result` among them. Their layout, chosen
  by `rustc`, is found when the program is built. A marker-trait derive instead requires an enum
  with fields to have a `repr` that specifies its layout. With the `unstable` feature, a `bool` is
  proven to be an `Option<bool>`. Without it, the pun is refused, because Rust does not specify
  where the `None` of an `Option<bool>` is.
- **The rules of a format are part of its type:** sizes and lengths stored in the data, values
  that say what follows, and checksums. With a marker trait, a cast gives a header, and the code
  that finds where the packet ends must still be written and trusted.

These crates have what `puns` does not: years of use and review, an older minimum Rust version,
helpers that allocate, and refusals expressed as trait bounds, which `cargo check` reports and an
editor shows. `puns` runs its analysis in the compiler when it evaluates constants. This costs
build time, a fraction of a second for each new pair of types.

## How it is verified

- **A model check.** Every verdict of the analysis is compared with oracles that do not use it.
  It covers every ordered pair of the model's types of the same size, and every value of the
  source. Parts are covered by writing every value through every `&mut`. The full check runs
  every night, with and without the `unstable` feature. A sample runs on every push, both ways.
- **Miri** runs on every push: with tree borrows on x86-64, and with stacked borrows on a 32-bit
  target (`i686`) and on a big-endian one (`s390x`).
- **Compile-fail tests.** More than a hundred programs must not build, each with its expected
  message.
- **A `core` that lies.** The derive runs in a crate whose `core` is another crate, with an
  `offset_of!` that swaps fields. The generated code never trusts a name that safe code can
  redefine.

The [crate documentation](https://docs.rs/puns/latest/puns/#soundness) states what the soundness of
`puns` rests on.

## Self-relative pointers

A self-relative pointer is an offset from its own address to a target in the same buffer, as in
the regions of [`nearest`](https://crates.io/crates/nearest). A crate declares its self-relative
pointers with `puns::SelfRelative`. `puns::view_ref` then reads a value that holds them as
another type, and proves at compile time everything that they point to. For example, a tree in a
buffer can be read as the next version of its type, for no instruction.

## Examples

Each file of [`puns/examples/`][examples] declares formats with the derive only, without `impl`
or `unsafe`. It reads them from bytes and checks that corrupted bytes are rejected. Each file
explains what is specific to its formats, which attribute expresses it, and what is not covered
and why. The examples run as tests (`cargo test --test ethernet`) or one at a time
(`cargo run --example ethernet`).

| Example          | Formats                                                                        |
|------------------|--------------------------------------------------------------------------------|
| `ethernet.rs`    | Ethernet II and 802.3, VLANs, ARP, IPv4, IPv6 and its extension headers, UDP, TCP, ICMP, MPLS, PPPoE, 802.1X, LLDP |
| `ip_services.rs` | DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP                         |
| `transport.rs`   | TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket, SSH, MQTT, CoAP, SCTP |
| `routing.rs`     | BGP-4, OSPFv2 and its Fletcher checksum, GRE, VXLAN, Geneve, IGMP, ICMPv6 neighbor discovery |
| `link.rs`        | IEEE 802.11, Bluetooth HCI and L2CAP, CAN and CAN FD, Modbus RTU and TCP, SLIP, PPP |
| `images.rs`      | PNG, BMP, the table directory of TrueType and OpenType fonts                  |
| `media.rs`       | RIFF and WAVE, MP4 (ISO-BMFF) boxes                                            |
| `archives.rs`    | ZIP, tar (ustar), gzip                                                         |
| `executables.rs` | ELF (32 and 64 bits, either byte order), PE/COFF, Mach-O, Java class files, WebAssembly |
| `storage.rs`     | the SQLite header, FAT boot sectors, GPT partition tables, USB descriptors     |
| `encodings.rs`   | ASN.1 DER (SNMP, Kerberos), Protocol Buffers, MessagePack, CBOR, UTF-8 fields, C strings, an HTTP/1.1 request line |

## Getting started

```toml
[dependencies]
puns = "0.1"
```

`puns` requires Rust 1.99 or later. 1.99 is the first stable release where `size_of_val_raw` and
`align_of_val_raw` are `const`, which `puns` needs to read the layout of a type that ends with a
slice. `puns` is `no_std` and depends only on its derive. Use `cargo build` to see refusals,
because `cargo check` does not evaluate the constants that decide them.

By default, nothing that `puns` does depends on a layout that Rust leaves to the compiler. With
`features = ["unstable"]`, `puns` uses the layout that `rustc` chose. See
[Layouts that Rust leaves to the compiler](#layouts-that-rust-leaves-to-the-compiler-and-the-unstable-feature).

The version is `0.1`, so the API may change before `1.0`.
[Limitations](https://docs.rs/puns/latest/puns/#limitations) lists what is not supported yet.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[checks]: https://github.com/uael/puns/blob/main/puns/benches/checks.rs
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[view]: https://github.com/uael/puns/blob/main/puns/src/view.rs
[narrow]: https://github.com/uael/puns/blob/main/puns/src/narrow.rs
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[padding]: https://doc.rust-lang.org/reference/glossary.html#padding
