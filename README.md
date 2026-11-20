# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` reinterprets a value of one type as a value of another type, such as bytes as a struct or
one enum as another, without `unsafe` in your code. A struct can also declare what a binary
format stores in its bytes, such as a size, a checksum or the type of what follows, and `puns`
checks it when it reads the bytes.

Each function of `puns` compares the layouts of its two types at compile time. A pun that the
types prove [valid] costs no check at run time. The `try_` and `from_` functions check at run
time only what the types do not prove. A pun that can be neither proven nor checked does not
compile, and the error says why. The analysis adds to the build time, and most of its errors
come from `cargo build` rather than from `cargo check`.

```toml
[dependencies]
puns = "0.1"
```

`puns` requires Rust 1.99 or later. It is `no_std`, and depends on nothing but its derive macro.
The version is `0.1`, so the API may change before `1.0`. See [Limitations] for what is not
supported yet.

## Usage

This example writes the save file of a dungeon game, then loads it in the next version of the
game, which adds keys to the items. It has no `unsafe` and no `impl`.

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

`load` copies nothing. The `&Save` that it returns points into `file`.

`bool`, `NonZero`, enums and the types of [`puns::num`][num] guarantee the values that they hold,
and `puns` relies on it. `#[pun(valid(..))]` and the rules of a struct are requirements instead.
A safe assignment can break them, as the last edit of `hp` does, and a pun checks them only when
it reads the value. See [Guarantees and requirements].

## How it works

`#[derive(Pun)]` gives a type a constant that describes its layout: its size, its alignment, its
fields, and the values that each byte may hold. A function of `puns` compares the layouts of its
two types at compile time, and the function decides what may happen at run time.

- A pun is *proven* if the analysis shows that every value of the source is a valid value of the
  target. `pun`, `pun_ref`, `as_bytes` and the functions that read a part of a value compile only
  for proven puns, and check nothing.
- A pun is *checked* if only some values of the source are valid. The `try_` and `from_`
  functions check at run time what is not proven, and return an [`Error`] if it does not hold.
  They never panic.
- A pun is *refused* if the function cannot make it. It does not compile. This is the case when
  no value of the source is valid for the target, when the target would read [padding], and when
  the pun needs a check of a value, a size or an address that the function does not make.

The analysis does not prove everything that is true. For example, a rule `v % 6 == 0` does not
prove a rule `v % 3 == 0`, so a pun that relies on it must be checked.

```rust
// Every `bool` is a byte.
assert_eq!(puns::pun::<[bool; 4], [u8; 4]>([true, false, true, true]), [1, 0, 1, 1]);
// A byte is a `bool` only if it is 0 or 1.
assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

```rust,compile_fail,E0080
// This does not compile. A `u8` may be 2, and `pun` checks nothing.
let flag: bool = puns::pun(2u8);
```

Most refusals are errors of constant evaluation in generic code, so `cargo build` reports them
and `cargo check` does not. See [Compile-time errors] for why, and for how to read them.

A `&mut` lets you write any value of the target, which the source then holds. So `pun_mut`
requires each type to accept every value of the other. `try_pun_mut` checks the value that the
source holds now, but every value of the target must still be a valid source.

```rust
// Read four `bool`s as two pairs, and write one of the pairs.
let mut flags = [true, false, true, true];
let pairs: &mut [[bool; 2]; 2] = puns::pun_mut(&mut flags);
pairs[1] = [false, false];
assert_eq!(flags, [true, false, false, false]);

// The byte holds a `bool` now, and any `bool` written through `flag` is a valid byte.
let mut byte = 1u8;
let flag: &mut bool = puns::try_pun_mut(&mut byte).unwrap();
*flag = false;
assert_eq!(byte, 0);
```

By default, no pun depends on a layout that Rust leaves to the compiler, such as the order of the
fields of a struct without `repr`. Such a pun is refused, and `-Zrandomize-layout` changes no
outcome. With the `unstable` feature, `puns` uses the layout that `rustc` chose for this build.
See [The `unstable` feature][unstable] for the puns that this allows.

```toml
[dependencies]
puns = { version = "0.1", features = ["unstable"] }
```

Deriving `Pun` lets anyone read and write every field of the type through bytes, as if it were
public. Do not derive it on a type whose fields hold an invariant that the derive is not told of.
See [What deriving `Pun` means].

## Comparison with `transmute`, `bytemuck` and `zerocopy`

`transmute` checks that the two sizes are equal. Everything else is a condition of the `unsafe`
block that calls it.

`bytemuck` and `zerocopy` describe each type on its own with marker traits, such as "any bytes
are a value of this type". A conversion is allowed when the source has one trait and the target
another. `puns` compares the two types with each other instead.

- A conversion between two types that are not plain bytes can be free. `&[OldItem]` as `&[Item]`
  checks nothing, because every old item is an item. `zerocopy::try_transmute_ref` checks the
  bytes against the target, because the traits do not relate the two types.
- A `&mut` conversion works between types that have invalid values, such as `&mut [bool; 4]` and
  `&mut [[bool; 2]; 2]`.
- Sizes, tags and checksums stored in the data are part of the type, so `from_bytes` checks a
  whole packet and finds where it ends. With `zerocopy`, you read the header, compute the length
  of the rest, and check the checksums yourself.

Use `zerocopy` or `bytemuck` if you need a Rust older than 1.99, helpers that allocate, refusals
that `cargo check` and your editor report as unmet trait bounds, or a crate with years of use and
review.


## Examples

Each example declares a family of formats with the derive alone, without `impl` or `unsafe`, and
reads them in place from bytes. It also checks that corrupted bytes are rejected, and says what it
does not cover. Run one with `cargo run --example ethernet`.

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

## Documentation

- The [API documentation][docs] describes each function and the compile-time conditions of each,
  the [attributes of the derive][derive], and the types of [`puns::num`][num].
- [Soundness] states what a pun guarantees, and what `puns` assumes to guarantee it.
- [Compile-time errors] explains the errors of refused puns and what to do about them.
- [The `unstable` feature][unstable] lists the puns that need a layout chosen by `rustc`.
- [Limitations] lists what is not supported yet.
- [`docs/performance.md`][performance] gives the machine code and the time of the checks, next to
  the same checks written by hand.
- [`docs/testing.md`][testing] describes how `puns` is tested.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[padding]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.undef
[docs]: https://docs.rs/puns
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Guarantees and requirements]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
[What deriving `Pun` means]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
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
