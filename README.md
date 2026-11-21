# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` reads a value of one type as a value of another type, and bytes as typed values, without
`unsafe`. Through a reference, nothing is copied. Use it instead of `transmute`, or instead of
parsing binary data by hand.

Such a conversion is a *pun*, from a *source* type to a *target* type. You derive `Pun` for your
types, and `puns` compares the two types of each pun at compile time. A pun is:

- **proven** if `puns` shows that every value of the source is a valid value of the target. It
  compiles to no check.
- **checked** if only some values of the source are valid, as when a `u8` is read as a `bool`, or
  if the address may not be aligned for the target. The `try_` and `from_` functions check at run
  time what `puns` could not prove. The other functions do not compile it.
- **refused** if it is never valid, or would read padding. It does not compile.

Because `puns` compares the two types, and not each type on its own, it can prove a pun such as
an enum of 0 and 1 read as a `bool`.

`puns` reads data whose fields are at fixed offsets, such as packets, file headers and records,
where only the last field varies in length. Data that must be decoded, such as compressed data or
text, still needs a parser.

```toml
[dependencies]
puns = "0.2"
```

`puns` is a `no_std` crate for Rust 1.99 and later. The API may change before 1.0. The
[documentation] describes every function and every attribute of the derive.

## Example

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is 0 or 1, which is a valid `bool`.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Checked: `from_bytes` checks that the length is a multiple of 4, and the `switch` of each lamp.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// Proven: every byte of a `Lamp` is initialized, so its bytes can be read.
assert_eq!(puns::as_bytes(lamps), bytes);
```

`on` and `lamps` borrow the existing data: nothing is copied. `u16le` is a little-endian `u16`
stored as 2 bytes, so a `Lamp` is 4 bytes without padding, and can be read at any address.

## The functions

`pun`, `pun_ref` and `pun_mut` read a whole value as another type. `head` and `tail` read its
first or last bytes. These functions accept only a proven pun. `try_pun`, `try_pun_ref` and
`try_pun_mut` also accept a checked pun, and so do the functions that read bytes: `from_bytes`,
`from_head` and `from_tail`. They return an [`Error`] when the check fails:

```rust
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

`pun` does not compile the same pun:

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

The message gives the reason and the byte, and the note gives the line of your code. The `...`
stands for about 30 lines about constants inside `puns`. [Compile-time errors] explains each
message and how to fix it.

Through a mutable reference, you can write any value of the target. So the `_mut` functions also
require every value of the target to be a valid source: a `&mut bool` cannot be read as a
`&mut u8`.

## Rules

The derive can also declare conditions that the bytes must meet:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))]
struct Hero {
  level: u8,
  hp: u8,
}

let mut bytes = [7, 42];
assert!(puns::from_bytes::<Hero>(&bytes).is_ok());

let hero: &mut Hero = puns::from_bytes_mut(&mut bytes).unwrap();
hero.hp = 99;
assert!(puns::from_bytes::<Hero>(&bytes).is_err());
```

A rule is checked when bytes are read as the type, not when a field is written. So a `&Hero`
does not promise that its rule still holds (see [Guarantees and requirements]). A pun between two
types that have the same rule over the same bytes is proven, and the rule is not checked again.

Rules are written in a small part of Rust: arithmetic and comparisons on unsigned integer fields,
`let`, `if`, and functions of `puns` such as `be`, `crc32` and `walk`, a loop. Other attributes
declare the valid values of a field, sizes and lengths stored in the data, regions and tags. The
[documentation of the derive][derive] describes them all.

## Reading two versions of a file

This example loads the save file of a game. Version 2 of the format adds a new kind of item, the
key. `load` reads a file of either version as the same type, without a copy.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// The items of version 1. With `#[repr(u8)]`, an item starts with a 1-byte tag. A sword is the
// tag 0 and a 2-byte damage, 3 bytes in all. A potion is 3 bytes too.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 adds the key. A sword and a potion keep their tag and their fields, and a key is
// 3 bytes too. So every `OldItem` is a valid `Item`, of the same size.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// One declaration for both versions. `V` is the type of the version byte, and `B` the type of
// the bag. By default, a `Save` has any version and a bag of `Item`s.
#[derive(Pun)]
#[repr(C)]
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // a big-endian `u32` that can only be "SAVE"
  version: V,
  bag: B, // the rest of the file
}

// A view enum: the derive gives it `from_bytes`, which returns the first variant that the bytes
// match. A `u8c<N>` is a `u8` that can only be `N`, so the version byte selects the variant.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    // Proven: every `OldItem` is a valid `Item`, and every `u8c<1>` a valid `u8`. Variants are
    // matched by their tag, not by their name: if the tag 1 of `Item` could not hold the fields
    // of a potion, this line would not compile.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

// A file of version 1: a sword of 300 damage, and a potion that heals 25.
let mut file = b"SAVE\x01\x00\x2c\x01\x01\x19\x00".to_vec();
assert_eq!(load(&file).unwrap().bag[0], Item::Sword { damage: 300.into() });

// The same file in version 2, with a key to door 0 of floor 3.
file[4] = 2;
file.extend_from_slice(&[2, 3, 0]);
assert_eq!(load(&file).unwrap().bag[2], Item::Key { floor: 3, door: 0 });
```

A view enum holds references and is not stored in the bytes, so it needs no `repr`.

## Choosing a representation

Give your types a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another
integer for an enum. The default representation, `repr(Rust)`, guarantees little about the
layout, so that the compiler can optimize it. The order of the fields, the tag of an enum and its
niches may change with the next version of the compiler, or at random with
[`-Zrandomize-layout`][randomize].

By default, `puns` accepts a pun of such a type only if whether it compiles, what it checks and
which values it accepts are the same in every layout that Rust allows. With the
[`unstable` feature][unstable], `puns` uses the layout that the compiler chose, and accepts more
puns. They are sound in the build that compiles them, but the next version of the compiler may
refuse them, or check other bytes.

Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
public. Do not derive it on a type with a private invariant ([What deriving `Pun` means][deriving]).

## Caveats

- `cargo check` does not report a pun refused inside a function. `cargo build` does. The refusal
  is a panic in a `const` block, which the compiler evaluates when it generates the code of the
  function.
- A generic function cannot require in its bounds that two types pun, since the analysis is a
  constant, not a trait. The error appears where the function is instantiated.
- Each pair of types takes about 17 ms of a debug build. A view enum whose variants overlap builds
  in time that grows as the square of its number of variants: 0.75 s for 16, 10 s for 64.
- An [`Error`] says whether the size, the alignment or a value is wrong, not where.
- A `walk` is a loop in a rule. A checksum or a `walk` inside a `walk` takes time in proportion
  to the square of the size of the value, and each further level multiplies it by the size again.
  Walks nest up to 4 deep, so a checksum inside the deepest one grows as the fifth power of the
  size. Keep this in mind before you read untrusted bytes.

## Performance

A proven pun compiles to nothing more than the computation of an address or a length:
`pun::<[bool; 4], u32>` is `mov eax, edi; ret` on x86-64. A checked pun compiles to its checks,
which never allocate or panic. Checking an IPv4 packet that holds 1472 bytes of UDP, both
checksums included, takes 22 ns, as long as the same check written by hand.
[`docs/performance.md`][performance] has more machine code, all the benchmarks, and the build
times.

## Examples

The [`examples`][examples] directory reads real formats in place, each declared with the derive.
Each example covers only the structures that it names, and says what it leaves out. Start with
`media.rs`, the shortest. Run an example with `cargo run --example media` in a clone of the
repository.

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

The documentation has shorter examples of each function in [Using the functions][using].

## Testing

`puns` uses `unsafe` code, and the analysis decides when that code runs. The tests check both:

- A model check compares the analysis with oracles written by hand, on every ordered pair of test
  types of the same size. It runs in full every night.
- The tests run under [Miri] on x86-64, on `i686` (32 bits) and on `s390x` (big-endian).
- More than 100 programs must fail to compile, each with its expected message.

[`docs/testing.md`][testing] describes the tests, and [Soundness] what `puns` assumes.

## Related crates

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They describe each type on its own, with marker traits. `puns` compares
the two types of each pun instead.

Rust's [safe transmute] project also compares two types, in the compiler. It requires nightly.

## History

I started this crate in November 2023, under the name `mutant`. At the time, I worked on
Bluetooth, which involves a lot of packing and unpacking of protocol data. [`typic`][typic] was
also a motivation. I was amazed by how much it achieves with the type system. Compile-time
evaluation had recently gained features on stable Rust, and inline `const` was being stabilized.
I wanted to see how far it could go.

Compile-time evaluation had a lot of limitations. I tried types instead, and found even more. I
wrote a lot of prototypes that mixed the two approaches. Then I stopped the project for a long
time.

Recent stabilizations and the progress of AI models made me want to come back to it. The name
`mutant` was taken, so the crate became `puns`. The [changelog] lists the releases since.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[documentation]: https://docs.rs/puns
[using]: https://docs.rs/puns/latest/puns/#using-the-functions
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[Guarantees and requirements]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[randomize]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
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
