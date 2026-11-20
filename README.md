# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` reads the bytes of a value of one type as a value of another type: bytes as a struct, a
struct as bytes, one enum as another. This is type punning. In Rust it usually takes `transmute`
or a pointer cast, and `unsafe`. With `puns`, your code has neither.

`puns` compares the two types byte by byte at compile time. If every value of the source is a
[valid] value of the target, the pun is *proven*, and nothing is checked at run time. If only
some values are, the pun is *checked* at run time, for what the types do not already guarantee.
Otherwise the pun is *refused* with a compile error that says why. The price is paid in the
build: most of these errors come from `cargo build` and not from `cargo check`, and each pair of
types adds to the build time.

```toml
[dependencies]
puns = "0.1"
```

`puns` requires Rust 1.99 or later. It is `no_std`, and depends on nothing but its derive macro.

## Example

This example writes the save file of a dungeon game, then loads it in the next version of the
game, which adds keys to the items. It has no `unsafe` and no `impl`.

```rust
use core::num::NonZero;

use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

/// An item, as version 1 of the game saves it: a tag, then two bytes.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0, // little-endian, at any address
  Potion { heal: u8, cursed: bool } = 1,
}

/// An item of version 2, which adds keys. A key without a door opens every door of its floor.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: NonZero<u8>, door: Option<NonZero<u8>> } = 2,
}

// View an item as its bytes. Every item is three initialized bytes, so nothing is checked.
assert_eq!(puns::as_bytes(&OldItem::Sword { damage: 300.into() }), [0, 0x2c, 0x01]);
// Read three bytes as an item. The tag is checked, then the fields of its variant.
let key: &Item = puns::from_bytes(&[2, 1, 0]).unwrap();
// A `u8c<1>` is always 1, so it converts to a `NonZero<u8>` without a check. A door of 0 is `None`.
assert_eq!(key, &Item::Key { floor: puns::pun(u8c::<1>::new()), door: None });
// A key of floor 0 is not an item.
assert_eq!(puns::from_bytes::<Item>(&[2, 0, 0]), Err(puns::Error::Validity));

/// A save file of version `V`, with a bag of type `B`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // at most 10 hp per level
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // always "SAVE"
  version: V,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: B, // to the end of the file
}

// Write a save of version 1. A save is its bytes, so writing it copies them.
let bag = [OldItem::Sword { damage: 9.into() }, OldItem::Potion { heal: 25, cursed: false }];
let old = Save { magic: u32bec::new(), version: u8c::<1>::new(), level: 7, hp: 42.into(), bag };
let mut file = puns::as_bytes(&old).to_vec();
// The first four bytes of a save are its magic number.
assert_eq!(puns::head_ref::<_, [u8; 4]>(&old), b"SAVE");
// Give the hero a level, in the file, through a `&mut Save` to its bytes.
puns::from_bytes_mut::<Save<u8c<1>, [OldItem]>>(&mut file).unwrap().level += 1;

/// A save file of either version.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

/// Loads a save of either version as a save of version 2.
fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  // Check the bytes once: the magic number, the version, the level, the rule and each item.
  Ok(match SaveFile::from_bytes(file)? {
    // Every old item is an item, and 1 is a `u8`, so neither conversion checks anything.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

assert_eq!(load(&file).unwrap().level, 8);
// Add the key to the bag. Version 1 has no keys, so the file is no longer a save.
file.extend_from_slice(puns::as_bytes(key));
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
// Upgrade the file to version 2 by writing its version byte.
puns::from_bytes_mut::<Save>(&mut file).unwrap().version = 2;
let save = load(&file).unwrap();
assert_eq!(save.bag[0], Item::Sword { damage: 9.into() });
assert_eq!(&save.bag[2], key);
// Reading the bag as old items is checked. It fails here, because of the key.
assert_eq!(puns::try_pun_ref::<_, [OldItem]>(&save.bag).err(), Some(puns::Error::Validity));
// Give the hero 999 hp. The rule is checked when bytes are read as a `Save`, so the file no
// longer loads.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```

`load` copies nothing. The `&Save` that it returns points into `file`.

## How it works

`#[derive(Pun)]` describes a type as a constant: its size, its alignment, its fields, and the
values that each byte may hold. Each function of `puns` compares the constants of its two types
at compile time. There are three outcomes.

- A pun is *proven* if every value of the source is a valid value of the target. `pun`,
  `pun_ref`, `as_bytes` and the functions that take a part of a value compile only then. They
  check nothing.
- A pun is *checked* if only some values are. The `try_` and `from_` functions check at run time
  what is not proven, and return an [`Error`] if it does not hold. They never panic.
- A pun is *refused* otherwise, and the call does not compile. This is the case when no value of
  the source is valid for the target, when the target would read [padding], and when the pun
  needs a check that the function does not make: of a value, of a size or of an address.

```rust
// Every `bool` is a byte.
assert_eq!(puns::pun::<[bool; 4], [u8; 4]>([true, false, true, true]), [1, 0, 1, 1]);
// A byte is a `bool` only if it is 0 or 1.
assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

```rust,compile_fail,E0080
// This does not compile. The byte may be 2, and `pun` checks nothing.
let flag: bool = puns::pun(2u8);
```

Most refusals are errors of constant evaluation in generic code, so `cargo build` reports them
and `cargo check` does not. [Compile-time errors] explains why.

A `&mut` lets you write any value of the target, which the source then holds. So `pun_mut`
requires each type to accept every value of the other. `try_pun_mut` checks the value that the
source holds now, but every value of the target must still be a valid source.

```rust
// Two pairs of `bool`s are four `bool`s, and four `bool`s are two pairs.
let mut flags = [true, false, true, true];
let pairs: &mut [[bool; 2]; 2] = puns::pun_mut(&mut flags);
pairs[1] = [false, false];
assert_eq!(flags, [true, false, false, false]);

// This byte holds a `bool` now, and every `bool` that is written through `flag` is a byte.
let mut byte = 1u8;
let flag: &mut bool = puns::try_pun_mut(&mut byte).unwrap();
*flag = false;
assert_eq!(byte, 0);
```

```rust,compile_fail,E0080
// This does not compile. A `u8` written through the `&mut` could be 2.
let mut flag = true;
let byte: &mut u8 = puns::pun_mut(&mut flag);
```

By default, no pun depends on a layout that Rust leaves to the compiler, such as the order of the
fields of a struct without `repr`. Such a pun is refused, and `-Zrandomize-layout` changes no
outcome. With the `unstable` feature, `puns` uses the layout that `rustc` chose for this build.
[The `unstable` feature][unstable] lists what this allows.

Deriving `Pun` lets anyone read and write every field of the type through bytes, as if it were
public. Do not derive it on a type whose fields hold an invariant that the derive is not told of.
[What deriving `Pun` means] says more.

## Reading a network packet

A size stored in a header, a checksum, a value that says what follows: the derive is told these
facts with attributes. The example below reads UDP over IPv4 over Ethernet with them, and has no
parsing code.

```rust
use puns::{
  Pun, Tag,
  num::{u8c, u16be, u16bec},
};

/// An Ethernet frame. By default its payload is bytes, of any EtherType. A `Frame<Ipv4>` is a
/// frame whose EtherType is IPv4's and whose payload is an `Ipv4`.
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
#[pun(tag(EtherType = u16bec<0x0800>))] // the EtherType of IPv4
#[pun(size = total_length)]
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))] // the checksum of the header
#[pun(get(header_length: usize = (version_ihl & 0x0f) * 4))]
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
  // The options, whose size is in the header too, then the payload.
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

/// A frame of one of several kinds. `from_bytes` returns the first variant that matches.
#[derive(Pun)]
enum Packet<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Ipv4(&'a Frame<Ipv4>),
  Other(&'a Frame),
}

let bytes: [u8; 47] = [
  2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x08, 0x00, // Ethernet, to IPv4
  0x45, 0, 0, 33, 0, 1, 0x40, 0, 64, 17, 0x26, 0xc9, 10, 0, 0, 1, 10, 0, 0, 2, // IPv4, to UDP
  0x30, 0x39, 0, 53, 0, 13, 0, 0, b'h', b'e', b'l', b'l', b'o', // UDP, to port 53
];

// Check the EtherType, the version, the size, the checksum of the header and the protocol, in
// one call. The frame is a reference into `bytes`.
let frame: &Frame<Ipv4<Udp>> = puns::from_bytes(&bytes).unwrap();
let ip = &frame.payload;
assert_eq!((ip.header_length(), ip.time_to_live), (20, 64));
assert!(ip.options().is_empty());
// Check the UDP header, whose size is its own.
let udp = ip.payload().unwrap();
assert_eq!((udp.destination_port.get(), &udp.payload), (53, &b"hello"[..]));

// Read the frame as a frame of any EtherType. Nothing is checked.
let any: &Frame = puns::pun_ref(frame);
assert_eq!(any.payload.len(), 33);
// Read it as an IPv4 frame of any protocol. `Ipv4<Udp>` and `Ipv4` have the same rules over the
// same bytes, so nothing is checked either.
let ip: &Frame<Ipv4> = puns::pun_ref(frame);
assert_eq!(ip.payload.header_length(), 20);

// Flip one bit of the header. The checksum fails, so the bytes are not IPv4.
let mut corrupted = bytes;
corrupted[22] ^= 1;
assert_eq!(puns::from_bytes::<Frame<Ipv4>>(&corrupted).err(), Some(puns::Error::Validity));
assert!(matches!(Packet::from_bytes(&corrupted), Ok(Packet::Other(_))));
assert!(matches!(Packet::from_bytes(&bytes), Ok(Packet::Udp(_))));

// Change the length of the UDP header. The IPv4 packet is still valid, and `payload` fails.
let mut wrong_length = bytes;
wrong_length[39] = 12;
let frame = puns::from_bytes::<Frame<Ipv4<Udp>>>(&wrong_length).unwrap();
assert_eq!(frame.payload.payload().err(), Some(puns::Error::Validity));

// Read a packet from the front of a stream. `from_head` takes its size from the header, and
// returns the bytes that follow.
let stream = [&bytes[14..], &[0xaa, 0xbb]].concat();
let (packet, rest) = puns::from_head::<Ipv4>(&stream).unwrap();
assert_eq!((size_of_val(packet), rest), (33, &[0xaa, 0xbb][..]));
```

A region such as `payload` is checked when you call its method, not with the value that holds
it. [`puns/examples/ethernet.rs`][ethernet] is the complete version of this example. It adds the
checksums of UDP and TCP, IPv6 behind any chain of extension headers, VLAN tags, and the frame
check sequence.

The derive takes these attributes. Its [documentation][derive] describes each of them.

| Attribute or type                      | Meaning                                                      |
|----------------------------------------|--------------------------------------------------------------|
| `u16be`, `u32le`, `i64be`, `f32le`     | a number of a given byte order, at any address               |
| `u8c<17>`, `u32bec<0x8950_4e47>`       | a number that has one value, such as a magic number          |
| `u8in<b'0', b'9'>`                     | a byte in a range, which the type guarantees                 |
| `#[pun(valid(1..=9, 0x80))]`           | the values that a field must hold                            |
| `#[pun(size = EXPR)]`                  | the size of the value, in bytes                              |
| `#[pun(len = EXPR)]`                   | the number of elements of the array or slice that ends it    |
| `#[pun(assert(EXPR))]`                 | a condition that the value must meet                         |
| `#[pun(regions(a = EXPR, b: Type))]`   | consecutive regions of the tail, and a method to read each   |
| `#[pun(get(name: Type = EXPR))]`       | a method that computes a value from the fields               |
| `#[pun(tag(Key = Type))]`              | the value of `Key` that comes with this type                 |
| an enum whose variants hold references | a value of one of several types (`from_bytes`, `from_head`)  |

An `EXPR` of a rule is an expression over the unsigned integer fields of the struct. It can also
read the bytes of the value: integers at an offset (`le`, `be`), checksums (`sum16`, `crc32`,
`crc`, `fletcher16`, `adler32`), integers of variable length (`leb128`, `quic`, `vlq`), patterns
(`matches`) and chains of headers (`walk`). An expression that would panic in Rust, such as an
overflow or a read past the end, makes the rule fail.

## Parts, streams and constants

Each function reads a whole value, its front or its back, by value, by `&` or by `&mut`. The
`from_` functions read bytes. The [list of functions][functions] gives each of them.

```rust
use puns::{
  Pun,
  num::{u8in, u16be, u32be, u32le},
};

// An `f32` is its bits. A `u32` is a `char` only if it is a Unicode scalar value.
assert_eq!(puns::pun::<f32, u32>(1.0), 0x3f80_0000);
assert_eq!(puns::try_pun::<u32, char>(0xe9), Ok('é'));
assert_eq!(puns::try_pun::<u32, char>(0xd800), Err(puns::Error::Validity));

// The low half of a big-endian number is its last two bytes.
assert_eq!(puns::tail::<u32be, u16be>(0x1234_5678.into()).get(), 0x5678);
assert_eq!(puns::head::<u32le, u8>(0x1234_5678.into()), 0x78);

/// A pixel. Its first three bytes are its color.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Rgba {
  red: u8,
  green: u8,
  blue: u8,
  alpha: u8,
}

// Turn RGBA into BGRA in place, through a `&mut` to the color alone.
let mut pixel = Rgba { red: 255, green: 128, blue: 0, alpha: 200 };
puns::head_mut::<_, [u8; 3]>(&mut pixel).reverse();
assert_eq!(pixel, Rgba { red: 0, green: 128, blue: 255, alpha: 200 });

/// A string as a stream holds it: a length, then that many bytes of UTF-8.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 1 + length)]
struct Text {
  length: u8,
  text: str,
}

// Read strings from the front of a stream. `from_head` returns each one and the bytes after it.
let mut stream: &[u8] = b"\x05hello\x00\x05world\xff";
let mut words = vec![];
while let Ok((word, rest)) = puns::from_head::<Text>(stream) {
  words.push(&word.text);
  stream = rest;
}
assert_eq!((words, stream), (vec!["hello", "", "world"], &[0xff][..]));

// Read a checksum from the back of a file.
let file = [b'd', b'a', b't', b'a', 0x78, 0x56, 0x34, 0x12];
let (data, checksum) = puns::from_tail::<u32le>(&file).unwrap();
assert_eq!((data, checksum.get()), (&b"data"[..], 0x1234_5678));

// Reading bytes as ASCII is checked. Reading ASCII as a `str` is proven.
let ascii: &[u8in<0, 0x7f>] = puns::from_bytes(b"hello").unwrap();
let text: &str = puns::pun_ref(ascii);
assert_eq!(text, "hello");
```

The functions are `const fn`, so bytes in a constant are checked when the program is compiled.

```rust
use puns::{
  Pun,
  num::{u16le, u32bec},
};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))]
struct Save {
  magic: u32bec<0x5341_5645>,
  version: u8,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: [u8],
}

// A new game starts from a save in the binary. With 11 hp at level 1, the build fails.
const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0a\x00") {
  Ok(save) => save,
  Err(_) => panic!("the save of a new game is not a save"),
};
assert_eq!((NEW_GAME.level, NEW_GAME.hp.get()), (1, 10));
```

A value that holds self-relative pointers, such as a region of [`nearest`], is read as another
type with [`view_ref`], which proves at compile time what the pointers point to. A crate declares
its pointers with [`SelfRelative`].

## Compile errors

A refusal is an error of `rustc`, and its first lines say why. Here is a common mistake with
`transmute`: reading a struct with padding as bytes.

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
...
note: the above error was encountered while instantiating `fn pun_ref::<Header, [u8; 8]>`
 --> src/main.rs:9:25
  |
9 |   let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
  |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Byte 1 is the first of the three bytes of padding after `kind`. The last note says where the call
is in your code. Between them, `rustc` prints about thirty lines that locate the failed constant
inside `puns`, which you can skip. To fix this pun, declare the padding as a field, so that it is
initialized.

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

[Messages] lists the other messages, each with code that compiles.

## Performance

A proven pun compiles to no check. It may still compute an address or a length. A checked pun
compiles to the checks of what the analysis could not prove, and nothing else. The table below
gives the machine code of some functions of [`puns/examples/codegen.rs`][codegen], which defines
their types and records the others. It was measured on `x86_64-unknown-linux-gnu` with `rustc`
1.100.0, in release with LTO and one codegen unit.

| Function                                           | Machine code                                  |
|----------------------------------------------------|-----------------------------------------------|
| `&Position -> &[u8; 2]`                            | `mov rax, rdi; ret`                           |
| `[bool; 4] -> u32`                                 | `mov eax, edi; ret`                           |
| `&Packet<Datagram> -> &Packet` (the same rules)    | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`               |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`, 12 instructions |
| `from_head::<Frame<Ipv4>>(&[u8])`, no checksum     | 29 instructions, no loop, no call             |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 207 instructions, no call                     |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, no branch in a block      |

[`puns/benches/checks.rs`][checks] times the checks next to the same checks written by hand. The
times below are in nanoseconds per check, measured with
`cargo bench -p puns --features unstable --bench checks` on a Xeon at 2.1 GHz. Each is the median
of 5 runs, or of 3 for IPv4 in 64 bytes, IPv6 behind 0 and 3 headers and the view enum, and each
run is the best of 7 samples. The checksums are computed the same way on both sides, except
where a row says otherwise.

| Check                                               | `puns` | by hand |
|-----------------------------------------------------|-------:|--------:|
| IPv4, UDP, 64 bytes (two checksums)                 |     11 |      10 |
| IPv4, UDP, 1472 bytes                               |     53 |      51 |
| IPv4, TCP, 1460 bytes                               |     53 |      53 |
| IPv6, TCP, no extension header                      |    1.1 |     1.5 |
| IPv6, TCP behind 3 extension headers                |    3.4 |     4.8 |
| IPv6, TCP behind 12 extension headers               |     12 |      15 |
| IPv6, UDP, 1400 bytes (its checksum)                |     50 |      42 |
| UDP, `from_head`                                    |    1.3 |     1.3 |
| a view enum of 6 variants: UDP over IPv4            |     13 |      11 |
| a view enum of 6 variants: ARP                      |    1.3 |     2.2 |
| a view enum of 6 variants: its last variant         |    1.0 |     2.1 |
| a view enum of 6 variants: a wrong checksum         |     12 |      11 |
| `[u8]` as `[bool]`, 64 KiB                          |  1 550 |  21 980 |
| `[u8]` as `[Kind]` (1, 2, 3 or 7), 4 KiB            |    152 |   5 241 |
| `[u32]` as `[char]`, 4 KiB                          |    186 |     336 |
| CRC-32, 1518 bytes (by hand: 16 bytes at a time)    |    710 |     594 |
| CRC-16/Modbus, 1518 bytes (by hand: a byte at once) |    701 |   3 711 |
| `sum16`, 1500 bytes                                 |     48 |      48 |

The slices are checked by hand the obvious way, such as `bytes.iter().all(|&b| b <= 1)`. `puns`
checks a slice of small elements, each a few comparisons, 64 bytes at a time, without a branch
inside a block. A view enum
checks what its variants share once, so a wrong checksum is computed once for all of them.

A check never allocates and never panics, and its time is bounded by the size of the value, so
you can give it untrusted bytes. Each checked pun is code specialized for its two types. The
tables of a CRC take 32 KiB, or 16 KiB for `crc32`, or 2 KiB on a 16-bit target. Measure the
checks in release. They rely on inlining, which a debug build mostly does not do.

## Comparison with `transmute`, `bytemuck` and `zerocopy`

`transmute` checks that the two sizes are equal. Everything else is yours to prove, usually in a
comment next to the `unsafe`.

`bytemuck` and `zerocopy` describe each type on its own with marker traits, such as "any bytes
are a value of this type". A conversion is allowed when the source has one trait and the target
another. `puns` compares the two types with each other instead, which changes three things.

- A conversion between two types that are not plain bytes can be free. `&[OldItem]` as `&[Item]`
  checks nothing, because every old item is an item. With marker traits, the target is checked
  from its bytes.
- A `&mut` conversion works between types that have invalid values, such as `&mut [bool; 4]` and
  `&mut [[bool; 2]; 2]`.
- Sizes, tags and checksums stored in the data are part of the type, so `from_bytes` returns a
  whole packet. With marker traits, a cast gives you a header, and you write the code that finds
  where the packet ends.

Use `zerocopy` or `bytemuck` if you need an older version of Rust, helpers that allocate, or
years of use and review. `zerocopy` also reports most of its refusals as unmet trait bounds,
which `cargo check` and your editor show. `puns` has none of these yet.

## Testing

`puns` has `unsafe` code, and its analysis decides what that code may do. Both are tested.

The analysis is compared with oracles written by hand, which do not use it. This model check
covers every ordered pair of its types of the same size. It tries every value of the source for
types of up to 2 bytes, and a chosen set of values for larger ones. It runs in full every night,
and in part on every push, with and without the `unstable` feature.

The tests run under [Miri] on every push: with tree borrows on x86-64, and with stacked borrows
on a 32-bit target (`i686`) and on a big-endian one (`s390x`). More than a hundred programs must
fail to compile, each with its expected message. The derive is also built in a crate whose
`core` is another crate, with an `offset_of!` that swaps fields. This checks that the generated
code trusts no name that you can redefine.

[Soundness] states what the soundness of `puns` rests on.

## Examples

Each file of [`puns/examples/`][examples] but `codegen.rs` declares formats with the derive only,
without `impl` or `unsafe`. It reads them from bytes and checks that corrupted bytes are
rejected. Each file says what is specific to its formats, which attribute expresses it, and what
it does not cover and why. Run one with `cargo run --example ethernet`.

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

The version is `0.1`, so the API may change before `1.0`. [Limitations] lists what is not
supported yet.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[padding]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.undef
[Miri]: https://github.com/rust-lang/miri
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[`view_ref`]: https://docs.rs/puns/latest/puns/fn.view_ref.html
[`SelfRelative`]: https://docs.rs/puns/latest/puns/trait.SelfRelative.html
[`nearest`]: https://docs.rs/nearest
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[functions]: https://docs.rs/puns/latest/puns/#reference
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[Messages]: https://docs.rs/puns/latest/puns/#messages
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[What deriving `Pun` means]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[checks]: https://github.com/uael/puns/blob/main/puns/benches/checks.rs
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
