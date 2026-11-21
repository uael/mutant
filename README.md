# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`. It decides at
compile time what each conversion needs: what it can prove costs nothing at run time, what it
cannot prove is checked, and a conversion that can never be valid does not compile.

```sh
cargo add puns
```

`puns` is a `no_std` crate. It requires Rust 1.99 or later, and its API may change before 1.0.
The [documentation] describes every function and every attribute of the derive.

## A first example

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is 0 or 1, and both are valid `bool`s.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Checked: the length must be a multiple of 4, and each `switch` must be 0 or 1.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// Proven: every byte of a `Lamp` is initialized, so its bytes can be read.
assert_eq!(puns::as_bytes(lamps), bytes);
```

Each of these conversions is a *pun*, from a *source* type to a *target* type. `#[derive(Pun)]`
describes the layout of a type and its valid values, and `puns` compares the descriptions of the
source and the target at compile time. A pun is then:

- **proven** if every value of the source is a valid value of the target. It costs nothing at run
  time.
- **checked** if only some values of the source are valid, as when a `u8` is read as a `bool`, or
  if the address of the source may not be aligned for the target. A function whose name starts
  with `try_` or `from_` checks it at run time, and the others reject it at compile time.
- **refused** if it is never valid, or if what it needs can be neither proven nor checked, such as
  reading padding. It does not compile.

Because `puns` looks at both types, it proves that a `Switch` is a `bool`, where a description of
each type alone would only allow a check. `on` and `lamps` borrow the original data, and nothing
is copied. `u16le` is a little-endian `u16` with an alignment of 1, so a `Lamp` has no padding and
can be read at any address.

## Choosing a function

| You have | You want | Function |
|---|---|---|
| a value | the same bytes as another type | `pun`, or `try_pun` to check |
| a reference | a reference to another type | `pun_ref` and `pun_mut`, or `try_pun_ref` and `try_pun_mut` |
| a value or a reference | its first or last bytes as another type | `head` and `tail`, with `_ref` and `_mut` |
| bytes | a typed reference | `from_bytes` and `from_bytes_mut` |
| bytes | a typed reference to their start or end, and the rest | `from_head` and `from_tail`, with `_mut` |
| a reference | its bytes | `as_bytes` |

The functions that take a value require `Copy` types. The ones that return a mutable reference
also require the reverse pun: you can write any value of the target through the reference, so each
must be a valid source. A `&mut bool` can be read as a `&mut Switch`, but not as a `&mut u8`.

The `try_` and `from_` functions return an [`Error`] when the check fails. The others accept
only proven puns, and refuse the rest when the program is built:

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

The message says why and at which byte, and the note points at your code. The `...` replaces about
30 lines about constants inside `puns`. [Compile-time errors] explains the most common messages
and how to fix them.

The refusal is a panic in a `const` block, which the compiler evaluates when it generates code for
the function. `cargo check` does not generate code, so it does not report a refused pun inside a
function: run `cargo build`.

## Reading a binary format

The derive can also state what the data says about itself: a size or a length stored in it, a
checksum, and other conditions that its bytes must meet.

```rust
use puns::{Pun, num::u32le};

// A record: the length of its text, the CRC-32 of the text, then the text in UTF-8.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 5 + length)]
#[pun(assert(crc32(5, 5 + length) == le(1, 4)))]
struct Record {
  length: u8,
  crc: u32le,
  text: str,
}

let mut stream = Vec::new();
for text in ["hello", "world"] {
  stream.push(text.len() as u8);
  stream.extend_from_slice(&puns::crc32(text.as_bytes()).to_le_bytes());
  stream.extend_from_slice(text.as_bytes());
}
stream.push(0xff);

// `from_head` reads one record from the front of the bytes, and returns the bytes after it.
let mut rest = &stream[..];
let mut texts = Vec::new();
while let Ok((record, after)) = puns::from_head::<Record>(rest) {
  texts.push(&record.text);
  rest = after;
}
assert_eq!(texts, ["hello", "world"]);
assert_eq!(rest, [0xff]);

// A wrong checksum is an error.
stream[1] ^= 1;
assert!(puns::from_head::<Record>(&stream).is_err());
```

These rules are written in a small part of Rust: arithmetic and comparisons on unsigned integer
fields, `let`, `if`, and functions of `puns` such as `le`, `crc32` and `walk`, which loops over the
data. Other attributes declare the valid values of a field, the length of a slice, regions of
bytes and tags. The [documentation of the derive][derive] describes them all.

A rule is a requirement on the bytes that are read, not a guarantee of the type. It is checked
when bytes are read as the type, and safe code can change the fields of a value afterwards. So a
pun between two types that have the same rule over the same bytes is proven: the rule of the
target holds exactly when the rule of the source does, and checking it again would add nothing.
[Guarantees and requirements] explains the difference.

## Reading two versions of a file

A pun can also read an old version of a format as the new one, when every old value is a valid
new value. In this example, version 2 of the save file of a game adds a new kind of item, the key.
`load` reads a file of either version as the same type, without a copy.

The file starts with a version byte. A *view enum*, whose variants each hold a reference, selects
the version: the derive gives it a `from_bytes` that returns the first variant that the bytes
match. A view enum is not stored in the bytes, so it needs no `repr`.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// Version 1. Each item starts with a 1-byte tag, and is 3 bytes long.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 keeps the tags and the fields of version 1, and adds a key of 3 bytes too.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// `V` is the type of the version byte, and `B` the type of the items.
#[derive(Pun)]
#[repr(C)]
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // always "SAVE", as a big-endian `u32`
  version: V,
  bag: B,
}

// A `u8c<N>` is a `u8` whose only valid value is `N`.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    // Proven: every `OldItem` is a valid `Item`, and every `u8c<1>` a valid `u8`.
    SaveFile::V1(old) => puns::pun_ref(old),
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
valid key. Keep the tags of the variants that exist.

## Layouts that Rust leaves to the compiler

Give your types a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another
integer for an enum. The default representation, `repr(Rust)`, guarantees little about the
layout, so that the compiler can optimize it. The order of the fields, the tag of an enum and its
niches may change with the next version of the compiler, or at random with
[`-Zrandomize-layout`][randomize].

By default, `puns` accepts a pun of such a type only if every layout that Rust allows gives it the
same outcome, the same checks and the same accepted values. Reading `[u8; 2]` as `(bool, u8)` is
refused, for example, since either byte may be the `bool`. With the [`unstable` feature][unstable],
`puns` uses the layout that the compiler chose and accepts more puns. They are sound in the build
that compiles them, but the next version of the compiler may refuse them, or check other bytes.

## Caveats

- Deriving `Pun` lets any code read and write every field of the type through bytes, as if the
  fields were public. Do not derive it on a type that keeps a private invariant. See
  [What deriving `Pun` means][deriving].
- A generic function cannot require in its bounds that two types pun, since the analysis is a
  constant, not a trait. When it is used with two types that do not pun, the error points at the
  pun inside the generic function.
- The analysis takes build time. On an Apple M4 Max, each pair of the five-field structs of
  [`docs/performance.md`][performance] took about 17 ms in a debug build and 7 ms in release. The
  build of a view enum whose variants overlap grew as the square of their number: 64 variants took
  10 s in debug, and 25 s in release.
- An [`Error`] says whether the size, the alignment or a value was wrong, but not where. A view
  enum returns `Error::Validity` when no variant matches.
- A `walk` inside a `walk` takes time in proportion to the square of the length of the data, and
  so does a checksum inside a `walk`. Each further level multiplies by the length again, up to 4
  nested walks. Limit the length of untrusted input that such a type reads.

## Performance

A proven pun compiles to no more than the computation of an address or a length. For example,
`pun::<[bool; 4], u32>` is `mov eax, edi; ret` on x86-64. A checked pun compiles to the checks
that `puns` could not prove, which never allocate or panic. On an Apple M4 Max, checking an IPv4
packet of 1472 bytes of UDP, both checksums included, takes 22 ns, the same as the check written
by hand. [`docs/performance.md`][performance] has more machine code and all the benchmarks.

## Format examples

The [`examples`][examples] directory reads parts of real formats in place. Formats with variable
parts are read the same way: options, compressed names and variable-length integers are regions
of bytes, which rules check and which the code reads further. Each example says what it leaves out.
The table goes from the shortest example to the longest. Run one with `cargo run --example media`
in a clone of the repository.

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

For shorter examples of each function, see [Using the functions][using].

## Compared with other crates

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They describe each type on its own, with marker traits such as "any bytes
are a valid value", and allow a conversion when the source and the target have the right traits.
`puns` compares the two types of each pun instead. That is how it proves `Switch` to `bool`, reads
one version of a format as another, and keeps the rules of a format in its types. They have what
`puns` does not: years of use and review, helpers to allocate, and refusals that are trait bounds,
which `cargo check` and editors report.

To decode data into owned values, or a format whose fields move with its content, use a parser
such as [`binrw`][binrw], [`deku`][deku] or [`nom`][nom]. Rust's [safe transmute] project also
compares two types, in the compiler. It requires nightly.

## Testing

`puns` uses `unsafe` code, and the analysis decides when that code runs. The tests check both:

- A model check compares the analysis with oracles written by hand, on every ordered pair of test
  types of the same size. It runs in full every night.
- The tests run under [Miri] on x86-64, on `i686` (32 bits) and on `s390x` (big-endian).
- More than 100 programs must fail to compile, each with its expected message.

[`docs/testing.md`][testing] describes the tests, and [Soundness] what `puns` assumes.

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
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
