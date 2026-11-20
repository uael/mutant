# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Zero-copy type punning without `unsafe` or `transmute`: read bytes as a struct, a struct as
bytes, or one type as another. `puns` compares the two types at compile time, so a pun is either
free, checked at run time for what the types don't guarantee, or refused by the compiler.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, `no_std`. `puns` is young: it is at 0.1, and its API may change before 1.0.

## Usage

Reading a struct out of a buffer usually takes a pointer cast and a comment arguing that the
bytes are long enough, aligned, initialized and valid. With `puns`, the compiler checks what it
can, and the rest is checked at run time.

```rust
#[derive(puns::Pun, Debug, PartialEq)]
#[repr(C)]
struct Pixel {
  red: u8,
  green: u8,
  blue: u8,
  alpha: u8,
}

let bytes = [255, 0, 0, 255, 0, 0, 255, 128];
let pixels: &[Pixel] = puns::from_bytes(&bytes).unwrap();
assert_eq!(pixels[1], Pixel { red: 0, green: 0, blue: 255, alpha: 128 });

let back: &[u8] = puns::as_bytes(pixels);
assert_eq!(back, bytes);
```

`from_bytes` checks the length of the bytes, and nothing else, since any four bytes are a
`Pixel`. `as_bytes` checks nothing, and both return references to the same memory.

The `#[repr(C)]` matters. Without it, Rust may order the fields as it likes, so `puns` refuses
any pun whose result would depend on that order. The `unstable` feature, which works on stable
Rust, makes `puns` use the order that `rustc` chose instead (see [the `unstable`
feature][unstable]). A struct with padding can be read from bytes, but not viewed as bytes, since
its padding is uninitialized.

### Reading a binary format

Formats often store a size, a magic number or a checksum in their bytes. The derive takes these
as attributes, and checks them when it reads the bytes. This reads the messages of a small
protocol one after the other.

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
assert_eq!(&data.body, b"hello");
assert!(rest.is_empty());

// There is no kind 3.
let unknown = puns::from_head::<Message>(b"PUNS\x03\x00\x00\x00");
assert_eq!(unknown.err(), Some(puns::Error::Validity));
```

`from_head` reads one message from the front of the bytes, and returns it with the bytes after
it. A `u16be` is a big-endian `u16` with an alignment of 1, so `Message` has no padding and can
start at any address. A `u32bec<N>` always holds `N`, which is how the magic number is checked.
The `size` rule tells `from_head` where a message ends.

Rules like `size`, `#[pun(valid(..))]` and `#[pun(assert(..))]` are checked when bytes are read.
They are requirements of the format, not guarantees of the type: safe code can still assign a
field that breaks one, and the next read of those bytes will catch it.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
which bytes are padding, and which values each field may hold. `pun::<S, D>` and the other
functions compare the constants of their two types in a `const` block, which the compiler
evaluates when it generates the function for those types. A pun is:

- **Proven** if every value of `S` is a [valid] `D`, of the right size and alignment, with no
  padding where `D` needs a byte. `pun`, `pun_ref` and `as_bytes` accept only these, and add no
  check.
- **Checked** if only some values are, or if the length or the address of the bytes decides. The
  `try_` and `from_` functions check what wasn't proven, and return an [`Error`] when it fails.
- **Refused** otherwise, for instance when the target would read padding. The `const` block
  panics, and the build fails with a message.

```rust
assert_eq!(puns::pun::<[bool; 2], [u8; 2]>([true, false]), [1, 0]);
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

This one is refused, because a `u8` can be 2:

```rust,compile_fail,E0080
let flag: bool = puns::pun(2u8);
```

```text
error[E0080]: evaluation panicked:
              cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
              Use a `try_*` function to check them at run time.
              This is at byte 0 of the target.
(34 lines that locate the failed constant inside puns)
note: the above error was encountered while instantiating `fn pun::<u8, bool>`
```

Because the check happens when code is generated, `cargo check` doesn't see these errors, and
`cargo build` does. The analysis also costs build time for each pair of types that you pun. An
enum of references costs the most as it grows, since its build grows with the square of its
variants.

## One type as another

Comparing two types, rather than checking each one on its own, is what lets `puns` convert
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

`puns::pun_mut::<[OldItem], [Item]>` is refused, since it would let you write a key into the old
bag. [`docs/save-file.md`][save-file] continues this example into a whole save file, with rules
on its fields and a loader for both versions.

## Deriving `Pun`

Deriving `Pun` lets any code read and write every field of the type through bytes, as if the
fields were public. Don't derive it on a type whose fields keep a private invariant. [What
deriving `Pun` means] has the details.

## Examples

Each example declares a family of formats with the derive alone, without `impl` or `unsafe`, and
reads them in place. They cover the structures they name, not whole formats, and most say what
they leave out. From a clone of the repository, run one with `cargo run --example media`.

| Example | Formats | Shows |
|---|---|---|
| [`media.rs`][media] | WAVE, MP4 boxes | four-character tags, a pad byte, sizes of 32 or 64 bits |
| [`images.rs`][images] | PNG, BMP, font tables | CRCs of chunks, the order of chunks, sorted records |
| [`archives.rs`][archives] | ZIP, tar, gzip | an end record found from the end, octal sizes, optional fields |
| [`storage.rs`][storage] | SQLite, FAT, GPT, USB | a sector size as a const parameter, a CRC over a header |
| [`link.rs`][link] | 802.11, Bluetooth, CAN, Modbus | bit fields, CRC-16, two lengths that must agree |
| [`routing.rs`][routing] | BGP, OSPF, GRE, VXLAN, IGMP | Fletcher checksums, fields present by flags |
| [`encodings.rs`][encodings] | DER, Protocol Buffers, CBOR | varints, shortest forms, C strings |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, STUN, RTP | DNS name pointers, options up to an end marker |
| [`executables.rs`][executables] | ELF, PE, Mach-O, WebAssembly | byte order and word size as type parameters, LEB128 |
| [`ethernet.rs`][ethernet] | Ethernet, IPv4, IPv6, UDP, TCP | EtherType tags, IP checksums, extension headers |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, SSH, MQTT | 24-bit lengths, variable-length integers |
| [`codegen.rs`][codegen] | none | the machine code of each kind of pun (`--features unstable`) |

## Testing

`puns` contains `unsafe` code, and its analysis decides when that code runs. A model check
compares the analysis with oracles written by hand on every pair of its test types, every night.
The tests run under [Miri] on x86-64, on 32-bit `i686` and on big-endian `s390x`. More than a
hundred programs must fail to compile with the expected message. [`docs/testing.md`][testing] has
more, and [Soundness] says what `puns` relies on.

## Performance

A proven pun adds no check. A pun by reference compiles to the computation of an address and a
length at most, and on x86-64 reading a `[bool; 4]` as a `u32` compiles to `mov eax, edi; ret`.
A checked pun runs about as fast as the same check written by hand: on an Apple M4 Max, under
load, checking an IPv4 packet with a 64-byte UDP datagram took 5.8 ns, against 5.3 ns by hand.
A check never allocates and never panics. [`docs/performance.md`][performance] has the other
cases, how they were measured, and the rules whose check time grows faster than the size of the
value.

## Comparison

`bytemuck` and `zerocopy` describe each type on its own with traits, such as "any bytes are a
valid value" or, in zerocopy, "these bytes can be checked at run time". `puns` describes each
type with a constant, and compares the source with the target. That is what makes `&[OldItem]`
to `&[Item]` free, and lets `&mut [bool; 4]` read as `&mut [[bool; 2]; 2]`. Rust's [safe
transmute] project compares two types too, in the compiler, but only on nightly. The rules of
the derive, such as sizes and checksums, are a separate feature, which these crates don't have.

`binrw`, `deku` and `nom` are parsers. They can build values of another shape than the bytes,
and their errors can say where parsing failed. `puns` only reads bytes as a type, and its
[`Error`] says why a read failed, not where.

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
