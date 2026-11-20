# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` reads a value of one type as a value of another, with no `unsafe` or `transmute` in your
code. It compares the two types at compile time: a pun that every value satisfies costs nothing,
one that only some values satisfy is checked at run time, and one that can't work doesn't
compile.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, `no_std`, and no dependency but its derive macro. `puns` is young: it is at
0.1, and its API may change before 1.0.

## Usage

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Read bytes as lamps. The length and each switch are checked.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });

// A switch of 2 is neither off nor on.
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// View the lamps as bytes, and a switch as a `bool`. Neither is checked.
assert_eq!(puns::as_bytes(lamps), bytes);
let on: &bool = puns::pun_ref(&lamps[0].switch);
assert!(*on);
```

`from_bytes` checks that the length is a multiple of 4 and that each switch is 0 or 1. It needs
no address check, since a `u16le` is a little-endian `u16` with an alignment of 1. Crates like
`bytemuck` and `zerocopy` can't read a `Switch` as a `bool` without a check: they know that a
`bool` is 0 or 1, but nothing relates it to `Switch`. `puns` compares the two types, and sees
that both values of `Switch` are valid `bool`s.

The `#[repr(C)]` keeps the fields in the order they're declared. Without it, Rust may reorder
them, so `puns` refuses a pun whose validity depends on that order, and you can't tell which byte
lands in which field. The `unstable` feature, which works on stable Rust, uses the order that
`rustc` chose instead ([the `unstable` feature][unstable]).

Deriving `Pun` lets any code read and write every field of the type through bytes. Don't derive
it on a type whose fields keep a private invariant ([what deriving `Pun` means][deriving]).

## Reading a binary format

The derive can also check what a format stores in its bytes, such as a size or a checksum. This
reads the messages of a small protocol one after the other.

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
```

`from_head` reads one message from the front of the bytes, and returns the bytes after it. The
`size` rule says where a message ends. A `u32bec<N>` is a big-endian `u32` that always holds `N`,
which is how the magic number is checked. The derive also takes `valid`, `assert`, checksums and
tags. A rule is an expression over the unsigned integer fields of the struct, with a few
functions to read its bytes, and is checked each time bytes are read as the type. Safe code can
still assign a field that breaks a rule. The [derive's documentation][derive] lists what rules
can say.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
which bytes are padding, and which values each field may hold. Each function compares the
constants of its two types in a `const` block, which the compiler evaluates when it generates
the function for those types. A pun is either:

- **proven**, if every value of the source is a [valid] value of the target, with no padding
  where the target needs a byte;
- **checked**, if only some values are, or if the length or the address of the bytes matters;
- or **refused**, if it can't work.

`pun`, `pun_ref` and `as_bytes` take only proven puns. The `try_` and `from_` functions also
take checked ones, and check at run time what wasn't proven. So `u8` to `bool` is checked, and
`pun` refuses it:

```rust
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
(33 lines that locate the failed constant inside puns)
note: the above error was encountered while instantiating `fn pun::<u8, bool>`
 --> src/main.rs:2:20
  |
2 |   let flag: bool = puns::pun(2u8);
  |                    ^^^^^^^^^^^^^^
```

Since these errors come from code generation, `cargo check` doesn't report them, and `cargo
build` does. The analysis also costs build time, most of all for an enum of references with many
variants, whose build time grows with the square of their number. Every function is a `const fn`,
so bytes in a `const` are checked at compile time too.

## Examples

[`docs/save-file.md`][save-file] walks through a save file that two versions of a game read,
with enums that gain a variant, rules on fields, and a loader for both versions. The examples
below each declare a family of formats with the derive alone, and read them in place. They cover
the structures they name, not whole formats, and are listed from the shortest. From a clone of
the repository, run one with `cargo run --example media`.

| Example | Formats | Shows |
|---|---|---|
| [`media.rs`][media] | WAVE, MP4 boxes | four-character tags, a pad byte, 32-bit or 64-bit sizes |
| [`images.rs`][images] | PNG, BMP, font tables | a CRC per chunk, the order of chunks, sorted records |
| [`archives.rs`][archives] | ZIP, tar, gzip | a ZIP read from its end, octal sizes, optional fields |
| [`storage.rs`][storage] | SQLite, FAT, GPT, USB | a sector size as a const parameter, a CRC over a header |
| [`link.rs`][link] | 802.11, Bluetooth, CAN, Modbus | bit fields, CRC-16, two lengths that must agree |
| [`routing.rs`][routing] | BGP, OSPF, GRE, VXLAN, IGMP | Fletcher checksums, optional fields set by flags |
| [`encodings.rs`][encodings] | DER, Protocol Buffers, CBOR | varints, shortest forms, C strings |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, STUN, RTP | DNS name pointers, options up to an end marker |
| [`executables.rs`][executables] | ELF, PE, Mach-O, WebAssembly | byte order and word size as type parameters, LEB128 |
| [`ethernet.rs`][ethernet] | Ethernet, IPv4, IPv6, UDP, TCP | EtherType tags, IP checksums, extension headers |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, SSH, MQTT | 24-bit lengths, variable-length integers |

## Testing

`puns` has `unsafe` code inside, and its analysis decides when that code runs. A model check
compares the analysis with oracles written by hand, every night. The tests run under [Miri] on
x86-64, on 32-bit `i686` and on big-endian `s390x`, and more than a hundred programs must fail
to compile with the expected message. [`docs/testing.md`][testing] has more.

## Performance

A proven pun by reference compiles to an address and a length at most: on x86-64, reading a `[bool;
4]` as a `u32` is `mov eax, edi; ret`. A check never allocates and never panics. Against the same
checks written by hand, on an Apple M4 Max under load, 14 of 33 checks took within 15% of the time,
2 took longer (by 16% and 52%), and 17 took less, several of them because the hand-written check
uses a simpler algorithm. [`docs/performance.md`][performance] has every case, how it was measured,
and the rules whose check time grows faster than the value.

## Comparison

Use `puns` to read one type as another when the two aren't plain bytes, such as an enum as a
`bool` or an old version of an enum as a new one, or `&mut` between types with invalid values,
or to check a format's sizes and checksums with the type. `bytemuck` and `zerocopy` give each
type on its own traits such as "any bytes are valid" or "these bytes can be checked", so they
check what `puns` proves from the pair. Rust's [safe transmute] project compares two types in the
compiler, but only on nightly.

Use `zerocopy` or `bytemuck` if you need a Rust older than 1.99, errors that `cargo check` and
your editor report, or a crate with years of use. Use a parser such as `binrw`, `deku` or `nom`
to build values of a different shape than the bytes, or to get errors that say where in the
bytes a read failed: a [`puns::Error`][`Error`] says only whether the size, the alignment or a
value was wrong.

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

[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
