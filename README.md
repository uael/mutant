# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning without `unsafe`: read bytes as a struct, a struct as bytes, or one type as another.
`puns` checks each pun at compile time, and checks at run time only what it could not prove there.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, `no_std`. The API may change before 1.0.

## Example

This reads messages of a small binary protocol from a stream. Each message starts with a magic
number, a kind and a length, and the length says where the message ends.

```rust
use puns::{
  Pun,
  num::{u16be, u32bec},
};

#[derive(Pun)]
#[repr(C)]
#[pun(size = 8 + length)]
struct Message {
  magic: u32bec<0x5055_4e53>, // "PUNS"
  kind: Kind,
  flags: u8,
  length: u16be,
  body: [u8],
}

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Data = 2,
}

let stream = b"PUNS\x01\x00\x00\x00PUNS\x02\x00\x00\x05hello";
let (ping, rest) = puns::from_head::<Message>(stream).unwrap();
let (data, rest) = puns::from_head::<Message>(rest).unwrap();
assert_eq!(ping.kind, Kind::Ping);
assert_eq!((&data.body, rest), (&b"hello"[..], &b""[..]));

// There is no kind 3.
let unknown = puns::from_head::<Message>(b"PUNS\x03\x00\x00\x00");
assert_eq!(unknown.err(), Some(puns::Error::Validity));
```

`ping` and `data` point into `stream`, and nothing was copied. The derive also takes checksums,
tags that say what follows, and conditions on fields. The [examples](#examples) use them on real
formats.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
its fields, and the values that each byte may hold. A function such as `pun::<S, D>` compares the
constants of `S` and `D` in a `const` block, which the compiler evaluates when it generates the
function for these two types. There are three outcomes:

- **Proven.** Every value of `S` is a [valid] `D`. `pun`, `pun_ref` and `as_bytes` accept only
  these puns, and check nothing at run time.
- **Checked.** Only some values are. The `try_` and `from_` functions check what was not proven,
  and return an [`Error`] if it fails.
- **Refused.** The `const` block panics, and the build fails with a message that says why.

```rust
assert_eq!(puns::pun::<[bool; 2], [u8; 2]>([true, false]), [1, 0]);
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

```rust,compile_fail,E0080
let flag: bool = puns::pun(2u8);
```

```text
error[E0080]: evaluation panicked:
              cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
              Use a `try_*` function to check them at run time.
              This is at byte 0 of the target.
...
note: the above error was encountered while instantiating `fn pun::<u8, bool>`
```

Since refusals happen when code is generated, `cargo check` does not report them. Use `cargo
build`. The analysis also adds to the build time of each pair of types that you pun.

Deriving `Pun` lets any code read and write every field of the type through bytes, as if the
fields were public. Don't derive it on a type with a private invariant that the derive doesn't
know about. See [What deriving `Pun` means].

By default, no pun depends on a layout that Rust leaves to the compiler, such as the order of
fields in a struct without `repr`. The `unstable` feature, which works on stable Rust, lets `puns`
use the layout that `rustc` chose instead. See [The `unstable` feature][unstable].

## A longer example

The save file of a dungeon game, read by two versions of the game. The second version adds keys
to the items. The example reads enums with fields and `NonZero` fields, writes through a `&mut`
into the file, picks a version with an enum of references, and rejects a save edited by a
cheater. `#[pun(valid(..))]` and `#[pun(assert(..))]` are checked when bytes are read as a
`Save`, not when a field is assigned.

```rust
use core::num::NonZero;

use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

/// An item of version 1 of the game, saved as a tag and two bytes.
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
// Floor 0 is not a `NonZero<u8>`, so these bytes are not an item.
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

// Write a save of version 1. Every byte of a save is initialized, so it can be copied as bytes.
let bag = [OldItem::Sword { damage: 9.into() }, OldItem::Potion { heal: 25, cursed: false }];
let old = Save { magic: u32bec::new(), version: u8c::<1>::new(), level: 7, hp: 42.into(), bag };
let mut file = puns::as_bytes(&old).to_vec();
// The first four bytes of a save are its magic number.
assert_eq!(puns::head_ref::<_, [u8; 4]>(&old), b"SAVE");
// Raise the level in the file, through a `&mut Save` that points into its bytes.
puns::from_bytes_mut::<Save<u8c<1>, [OldItem]>>(&mut file).unwrap().level += 1;

/// A save file of either version.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

/// Loads a save of either version as a `Save`, whose version is a `u8`.
fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  // Check the bytes once: the magic number, the version, the level, the rule and each item.
  Ok(match SaveFile::from_bytes(file)? {
    // Every old item is an item, and every version is a `u8`, so neither arm checks anything.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

let loaded = load(&file).unwrap();
assert_eq!((loaded.version, loaded.level), (1, 8));
// Add the key to the bag. Version 1 has no keys, so the file is no longer a valid save.
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

## Examples

Each example declares a family of formats with the derive alone, without `impl` or `unsafe`, and
reads them in place from bytes. It also checks that corrupted bytes are rejected. Start with
`ethernet.rs`, and run one with `cargo run --example ethernet`.

| Example | Formats, and what they need |
|---|---|
| [`ethernet.rs`][ethernet] | Ethernet, VLANs, ARP, IPv4, IPv6, UDP, TCP, ICMP, MPLS, LLDP. An EtherType that says what follows, sizes and options given by headers, checksums over a pseudo-header, the frame check sequence. |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP. DNS names through pointers, options that end with a marker, records counted by a header. |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, WebSocket, SSH, MQTT, CoAP, SCTP. Lengths of 24 bits, integers of variable length, fields present by flags. |
| [`routing.rs`][routing] | BGP-4, OSPFv2, GRE, VXLAN, Geneve, IGMP, ICMPv6. The Fletcher checksum, optional fields announced by flags, prefixes sized in bits. |
| [`link.rs`][link] | IEEE 802.11, Bluetooth HCI and L2CAP, CAN and CAN FD, Modbus, SLIP, PPP. Bit fields, a CRC-16, two lengths that must agree. |
| [`images.rs`][images] | PNG, BMP, the tables of TrueType and OpenType fonts. The CRC of each chunk, the order of chunks, records sorted by tag. |
| [`media.rs`][media] | RIFF and WAVE, MP4 boxes. Four-character codes, a pad byte outside the length, sizes of 32 or 64 bits. |
| [`archives.rs`][archives] | ZIP, tar, gzip. The end of a ZIP file found from its end, sizes in octal ASCII, NUL-terminated names. |
| [`executables.rs`][executables] | ELF, PE/COFF, Mach-O, Java class files, WebAssembly. Byte order and word size as type parameters, sizes in LEB128. |
| [`storage.rs`][storage] | The SQLite header, FAT, GPT, USB descriptors. A sector size as a const parameter, a CRC-32 of a header over itself. |
| [`encodings.rs`][encodings] | ASN.1 DER, Protocol Buffers, MessagePack, CBOR, text. Shortest forms, ZigZag, C strings, UTF-8 of a counted length. |
| [`codegen.rs`][codegen] | No format. Functions whose machine code shows what is checked at run time. |

## Testing

`puns` is `unsafe` code that decides what it may do from its own analysis, so both are tested. A
model check compares the analysis with oracles written by hand, on every pair of a set of types
of the same size, every night. The tests run under [Miri] on x86-64, on a 32-bit target and on a
big-endian one. More than a hundred programs must fail to compile with the expected message.
[`docs/testing.md`][testing] has the details, and [Soundness] states what `puns` relies on.

## Performance

A proven pun compiles to no check, and at most to the arithmetic of an address or a length.
Reading a `[bool; 4]` as a `u32` is `mov eax, edi; ret`. On an Apple M4 Max, checking 64 KiB of
bytes as `[bool]` takes 731 ns, against 18,820 ns for `bytes.iter().all(|&b| b <= 1)`. [`docs/performance.md`][performance] has the
machine code and the times of the other checks, and how they were measured.

## Comparison

`bytemuck` and `zerocopy` give each type marker traits, such as "any bytes are a valid value",
and allow a conversion when the source has one trait and the target another. `puns` compares the
two types with each other. So `&[OldItem]` reads as `&[Item]` with no check, `&mut [bool; 4]`
reads as `&mut [[bool; 2]; 2]`, and the size and checksums that a format stores are part of its
type. Rust's [safe transmute] project compares two types in the compiler too, but it is
available on nightly only.

`binrw` and `deku` derive parsers that build values you own, and `nom` combines parsers that you
write. `puns` reads the bytes in place, and gives you a reference into them.

Use `zerocopy` or `bytemuck` if you need a Rust older than 1.99, refusals that `cargo check` and
your editor report, or a crate with years of use and review.

## Documentation

- [API documentation][docs]: the functions, the [attributes of the derive][derive], and the types
  of [`puns::num`][num].
- [Compile-time errors]: each message, and what to do about it.
- [Limitations]: what is not supported yet.
- [`CHANGELOG.md`][changelog].

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[Miri]: https://github.com/rust-lang/miri
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
[docs]: https://docs.rs/puns
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[What deriving `Pun` means]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[ip_services]: https://github.com/uael/puns/blob/main/puns/examples/ip_services.rs
[transport]: https://github.com/uael/puns/blob/main/puns/examples/transport.rs
[routing]: https://github.com/uael/puns/blob/main/puns/examples/routing.rs
[link]: https://github.com/uael/puns/blob/main/puns/examples/link.rs
[images]: https://github.com/uael/puns/blob/main/puns/examples/images.rs
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[archives]: https://github.com/uael/puns/blob/main/puns/examples/archives.rs
[executables]: https://github.com/uael/puns/blob/main/puns/examples/executables.rs
[storage]: https://github.com/uael/puns/blob/main/puns/examples/storage.rs
[encodings]: https://github.com/uael/puns/blob/main/puns/examples/encodings.rs
[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
