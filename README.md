# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning without `unsafe`: read bytes as a struct, a struct as bytes, or one type as another,
in place. Unlike crates that describe each type on its own, `puns` compares the two types of a
pun at compile time. It proves what holds for every value, checks the rest at run time, and
refuses to compile what it can do neither for.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, `no_std`. `puns` is young: it is at 0.1, and its API may change before 1.0.

## Usage

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// A switch is 0 or 1, so every switch is a valid `bool`, and this compiles to no check.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Bytes can be any value, so reading them as lamps checks the length and each switch.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// And every lamp is four initialized bytes, so this compiles to no check either.
assert_eq!(puns::as_bytes(lamps), bytes);
```

`lamps` and `on` are references into `bytes` and into the switch: nothing is copied. A `u16le` is
a little-endian `u16` with an alignment of 1, so a `Lamp` can start at any address. `bytemuck`
and `zerocopy` would check the switch before reading it as a `bool`, since they know what a
`bool` accepts but don't relate it to `Switch`.

The `#[repr(C)]` keeps the fields in their declared order. Without it Rust may reorder them, so
`puns` refuses any pun whose validity depends on the order. With the `unstable` feature, `puns`
uses the order that `rustc` chose for this build instead. The feature works on stable Rust, but a
pun that compiles with one compiler may not with another; see [the `unstable` feature][unstable].

Deriving `Pun` lets any code read and write every field of the type through bytes, so don't
derive it on a type whose fields keep a private invariant. See [what deriving `Pun`
means][deriving].

## Reading a binary format

A format often stores its own size, a magic number or a checksum. The derive can check those as
it reads the bytes. This reads the messages of a small protocol one after the other:

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
`size` rule tells it where a message ends. A `u32bec<N>` is a big-endian `u32` that always holds
`N`, which is how the magic number is checked.

Rules can also restrict values (`valid`), state any condition (`assert`), and compute checksums.
A rule is an expression over the unsigned integer fields of the struct, with a few functions that
read its bytes; the [derive's documentation][derive] lists them. Rules are checked when bytes are
read as the type, not afterwards. Safe code can assign a field that breaks a rule, and only the
next read of those bytes will notice.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
which bytes are padding, and which values each field may hold. Each function of `puns` compares
the constants of its two types in a `const` block, which the compiler evaluates when it generates
the function for those types.

The function decides what may happen. `pun`, `pun_ref` and `as_bytes` take only puns that hold
for every value. The `try_` and `from_` functions also take puns that hold for some values, and
check the rest at run time. Anything else does not compile. So reading a `u8` as a `bool` is a
`try_pun`, and `pun` refuses it:

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

[Compile-time errors] explains each message, and how to fix it. These errors come from code
generation, so `cargo check` doesn't report them, and `cargo build` does. For the same reason, a
generic function can't say in its bounds that its types pun: a caller that instantiates it with
the wrong types gets the error in its own build.

The analysis costs build time for each pair of types. The most expensive is a view enum, an enum
of references that reads whichever of several formats matches: its build time grows with the
square of its variants.

## Examples

[`docs/save-file.md`][save-file] is the best next step. It reads the save file of a game in two
versions, with enums that gain a variant, rules on fields, and a loader for both.

The examples below each declare a family of formats with the derive alone, and read them in place.
They cover the structures they name, not whole formats, and the shortest come first. From a clone
of the repository, run one with `cargo run --example media`.

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

`puns` has `unsafe` code inside, and its analysis decides when that code runs. A test compares
the analysis with oracles written by hand, on every pair of a set of types, every night. The
tests also run under [Miri] on x86-64, on 32-bit `i686` and on big-endian `s390x`, and more than
a hundred programs must fail to compile with the expected message. See
[`docs/testing.md`][testing], and [Soundness] for what `puns` relies on.

## Performance

A pun that holds for every value compiles to no check: on x86-64, `pun::<[bool; 4], u32>` is
`mov eax, edi; ret`. A check never allocates and never panics. 33 checks were timed against the
same checks written by hand, on an Apple M4 Max under load. 14 took within 15% of the time by
hand, 17 took less, several because the hand-written check uses a simpler algorithm, and 2 took
more, by 16% and 52%. Measure in release builds, since the checks rely on inlining.
[`docs/performance.md`][performance] has every case, and how they were measured.

## Comparison

Use `puns` to read one type as another when neither is plain bytes, such as an enum as a `bool`
or the items of an old version as those of a new one. Use it for `&mut` puns between types that
restrict their values, such as `&mut [bool; 4]` as `&mut [[bool; 2]; 2]`, and to check the sizes
and checksums of a format as part of its type.

Use `zerocopy` or `bytemuck` if you need a Rust older than 1.99, errors that `cargo check` and
your editor report, or a crate with years of use. They check a value of a restricted type when
they read it, where `puns` can prove from the pair of types that no check is needed. Rust's [safe
transmute] project compares two types too, in the compiler, but only on nightly.

Use a parser such as `binrw`, `deku` or `nom` to build values of another shape than the bytes, or
to get errors that say where a read failed. A [`puns::Error`][`Error`] only says whether the size,
the alignment or a value was wrong.

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
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
