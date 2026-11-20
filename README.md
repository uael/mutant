# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning without `unsafe`: read bytes as a struct, a struct as bytes, or one type as another.
`puns` compares the two types byte by byte at compile time, so a pun is either free, checked at
run time for what the types don't guarantee, or rejected by the compiler.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, `no_std`. The API may change before 1.0.

## Usage

This reads the messages of a small binary protocol from a byte slice. Each message has a magic
number, a kind, a length, and a body of that length.

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

let bytes = b"PUNS\x01\x00\x00\x00PUNS\x02\x00\x00\x05hello";
let (ping, rest) = puns::from_head::<Message>(bytes).unwrap();
let (data, rest) = puns::from_head::<Message>(rest).unwrap();
assert_eq!(ping.kind, Kind::Ping);
assert_eq!((&data.body, rest), (&b"hello"[..], &b""[..]));

// There is no kind 3.
let unknown = puns::from_head::<Message>(b"PUNS\x03\x00\x00\x00");
assert_eq!(unknown.err(), Some(puns::Error::Validity));
```

`from_head` reads a message from the front of the bytes and returns the rest. The message is a
reference into `bytes`, and nothing is copied.

The field types come from [`puns::num`][num]. A `u16be` is a big-endian `u16`, stored as two bytes
at any address, so `Message` has no padding and can sit anywhere in a buffer. A `u32bec<N>` is a
`u32` that is always `N`, which is how the magic number is checked. The `size` attribute says how
long a message is, so `from_head` knows where the next one starts. The derive also takes
conditions on fields, checksums, and tags that say what follows: the [examples](#examples) use
them on real formats.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
its fields, and the values they may hold. `pun::<S, D>` and the other functions compare the
constants of their two types in a `const` block, which the compiler evaluates when it generates
the function for those types. There are three outcomes:

- **Proven.** Every value of `S` is a [valid] `D`. `pun`, `pun_ref` and `as_bytes` accept only
  these, and check nothing at run time.
- **Checked.** Only some values are. The `try_` and `from_` functions check what wasn't proven,
  and return an [`Error`] when it fails.
- **Rejected.** The `const` block panics, and the build fails with a message that says why.

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

Because the check happens when code is generated, `cargo check` doesn't see these errors, and
`cargo build` does. The analysis isn't free either: it adds to the build time of each pair of
types that you pun.

## One type as another

Comparing two types, rather than checking each type on its own, is what lets `puns` convert
between types that aren't plain bytes. Here version 2 of a game adds keys to its items, and reads
the items saved by version 1 without a check.

```rust
use core::num::NonZero;

use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: NonZero<u8>, door: Option<NonZero<u8>> } = 2,
}

// Every old item is an item, so this compiles to no check.
let bag = [OldItem::Sword { damage: 7.into() }, OldItem::Potion { heal: 5, cursed: true }];
let items: &[Item] = puns::pun_ref(&bag[..]);
assert_eq!(items[1], Item::Potion { heal: 5, cursed: true });

// The other way needs a check, and a key fails it.
let keys = [Item::Key { floor: NonZero::new(3).unwrap(), door: None }];
assert_eq!(puns::try_pun_ref::<_, [OldItem]>(&keys[..]).err(), Some(puns::Error::Validity));
```

`puns::pun_mut::<[OldItem], [Item]>` doesn't compile, since it would let you write a key into the
old bag. [`docs/save-file.md`][save-file] continues this example into a whole save file, with a
rule on its fields and a loader for both versions.

## Deriving `Pun`

Deriving `Pun` lets any code read and write every field of the type through bytes, as if the
fields were public. Don't derive it on a type whose fields keep a private invariant that the
derive doesn't know about. See [What deriving `Pun` means].

By default, no pun depends on a layout that Rust leaves to the compiler, such as the order of the
fields of a struct without `repr`. The `unstable` feature, which works on stable Rust, lets `puns`
use the layout that `rustc` chose instead. See [The `unstable` feature][unstable].

## Examples

Each example declares a family of formats with the derive alone, without `impl` or `unsafe`, and
reads them in place. Each covers the structures it names, not every corner of its formats, and
most list what they leave out. `media.rs` and `images.rs` are the shortest. Run one with
`cargo run --example media`.

| Example | Formats |
|---|---|
| [`media.rs`][media] | RIFF and WAVE, MP4 boxes |
| [`images.rs`][images] | PNG, BMP, TrueType and OpenType tables |
| [`archives.rs`][archives] | ZIP, tar, gzip |
| [`storage.rs`][storage] | SQLite header, FAT, GPT, USB descriptors |
| [`link.rs`][link] | 802.11, Bluetooth HCI and L2CAP, CAN, Modbus, SLIP, PPP |
| [`routing.rs`][routing] | BGP-4, OSPFv2, GRE, VXLAN, Geneve, IGMP, ICMPv6 |
| [`encodings.rs`][encodings] | ASN.1 DER, Protocol Buffers, MessagePack, CBOR, C strings |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP |
| [`executables.rs`][executables] | ELF, PE/COFF, Mach-O, Java class files, WebAssembly |
| [`ethernet.rs`][ethernet] | Ethernet, VLANs, ARP, IPv4, IPv6, UDP, TCP, ICMP, MPLS, LLDP |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, WebSocket, SSH, MQTT, CoAP, SCTP |
| [`codegen.rs`][codegen] | No format: functions whose machine code shows what is checked (needs `--features unstable`) |

## Testing

`puns` contains `unsafe` code, and its own analysis decides when that code may run. A model check
compares the analysis with oracles written by hand, on every pair of its test types of the same
size, every night. The tests also run under [Miri] on x86-64, on 32-bit `i686` and on big-endian
`s390x`, and more than a hundred programs must fail to compile with the expected message. See
[`docs/testing.md`][testing], and [Soundness] for what `puns` relies on.

## Performance

A proven pun costs nothing beyond computing an address or a length: on x86-64, reading a `[bool; 4]`
as a `u32` compiles to `mov eax, edi; ret`. Checks run at about the speed of the same checks written
by hand. On an Apple M4 Max, checking an IPv4 packet that carries a 64-byte UDP datagram takes 5.8
ns, against 5.3 ns by hand. Checking 64 KiB of valid bytes as `[bool]` takes 731 ns, against 18,820
ns for `bytes.iter().all(|&b| b <= 1)`, because `puns` checks 64 bytes at a time. A check never
allocates and never panics. Its time grows with the size of the value, except that a `walk` nested
in a `walk` is quadratic. See [`docs/performance.md`][performance] for the other cases, including
one where `puns` is slower, and how they were measured.

## Comparison

`bytemuck` and `zerocopy` give each type marker traits, such as "any bytes are a valid value",
and allow a conversion when the source has one trait and the target another. `puns` compares the
two types with each other instead. That is what makes `&[OldItem]` to `&[Item]` free, lets
`&mut [bool; 4]` read as `&mut [[bool; 2]; 2]`, and makes the sizes and checksums stored in a
format part of its type. Rust's [safe transmute] project also compares two types, in the
compiler, but it is only on nightly.

`binrw`, `deku` and `nom` are parsers: you describe how to turn bytes into values, which can be of
another shape than the bytes, and their errors can say where parsing failed. `puns` only reads bytes
as a type with the same layout, and its [`Error`] says why, not where.

Use `zerocopy` or `bytemuck` if you need a Rust older than 1.99, errors that `cargo check` and
your editor report, or a crate with years of use and review.

## Documentation

- [API documentation][docs]: the functions, the [attributes of the derive][derive], and the types
  of [`puns::num`][num].
- [Compile-time errors]: each error message, and how to fix it.
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
[save-file]: https://github.com/uael/puns/blob/main/docs/save-file.md
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
