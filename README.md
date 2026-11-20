# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` is type punning without `unsafe`: it reads a value of one type as a value of another, such
as bytes as a struct, a struct as bytes, or an enum as a `bool`. It compares the two types at
compile time, to find which values of the source are valid values of the target. `pun` and
`pun_ref` accept a pun only if `puns` proves it for every value, and don't compile otherwise.
`try_pun`, `from_bytes` and the other checked functions also accept a pun that only some values
pass, and check at run time what wasn't proven. The functions on references read in place,
without copying.

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later, and `no_std` with no feature to turn off. A pun that `puns` refuses is an
error of `cargo build`, not of `cargo check` ([why](#how-it-works)). `puns` is young: it is at
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

`lamps` borrows `bytes`, and `on` borrows the switch: nothing is copied. A `u16le` is a
little-endian `u16` with an alignment of 1, so a `Lamp` can start at any address and has no
padding. A struct with padding can be read from bytes, but `as_bytes` refuses it, since padding
may be uninitialized.

`#[repr(C)]` keeps the fields in their declared order. Without it, Rust may reorder them, and
`puns` refuses any pun whose validity depends on that order.

Deriving `Pun` lets any code read every field of the type through bytes, and set it to any valid
value, as if the fields were public. So don't derive it on a type whose fields keep a private
invariant ([what deriving `Pun`
means][deriving]).

## Comparison

[`bytemuck`][bytemuck] and [`zerocopy`][zerocopy] are the established crates for this, and `puns`
owes them a lot: both have long been major sources for me, of information and of implementation
details. They describe each type on its own, with traits. `puns` compares the two types of a pun,
which lets it prove a pun that depends on both: a `Switch` read as a `bool`, or the items of an
old version read as those of a new one, as in the [save file](#reading-two-versions-of-a-file)
below.

Use `zerocopy` or `bytemuck` if you need errors that `cargo check` and your editor report, or a
crate with years of use. Rust's [safe transmute] project compares two types too, in the compiler,
but only on nightly.

Use a parser such as `binrw`, `deku` or `nom` to build values of a different shape than the
bytes, or to get errors that say where a read failed. A [`puns::Error`][`Error`] only says whether
the size, the alignment or a value was wrong.

## Reading two versions of a file

Version 2 of a game adds keys to the items of its save file. Its `load` reads a file of either
version as one type, `Save`, without copying it, and checks each file once.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// The items of version 1. With `#[repr(u8)]`, an item starts with its tag: a sword is a 0, then
// its damage, 3 bytes in all.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 adds keys. Swords and potions keep their tags and their fields, and a key fits in the
// 3 bytes of a sword, so every `OldItem` is a valid `Item`.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// One declaration for both versions: `V` is the type of the version, and `B` of the items.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // a rule, checked when bytes are read as a `Save`
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // a big-endian `u32` that always holds "SAVE"
  version: V,
  level: u8,
  hp: u16le,
  bag: B, // the rest of the file
}

// A view enum: an enum of references, whose `from_bytes` returns the first variant that the
// bytes match. Its tag is never in the bytes, so it needs no `repr`. A `u8c<N>` is a `u8` that
// always holds `N`, so the version byte decides.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    // Every old item is a valid item, and both versions have the same rule, so this checks
    // nothing. If `Item` gave potions the tag 3, it would not compile, and `try_pun_ref` would
    // check each item.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

// Version 1, level 7, 42 hp, a sword of 300 damage and a potion that heals 25.
let mut file = b"SAVE\x01\x07\x2a\x00\x00\x2c\x01\x01\x19\x00".to_vec();
assert_eq!(load(&file).unwrap().bag[0], Item::Sword { damage: 300.into() });

// Upgrade the file to version 2, and pick up a key to floor 3.
file[4] = 2;
file.extend_from_slice(&[2, 3, 0]);
assert_eq!(load(&file).unwrap().bag[2], Item::Key { floor: 3, door: 0 });

// The rule is a requirement of the format, not of the type, and writing a field doesn't check
// it. A cheater gives the hero 999 hp, and the next `load` refuses the file.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```

So what a `&Save` guarantees depends on where it came from. Its types always hold: each item has a
known tag, and `magic` holds "SAVE". The rule held when the bytes were read. See [guarantees and
requirements][guarantees].

Besides `assert`, the derive takes ranges of valid values, sizes stored in the bytes, checksums
and tags: the [examples](#examples) use them on real formats, and the [derive's
documentation][derive] describes them.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
which bytes are padding, and which values each field may hold. Each function of `puns` compares
the constants of its two types in a `const` block, which the compiler evaluates when it generates
the function for those types.

Which puns compile depends on the function. `pun`, `pun_ref` and `as_bytes` accept only puns
proven for every value. The `try_` and `from_` functions also accept puns proven for some values,
and check the others at run time. So `try_pun` reads a `u8` as a `bool`, and `pun` refuses to:

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
[33 lines omitted: the constants inside `puns` that panicked]
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

The analysis costs build time for each pair of types. The most expensive is a view enum, whose
build time grows with the square of the number of its variants. [Limitations] lists what the
analysis can't prove yet, such as one rule from another rule that implies it.

The `unstable` feature, which works on stable Rust, makes `puns` use the layout that `rustc`
chose where Rust leaves it to the compiler, such as the order of the fields of a struct without
`repr`. A pun may then compile with one compiler and not with the next, and bytes written by one
build may read as other values in the next. See [the `unstable` feature][unstable].

## Examples

Each example declares a family of formats with the derive alone, and reads them in place. They
cover the structures they name, not whole formats, and the shortest come first. From a clone of
the repository, run one with `cargo run --example media`. `media.rs` is the one to read first.

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

`puns` has `unsafe` code inside, and its analysis decides when that code runs. Every night, a
test compares the verdicts of the analysis with checks written by hand that don't use it, on
every pair of the types it tests.
The tests also run under [Miri] on x86-64, on 32-bit `i686` and on big-endian `s390x`, and more
than a hundred programs must fail to compile with the expected message. See
[`docs/testing.md`][testing], and [Soundness] for what `puns` relies on.

## Performance

A pun proven for every value compiles to no check: on x86-64, in a release build,
`pun::<[bool; 4], u32>` compiles to `mov eax, edi; ret`. A check never allocates or panics, but
its time can grow faster than the size of the value. A `walk`, which steps through records whose
sizes are in the bytes, takes time that grows with the square of the size when it contains a
checksum or another `walk`. Keep that in mind before giving such a type untrusted bytes.

In the cases measured, most checks took about as long as the same check written by hand. Timed on an
Apple M4 Max under load, checking an IPv4 packet with a 1,472-byte UDP datagram took 25 ns, like the
code by hand, and walking 12 IPv6 extension headers took 7 ns, against 12 ns. The largest slowdown
among the cases measured reads 64 KiB as an enum whose tag is a niche in a `bool`, which needs the
`unstable` feature: 29 µs, against 19 µs. [`docs/performance.md`][performance] has every case, and
how they were measured.

## History

I started `puns` in November 2023, on nightly, under the name `mutant`. I was working on
Bluetooth at the time, which meant packing and unpacking a lot of protocol structures, and
[`typic`][typic] had amazed me with how much it got out of the type system. I wanted to see how
far the same idea could go with compile-time evaluation instead. Rust was stabilizing more of it:
inline `const` blocks were on their way, and a `const` block in a generic function can refuse to
compile for some types.

Compile-time evaluation had too many limits then. From October 2024 to March 2025 I tried types
instead, as `typic` does, and ran into even more. After many prototypes that mixed both
approaches, I put the project aside.

New stabilizations, and how far AI models had come, brought me back to it in November 2026. Rust
1.99 brought the last piece that stable Rust lacked: the size and alignment of an unsized type in
a `const`. Layouts became `const` values, compared by ordinary `const fn` code, and since the name
`mutant` was taken, the crate became `puns`.

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
[typic]: https://github.com/jswrenn/typic
[bytemuck]: https://crates.io/crates/bytemuck
[zerocopy]: https://crates.io/crates/zerocopy
[guarantees]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
