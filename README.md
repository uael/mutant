# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`. It compares the
two types at compile time. A conversion that is always valid compiles to no check, one that may
fail is checked at run time, and one that is never valid does not compile:

```rust
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Always valid: a `Switch` is 0 or 1, and both are valid `bool`s. No check.
let on: bool = puns::pun(Switch::On);
assert!(on);

// May fail: a `u8` is a `bool` only if it is 0 or 1. `try_pun` checks it.
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
```

`puns::pun::<u8, bool>(1)` does not compile, even though 1 is a valid `bool`: `puns` decides from
the types, not from the values.

```sh
cargo add puns
```

`puns` is `no_std` and needs Rust 1.99 or later ([why][msrv]). It is young: the first release was
in November 2026, and the API may change before 1.0. The compiler reports a refused conversion
when it generates code, so run `cargo build`: `cargo check` does not see it.

## How it decides

Each conversion is a *pun* from a *source* type to a *target* type. `#[derive(Pun)]` records the
layout of a type and its valid values, and `puns` compares those of the source with those of the
target:

- A pun is **proven** when `puns` shows that every value of the source is a valid value of the
  target, at an address aligned for the target. Through a `&mut`, the reverse must hold too, since
  you can write any value of the target: a `&mut bool` can become a `&mut Switch`, but not a
  `&mut u8`.
- A pun is **checked** when only some values of the source are valid for the target, or when the
  address may not be aligned. The `try_` and `from_` functions check it at run time, and return an
  [`Error`] when it fails.
- Otherwise, the pun is **refused**, as is reading padding as data. It does not compile.

The functions differ by what they read, and how much of it:

| Reads | the whole | the front | the back |
|---|---|---|---|
| a typed value | `pun`, `pun_ref`, `pun_mut`, and their `try_` forms | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes | `from_bytes`, `from_bytes_mut`, and `as_bytes` the other way | `from_head`, `from_head_mut` | `from_tail`, `from_tail_mut` |

The `try_` and `from_` functions check. The others accept only proven puns, and `as_bytes` reads
a value as its bytes. They are all `const fn`. The ones that take a value, such as `pun`, need
`Copy` types. A refusal looks like this:

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
messages and how to fix them.

## Reading a packet

```rust
use puns::{Pun, num::u16be};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Pong = 2,
}

// `u16be` is a big-endian `u16` with an alignment of 1, so a `Header` has no padding, and can be
// read at any address.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Header {
  kind: Kind,
  ttl: u8,
  length: u16be,
}

let packet = [1, 64, 0, 3, b'a', b'b', b'c'];

// Checked: there must be 4 bytes, and the kind must be 1 or 2. The rest of the bytes is returned.
let (header, payload) = puns::from_head::<Header>(&packet).unwrap();
assert_eq!(header.kind, Kind::Ping);
assert_eq!(header.length.get(), 3);
assert_eq!(payload, b"abc");
assert_eq!(puns::from_head::<Header>(&[9, 64, 0, 0]).err(), Some(puns::Error::Validity));

// Proven: every byte of a `Header` is initialized, so its bytes can always be read.
assert_eq!(puns::as_bytes(header), &packet[..4]);
```

`header` and `payload` borrow `packet`: nothing is copied.

## Rules

A format often stores its own sizes, lengths and checksums. The derive declares them as *rules*,
which the `try_` and `from_` functions check. A rule is a Rust expression over the unsigned
integer fields. It can also call the functions that the derive provides: `le(offset, n)` and
`be(offset, n)` read `n` bytes at an offset from the start of the value, `crc32(start, end)`
computes the CRC-32 of the bytes between two offsets, and `walk(..)` loops over the data.

```rust
use puns::{Pun, num::u32le};

// A record: the length of the text, the CRC-32 of the text, then the text in UTF-8. The text
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

let bytes = b"\x02\xac\x2a\x93\xd8hi and more";
let (record, rest) = puns::from_head::<Record>(bytes).unwrap();
assert_eq!(&record.text, "hi");
assert_eq!(rest, b" and more");

// A wrong CRC, or text that is not UTF-8, is an error.
assert!(puns::from_head::<Record>(b"\x02\x00\x00\x00\x00hi").is_err());
```

A rule is checked when bytes are read, and is not an invariant of the type: code that holds a
`&mut Record` can write a `length` that breaks it. The [documentation of the derive][derive]
describes the other attributes: valid ranges of a field, the length of a slice, regions of bytes
whose sizes are in the data, and tags.

## Reading an old version of a format

Because `puns` compares two types, it can read data in an old version of a format as the new
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

// Proven: an `OldItem` is always a valid `Item`. No check.
let items: &[Item] = puns::pun_ref(old);
assert_eq!(items[1], Item::Potion { heal: 25, cursed: false });
```

Variants are matched by their tag, not by their name. If version 2 gave the tag 1 to the key,
this would still compile, and would read an old potion as a key, since its bytes make a valid key.
Keep the tags of existing variants. To read either version from the same file, an enum of
references can choose by a version byte: see view enums in the [documentation of the
derive][derive].

## When to use it

Use `puns` to read data in place, such as network packets, file headers and records, or to read a
value of one of your types as another. To decode data into owned values, or to write a format, a
declarative crate such as [`binrw`][binrw] or [`deku`][deku] fits better. For formats read token by
token, such as text, use a parser such as [`nom`][nom].

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They decide what each type accepts from that type alone, so reading a
`Switch` as a `bool` needs a check with them. Prefer them when you need an older version of Rust,
or years of use and review behind your `unsafe` code. Most of their errors are trait bounds, which
`cargo check` and editors report. Rust's [Safe Transmute][safe transmute] project also compares the
source and the target, in the compiler, and is available on nightly only.

## Caveats

- `cargo check` does not report a refused pun inside a function. Run `cargo build`.
- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type with a private invariant ([why][deriving]).
- Give your types a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another
  integer for an enum. `repr(C)` fixes the order of the fields, not their byte order or the
  padding between them: use the integers of [`puns::num`][num], such as `u16be`, for both.
- The default representation, `repr(Rust)`, guarantees little about the layout, so that the
  compiler can optimize it: the order of the fields, the tag of an enum and its niches may change
  with the next version of the compiler, or at random with [`-Zrandomize-layout`][randomize]. By
  default, `puns` refuses a pun of such a type that depends on these choices. With the
  [`unstable` feature][unstable], it uses the layout that the compiler chose, and accepts more
  puns, which `puns` may refuse or check differently with the next version of the compiler.
- A generic function cannot require in its bounds that a pun is possible, since the analysis is a
  constant, not a trait. The error points inside the generic function.
- An [`Error`] says whether the size, the alignment or a value was wrong, not where.
- A check never allocates or panics. Its time is in proportion to the length of the data, unless
  a rule nests a checksum or a `walk` inside a `walk`: then it grows as the square, or a higher
  power for deeper nests. Limit the length of untrusted input for such a type.
- The analysis costs build time. Each pair of the five-field structs measured in
  [`docs/performance.md`][performance] took about 17 ms of a debug build on an Apple M4 Max. A view
  enum of 64 overlapping variants took 10 s in debug, and 25 s in release.

## Learn more

- [The reference][api]: every function, with an example.
- [Limitations]: what `puns` cannot prove yet.
- [Soundness]: what `puns` relies on, and what deriving `Pun` promises.
- [Performance][performance]: the machine code of proven puns, benchmarks against the same checks
  written by hand, and build times.
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
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
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
[zerocopy]: https://crates.io/crates/zerocopy
[bytemuck]: https://crates.io/crates/bytemuck
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
