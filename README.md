# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value of one type as a value of another type. It compares the two types at compile
time. Each pun is then proven (nothing is checked), checked at run time, or refused (it does not
compile).

```toml
[dependencies]
puns = "0.1"
```

Requires Rust 1.99 or later. Works without `std`. The API may change before 1.0.

## Usage

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: every `Switch` is 0 or 1, so it is a valid `bool`. Nothing is checked.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Checked: `from_bytes` checks the length, and the `switch` of each lamp.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// Proven: every `Lamp` is 4 initialized bytes. Nothing is checked.
assert_eq!(puns::as_bytes(lamps), bytes);
```

`on` and `lamps` are references to the existing data. Nothing is copied. `u16le` is a
little-endian `u16` with an alignment of 1, so `Lamp` has no padding.

Two notes on the derive:

- Give your types a `repr`. Without one, Rust may reorder the fields, and `puns` refuses a pun
  whose checks depend on the field order. The [`unstable` feature][unstable] removes this
  restriction.
- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type that has a private invariant. See
  [What deriving `Pun` means][deriving].

## Reading two versions of a file

This example loads the save file of a game. Version 2 of the format adds keys to the items. `load`
reads a file of either version as the same type, without a copy.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// The items of version 1. With `#[repr(u8)]`, an item starts with a 1-byte tag. A sword is the
// tag 0, then its damage: 3 bytes. A potion is 3 bytes too.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 adds keys. A sword and a potion keep their tag and their fields, and a key is also
// 3 bytes. So every `OldItem` is a valid `Item`.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// One declaration for both versions. `V` is the type of the version byte, and `B` is the type
// of the bag. By default, a `Save` has any version and a bag of `Item`s.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // a rule, checked when bytes are read as a `Save`
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // a big-endian `u32` that is always "SAVE"
  version: V,
  level: u8,
  hp: u16le,
  bag: B, // the rest of the file
}

// A view enum. `from_bytes` returns the first variant that the bytes match. A `u8c<N>` is a
// `u8` that is always `N`, so the version byte selects the variant. A view enum holds
// references and is not stored in the bytes, so it needs no `repr`.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    // Proven: every `OldItem` is a valid `Item`, a `u8c<1>` is a `u8`, and both sides have the
    // same rule. If `Potion` had another tag in `Item`, this line would not compile.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

// A file of version 1: level 7, 42 hp, a sword of 300 damage and a potion that heals 25.
let mut file = b"SAVE\x01\x07\x2a\x00\x00\x2c\x01\x01\x19\x00".to_vec();
assert_eq!(load(&file).unwrap().bag[0], Item::Sword { damage: 300.into() });

// Change the file to version 2, and add a key to floor 3.
file[4] = 2;
file.extend_from_slice(&[2, 3, 0]);
assert_eq!(load(&file).unwrap().bag[2], Item::Key { floor: 3, door: 0 });

// Set hp to 999. A write to a field is not checked. It breaks the rule here, so `load` now
// fails.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```

A rule such as `assert` is checked when bytes are read as the type. A write to a field does not
check it. See [Guarantees and requirements][guarantees].

The derive has other attributes, for valid ranges, sizes stored in the data, checksums and tags.
The [examples](#examples) use them, and the [documentation of the derive][derive] describes them.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size, its alignment,
its padding, and the valid values of each field. Each function in `puns` compares the constants of
its two types in a `const` block. The compiler evaluates this block for each pair of types that
the function is used with.

`pun`, `pun_ref`, `pun_mut` and `as_bytes` accept only a proven pun. The `try_` and `from_`
functions also accept a checked pun, and return an [`Error`] when the check fails:

```rust
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

`pun` refuses the same pun:

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
 --> src/main.rs:2:20
  |
2 |   let flag: bool = puns::pun(2u8);
  |                    ^^^^^^^^^^^^^^
```

The first lines give the reason and the byte. The last note gives the place in your code. The
`...` stands for about 30 lines that describe constants inside `puns`. [Compile-time errors]
explains each message.

Some puns are refused by the `try_` functions too, such as a pun that would read padding.

## Caveats

- `cargo check` does not report a refused pun. `cargo build` does. The error comes from code
  generation.
- For the same reason, a generic function cannot require in its bounds that two types pun. The
  error appears where the function is instantiated.
- The analysis takes build time for each new pair of types. The cost of a view enum grows with
  the square of its number of variants.
- An [`Error`] says whether the size, the alignment or a value is wrong. It does not say where.

## Examples

The [`examples`][examples] directory declares real formats with the derive only, and reads them in
place. Each example covers only the structures that it names, and says what it does not cover.
The table goes from the shortest example to the longest. Run one with
`cargo run --example media`, from a clone of the repository.

| Example | Formats | Shows |
|---|---|---|
| [`media.rs`][media] | WAVE, MP4 boxes | four-character codes, a pad byte, box sizes of 32 or 64 bits |
| [`images.rs`][images] | PNG, BMP, font tables | a CRC per chunk, the order of chunks, sorted records |
| [`archives.rs`][archives] | ZIP, tar, gzip | an end record found from the end of the file, octal sizes, optional fields |
| [`storage.rs`][storage] | SQLite, FAT, GPT, USB | a sector size as a const parameter, a CRC over a header |
| [`link.rs`][link] | 802.11, Bluetooth, CAN, Modbus | bit fields, CRC-16, two lengths that must agree |
| [`routing.rs`][routing] | BGP, OSPF, GRE, VXLAN, IGMP | Fletcher checksums, optional fields announced by flags |
| [`encodings.rs`][encodings] | DER, Protocol Buffers, CBOR | varints, shortest forms, C strings |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, STUN, RTP | DNS compression pointers, options up to an end marker |
| [`executables.rs`][executables] | ELF, PE, Mach-O, WebAssembly | byte order and word size as type parameters, LEB128 |
| [`ethernet.rs`][ethernet] | Ethernet, IPv4, IPv6, UDP, TCP | EtherType tags, IP checksums, extension headers |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, SSH, MQTT | 24-bit lengths, variable-length integers |

The crate documentation has shorter examples in [Using the functions][using].

## Testing

`puns` uses `unsafe` code, and the analysis decides when this code runs. So the tests check the
analysis:

- A model check compares the analysis with oracles written by hand, on every ordered pair of test
  types of the same size. It runs in full every night.
- The tests run under [Miri] on x86-64, on `i686` (32 bits) and on `s390x` (big-endian).
- More than 100 programs must fail to compile, each with its expected message.

See [`docs/testing.md`][testing], and [Soundness] for what `puns` assumes.

## Performance

A proven pun compiles to no check. For example, `pun::<[bool; 4], u32>` compiles to
`mov eax, edi; ret` on x86-64, in a release build. A check does not allocate and does not panic.
[`docs/performance.md`][performance] has the machine code of more functions, and benchmarks
against the same checks written by hand.

The time of a check can grow with the square of the size of the value. This happens when a `walk`
(a loop in a rule) contains a checksum or another `walk`.

## Related crates

[`zerocopy`][zerocopy] and [`bytemuck`][bytemuck] have been major sources of information and of
implementation details for `puns`. They describe each type on its own, with marker traits. `puns`
compares the two types of a pun. This is how it proves a pun such as `Switch` to `bool`.

Rust's [safe transmute] project also compares two types, in the compiler. It requires nightly.

## History

I started this crate in November 2023, under the name `mutant`. At the time, I worked on
Bluetooth, which involves a lot of packing and unpacking of protocol data. [`typic`][typic] was
also a motivation. I was amazed by how much it achieves with the type system. Compile-time
evaluation had recently gained features on stable Rust, and inline `const` was being stabilized.
I wanted to see how far it could go, and to push what the compiler can do.

Compile-time evaluation had a lot of limitations. I tried types instead, and found even more. I
wrote a lot of prototypes that mixed the two approaches. Then I stopped the project for a long
time.

Recent stabilizations and the progress of AI models made me want to come back to it. The name
`mutant` was taken, so the crate became `puns`.

## Documentation

- [Reference][reference]: every function, by what it reads and how much of it.
- [`#[derive(Pun)]`][derive] and its attributes, and the integer types of [`puns::num`][num].
- [Compile-time errors], [Soundness], [Cargo features][features],
  [Minimum Rust version][msrv], [Limitations].
- [`CHANGELOG.md`][changelog].

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[reference]: https://docs.rs/puns/latest/puns/#reference
[using]: https://docs.rs/puns/latest/puns/#using-the-functions
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[guarantees]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[features]: https://docs.rs/puns/latest/puns/#cargo-features
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[msrv]: https://docs.rs/puns/latest/puns/#minimum-rust-version
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[images]: https://github.com/uael/puns/blob/main/puns/examples/images.rs
[archives]: https://github.com/uael/puns/blob/main/puns/examples/archives.rs
[storage]: https://github.com/uael/puns/blob/main/puns/examples/storage.rs
[link]: https://github.com/uael/puns/blob/main/puns/examples/link.rs
[routing]: https://github.com/uael/puns/blob/main/puns/examples/routing.rs
[encodings]: https://github.com/uael/puns/blob/main/puns/examples/encodings.rs
[ip_services]: https://github.com/uael/puns/blob/main/puns/examples/ip_services.rs
[executables]: https://github.com/uael/puns/blob/main/puns/examples/executables.rs
[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[transport]: https://github.com/uael/puns/blob/main/puns/examples/transport.rs
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[Miri]: https://github.com/rust-lang/miri
[bytemuck]: https://crates.io/crates/bytemuck
[zerocopy]: https://crates.io/crates/zerocopy
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
