# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reinterprets the bytes of a value as another type, or a byte slice as a typed value,
without `unsafe`. Through a reference, nothing is copied. Crates such as `zerocopy` and `bytemuck`
mark each type with what it accepts, such as "any bytes are a valid value". `puns` compares the
source type of each conversion with its target type instead:

- If every value of the source is a valid value of the target, at an address aligned for the
  target, the conversion is **proven**. `pun`, `pun_ref` and the other functions without a prefix
  accept it, and compile it to no check. This holds even when the target has invalid values, as
  when an enum of 0 and 1 is read as a `bool`.
- If only some values are valid, or the address may not be aligned, the conversion is
  **checked**. The `try_` and `from_` functions test at run time what `puns` could not prove, and
  the functions without a prefix refuse to compile it.
- If it is never valid, or if `puns` can neither prove nor check what it needs, such as reading
  padding, the conversion is **refused** and does not compile.

It works with plain data, such as integers, `bool`, `char`, arrays, slices, `str`, and the
structs, enums and unions that derive `Pun`.

```sh
cargo add puns
```

`puns` is `no_std` and needs Rust 1.99 or later ([why][msrv]). It is young: the first release was
in November 2026, and the API may change before 1.0.

## When to use it

Use `puns` to read data in place: network packets, file headers and records, or a value of one of
your types as another of your types. Its derive also checks what a format declares in its own
bytes, such as sizes, lengths and checksums.

If your types accept any bytes, if you need an older version of Rust, or if you want years of use
and review behind your `unsafe` code, use `zerocopy` or `bytemuck`. `puns` owes them a lot, for
ideas and for implementation details. Most of their errors are trait bounds, which `cargo check`
and editors report.

To decode data into owned values, or to write a format, use a declarative crate such as
[`binrw`][binrw] or [`deku`][deku]. For formats that are read token by token, such as text, use a
parser such as [`nom`][nom].

## Example

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is always 0 or 1, so it is always a valid `bool`.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

// `u16le` is a little-endian `u16` with an alignment of 1, so a `Lamp` is 4 bytes with no padding,
// and can be read at any address.
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

// Proven: every byte of a `Lamp` is initialized, so its bytes can always be read.
assert_eq!(puns::as_bytes(lamps), bytes);

// Checked: a `u8` is a `bool` only if it is 0 or 1.
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

`pun` refuses the same conversion, even for a value that would be valid, since `puns` decides from
the types, not from the value:

```rust,compile_fail,E0080
let flag: bool = puns::pun(1u8);
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
2 |   let flag: bool = puns::pun(1u8);
  |                    ^^^^^^^^^^^^^^
```

The `...` stands for about 30 lines inside `puns`. [Compile-time errors] explains the common
messages and how to fix them. The compiler reports these errors when it generates code, which
`cargo check` does not do: run `cargo build` to see them.

## Reading a format

A format often stores its own sizes, lengths and checksums. The derive declares them as *rules*:
Rust expressions over the unsigned integer fields, with functions such as `le`, `be` and `crc32`,
which read the bytes at offsets from the start of the value. The `try_` and `from_` functions
check the rules.

```rust
use puns::{Pun, num::u32le};

// A record: the length of the text, a CRC-32 of the text, then the text in UTF-8. The text
// starts at offset 5, after 1 byte of length and 4 bytes of CRC.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 5 + length)]
#[pun(assert(crc32(5, 5 + length) == crc))]
struct Record {
  length: u8,
  crc: u32le,
  text: str,
}

// Reads every record, or returns the first error.
fn read(mut bytes: &[u8]) -> Result<Vec<&str>, puns::Error> {
  let mut texts = Vec::new();
  while !bytes.is_empty() {
    // One record from the front of `bytes`, and the bytes after it.
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

stream[1] ^= 1; // a wrong CRC
assert_eq!(read(&stream), Err(puns::Error::Validity));
```

The [documentation of the derive][derive] describes the other attributes: valid ranges of a
field, the length of a slice, regions of bytes whose sizes the data gives, and loops over the
data. A rule constrains the bytes that are read, not the type: code can still write a `length`
that breaks it, so a `&Record` does not promise that its rule holds. [Guarantees and requirements]
explains the difference.

## Reading an old version of a format

Because `puns` compares two types, it can read data of an old version of a format as the new
version, when every old value is still a valid new value:

```rust
use puns::{Pun, num::u16le};

// Version 1: each item is a 1-byte tag and 2 bytes of fields.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 keeps the tags and fields of version 1, and adds a key of the same size.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// A file of version 1: a sword of 300 damage, and a potion that heals 25.
let file = [0, 0x2c, 0x01, 1, 25, 0];
let old: &[OldItem] = puns::from_bytes(&file).unwrap();

// Proven: an `OldItem` is always a valid `Item`, so this costs nothing.
let items: &[Item] = puns::pun_ref(old);
assert_eq!(items[1], Item::Potion { heal: 25, cursed: false });
```

Variants are matched by their tag, not by their name. If version 2 gave the tag 1 to the key,
this would still compile, and would read an old potion as a key, since its bytes make a valid key.
Keep the tags of existing variants. To read either version from the same file, an enum of
references can choose by a version byte (a view enum, in the [documentation of the
derive][derive]).

## Caveats

- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type with a private invariant ([why][deriving]).
- Give your types a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another
  integer for an enum. The default representation, `repr(Rust)`, guarantees little about the
  layout, so that the compiler can optimize it: the order of the fields, the tag of an enum and its
  niches may change with the next version of the compiler, or at random with
  [`-Zrandomize-layout`][randomize]. By default, `puns` refuses a conversion of such a type that
  depends on these choices. The [`unstable` feature][unstable] uses the layout that the compiler
  chose and accepts more, but the next version of the compiler may refuse those conversions, or
  check other bytes.
- A generic function cannot require in its bounds that a conversion is possible, since the
  analysis is a constant, not a trait. The error points inside the generic function.
- The analysis costs build time. For the five-field structs measured in
  [`docs/performance.md`][performance], each pair of types took about 17 ms of a debug build on an
  Apple M4 Max. A view enum whose variants
  overlap builds in time that grows as the square of its number of variants: 64 of them took 10 s
  in a debug build, and 25 s in release.
- A check never allocates or panics, and its time grows with the length of the data, unless a rule
  nests a loop or a checksum inside a loop. Limit the length of untrusted input for such a type
  ([details][performance]).

## Learn more

- [The reference][api]: every function, by what it reads (a value or bytes) and which part (the
  whole, the front or the back).
- [Soundness]: what `puns` relies on, and what deriving `Pun` promises.
- [Performance][performance]: the machine code of proven conversions, benchmarks against the same
  checks written by hand, and build times.
- [Testing][testing]: a model check of the analysis against oracles written by hand, Miri on
  32-bit and big-endian targets, and more than 100 programs that must not compile.
- [Examples][examples] of real formats, from WAVE and PNG to TCP and ELF. Start with
  [`media.rs`][media], the shortest: `cargo run --example media`.

If you find a soundness bug, please [open an issue][issues].

## History

I started this crate in November 2023, under the name `mutant`. At the time, I worked on
Bluetooth, which involves a lot of packing and unpacking of protocol data. [`typic`][typic] was
also a motivation. I was amazed by how much it achieves with the type system. Compile-time
evaluation had recently gained features on stable Rust, and inline `const` was being stabilized.
I wanted to see how far it could go.

Compile-time evaluation had many limitations. I tried types instead, and found even more. I wrote
many prototypes that mixed the two approaches. Then I stopped the project for a long time.

Recent stabilizations and the progress of AI models made me want to come back to it. The name
`mutant` was taken, so the crate became `puns`. The [changelog] lists the releases since.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[msrv]: https://docs.rs/puns/latest/puns/#minimum-rust-version
[api]: https://docs.rs/puns/latest/puns/#reference
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Guarantees and requirements]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[randomize]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[issues]: https://github.com/uael/puns/issues
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
