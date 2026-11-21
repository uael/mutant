# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Safe, zero-copy type punning, checked at compile time.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`. It replaces
`transmute` and hand-written parsing of bytes. For each conversion, `puns` works out at compile
time what could go wrong. If nothing can, the conversion needs no check at run time. If something
can, `puns` checks that and nothing else. If the conversion can never be valid, your program does
not compile.

```sh
cargo add puns
```

`puns` is `no_std` and needs Rust 1.99 or later ([why][msrv]). It is young: the first release was
in November 2026, and the API may change before 1.0.

## Example

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// A `Switch` is always 0 or 1, so it is always a valid `bool`: no check.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Bytes are not always lamps: `from_bytes` checks the length, and that each `switch` is 0 or 1.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// Every byte of a lamp is initialized, so lamps are always valid bytes: no check.
assert_eq!(puns::as_bytes(lamps), bytes);
```

Each of these conversions is a *pun* from a *source* type to a *target* type. `#[derive(Pun)]`
records the layout and the valid values of a type, and `puns` compares the source with the target.
A pun is:

- **proven** when every value of the source is a valid value of the target, as for `Switch` to
  `bool`. It needs no check at run time.
- **checked** when only some values are valid, as for bytes to `Lamp`, or when the address may not
  be aligned for the target. The functions whose names start with `try_` or `from_` check it and
  return an [`Error`]. The other functions refuse it at compile time.
- **refused** when it is never valid, or when what it needs can be neither proven nor checked, such
  as reading padding. It does not compile.

A library that describes each type on its own has to check that a byte is 0 or 1 before it reads
the byte as a `bool`. Comparing the two types is what lets `puns` prove it. `on` and `lamps` borrow
the original data. `u16le` is a little-endian `u16` with an alignment of 1, so `Lamp` has no
padding and can be read at any address.

## Functions

| Reads | As | Proven only | Checked |
|---|---|---|---|
| a value | another type of the same size | `pun` | `try_pun` |
| a reference | a reference to another type | `pun_ref`, `pun_mut` | `try_pun_ref`, `try_pun_mut` |
| the first or last bytes of a value | another type | `head`, `tail`, and their `_ref` and `_mut` forms | |
| bytes | a typed reference | | `from_bytes`, `from_bytes_mut` |
| the first or last bytes of a slice | a typed reference, with the rest of the slice | | `from_head`, `from_tail`, and their `_mut` forms |
| a reference | its bytes | `as_bytes` | |

The functions that take a value need `Copy` types. The functions that return a `&mut` also need
the reverse pun, since you can write any value of the target through it: a `&mut bool` can become a
`&mut Switch`, but not a `&mut u8`.

`try_pun` checks a `u8` before it reads it as a `bool`. `pun` refuses the same pun:

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
...
note: the above error was encountered while instantiating `fn pun::<u8, bool>`
 --> src/main.rs:2:20
  |
2 |   let flag: bool = puns::pun(2u8);
  |                    ^^^^^^^^^^^^^^
```

The message says why and at which byte, and the note points at your code. About 30 lines inside
`puns` are left out at the `...`. [Compile-time errors] explains the common messages and how to
fix them.

The refusal is a panic in a `const` block, which the compiler evaluates when it generates the code
of the function. `cargo check` generates no code, so it misses refusals inside functions. Run
`cargo build`.

## Reading a binary format

Formats store sizes, lengths and checksums in their data. The derive declares them as *rules*,
which the `try_` and `from_` functions check:

```rust
use puns::{Pun, num::u32le};

// A record: the length of its text, a CRC-32 of the text, then the text in UTF-8.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 5 + length)]
#[pun(assert(crc32(5, 5 + length) == crc))]
struct Record {
  length: u8,
  crc: u32le,
  text: str,
}

// Reads every record of `bytes`, or returns the first error.
fn read(mut bytes: &[u8]) -> Result<Vec<&str>, puns::Error> {
  let mut texts = Vec::new();
  while !bytes.is_empty() {
    let (record, rest) = puns::from_head::<Record>(bytes)?;
    texts.push(&record.text);
    bytes = rest;
  }
  Ok(texts)
}

let mut stream = Vec::new();
for text in ["hello", "world"] {
  stream.push(text.len() as u8);
  stream.extend_from_slice(&puns::crc32(text.as_bytes()).to_le_bytes());
  stream.extend_from_slice(text.as_bytes());
}
assert_eq!(read(&stream), Ok(vec!["hello", "world"]));

stream[1] ^= 1; // a wrong checksum
assert_eq!(read(&stream), Err(puns::Error::Validity));
```

In a rule, a field stands for its value, and `crc32(5, 5 + length)` is the CRC-32 of the bytes of
the record from offset 5 to offset `5 + length`. `from_head` reads one record from the front of the
bytes, as long as its `size` rule says, and checks its rules and its UTF-8. Rules are Rust
expressions over the unsigned integer fields, with the functions that the derive provides, such
as `le`, `crc32`, and `walk`, a loop over the data. Other attributes give the valid values of a
field, the length of a slice, named regions of bytes, and tags. The [derive] documentation lists
them all.

A rule constrains the bytes that are read, not the type: safe code can still write a `length` that
breaks it. A pun between two types with the same rules over the same bytes is proven, and does not
check them again, so if the source no longer meets its rules, neither does the target. Otherwise,
the rules of the target are checked. [Guarantees and requirements] has the details.

## Reading two versions of a file

When every value of an old format is a valid value of the new one, a pun reads an old file as the
new type. Here, version 2 of a game's save file adds keys to the items.

A *view enum* holds one reference per variant. The derive gives it a `from_bytes` that returns the
first variant that the bytes match, so it can dispatch on the version byte. It is not stored in
the bytes, so it needs no `repr`.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// Version 1: each item is a 1-byte tag and 2 bytes of fields.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 keeps the tags and the fields of version 1, and adds a key of the same size.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

#[derive(Pun)]
#[repr(C)]
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // always "SAVE", as a big-endian `u32`
  version: V,
  bag: B,
}

// `u8c<N>` is a `u8` whose only valid value is `N`.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    SaveFile::V1(old) => puns::pun_ref(old), // proven: an `OldItem` is always a valid `Item`
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

// Version 1: a sword of 300 damage, and a potion that heals 25.
let mut file = b"SAVE\x01\x00\x2c\x01\x01\x19\x00".to_vec();
assert_eq!(load(&file).unwrap().bag[0], Item::Sword { damage: 300.into() });

// The same file in version 2, with a key to door 0 of floor 3.
file[4] = 2;
file.extend_from_slice(&[2, 3, 0]);
assert_eq!(load(&file).unwrap().bag[2], Item::Key { floor: 3, door: 0 });
```

Variants are matched by their tag, not by their name. If version 2 gave the tag 1 to the key,
`load` would still compile, and would read a potion of version 1 as a key, since its bytes are a
valid key. Keep the tags of existing variants.

## Representation

Give your types a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another
integer for an enum. Byte order and padding then depend on the types of the fields. The integers
of [`puns::num`][num], such as `u16le`, have a fixed byte order and an alignment of 1.

The default representation, `repr(Rust)`, guarantees little about the layout, so that the compiler
can optimize it. The order of the fields, the tag of an enum and its niches can change with the
next version of the compiler, or at random with [`-Zrandomize-layout`][randomize]. By default,
`puns` accepts a pun of such a type only when the result does not depend on these choices. For
example, it refuses to read `[u8; 2]` as `(bool, u8)`, since either byte could be the `bool`. With
the [`unstable` feature][unstable], `puns` uses the layout that the compiler chose, and accepts
more puns. They are sound in the build that compiles them, but the next version of the compiler
may refuse them, or check other bytes.

## Caveats

- Deriving `Pun` lets any code read and write every field of the type through bytes, as if the
  fields were public. Do not derive it on a type that keeps a private invariant. See
  [What deriving `Pun` means][deriving].
- A generic function cannot require in its bounds that two types pun, since the analysis is a
  constant, not a trait. When it is used with two types that do not pun, the error points at the
  pun inside the generic function.
- The analysis takes build time. On an Apple M4 Max, each pair of types of the benchmark took
  about 17 ms of a debug build. A view enum whose variants overlap took 10 s with 64 variants in
  debug, growing as the square of their number, and 25 s in release, growing faster.
  [`docs/performance.md`][performance] has the details.
- An [`Error`] says whether the size, the alignment or a value was wrong, not where. A view enum
  returns `Error::Validity` when no variant matches.
- A `walk` inside a `walk`, or a checksum inside a `walk`, takes time in proportion to the square
  of the length of the data. Each further level, up to 4 nested walks, multiplies it by the length
  again. Limit the length of untrusted input that such a type reads.

## Performance

A proven pun compiles to no check: `pun::<[bool; 4], u32>` is `mov eax, edi; ret` on x86-64. A
checked pun compiles to the checks that `puns` could not prove, which never allocate or panic. On
an Apple M4 Max, checking an IPv4 packet that carries 1472 bytes of UDP, both checksums included,
takes 22 ns, as long as the same check written by hand. [`docs/performance.md`][performance] has
the machine code of more puns and all the benchmarks.

## Examples

The [`examples`][examples] directory reads parts of real formats in place. Variable parts, such as
options, DNS names and variable-length integers, are regions of bytes that rules check and that
the example then reads with further puns. Each example says what it leaves out. Start with
`media.rs`, the shortest, with `cargo run --example media` in a clone of the repository.

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

The documentation has a short example of each function in [Using the functions][using].

## Compared with other crates

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They describe each type on its own, with marker traits such as "any bytes
are a valid value". `puns` compares the two types of each pun instead, which is how it proves
`Switch` to `bool` or reads an old version of a format as the new one. In return, they have years
of use and review, functions that allocate, and errors that `cargo check` and editors report,
since theirs are trait bounds.

To decode data into owned values, or a format whose fields are not at fixed offsets, a parser such
as [`binrw`][binrw], [`deku`][deku] or [`nom`][nom] fits better. Rust's [safe transmute] project
also compares two types, in the compiler, and requires nightly.

## Testing

`puns` uses `unsafe` code, and the analysis decides when that code runs. The tests check both:

- A model check compares the analysis with oracles written by hand, on every ordered pair of test
  types of the same size. It runs in full every night.
- The tests run under [Miri] on x86-64, on `i686` (32 bits) and on `s390x` (big-endian).
- More than 100 programs must fail to compile, each with its expected message.

[`docs/testing.md`][testing] describes the tests, and [Soundness] what `puns` assumes. If you find
a soundness bug, please open an issue.

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

[msrv]: https://docs.rs/puns/latest/puns/#minimum-rust-version
[using]: https://docs.rs/puns/latest/puns/#using-the-functions
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
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
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
