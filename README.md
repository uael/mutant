# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe` and without
copying. Crates such as `zerocopy` and `bytemuck` describe each type on its own. `puns` compares
the two types of each conversion instead:

- If every value of the source type is a valid value of the target type, the conversion is
  **proven**, and compiles to no check. This holds even when the target has invalid values, as
  when an enum of 0 and 1 is read as a `bool`.
- If only some values are valid, the conversion is **checked**: the `try_` and `from_` functions
  test at run time what `puns` could not prove.
- Otherwise it is **refused**, and your program does not compile.

It works with plain data, such as integers, `bool`, `char`, arrays, slices, `str`, tuples, and
the structs, enums and unions that derive `Pun`.

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

// Proven: a `Switch` is always 0 or 1, so it is always a valid `bool`.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

// `u16le` is a little-endian `u16` with an alignment of 1, so a `Lamp` is 4 bytes with no padding.
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
```

`on` and `lamps` borrow the original data. Reading a `u8` as a `bool` with `pun` instead of
`try_pun` does not compile:

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

The `...` stands for about 30 lines inside `puns`. [Compile-time errors] explains the common
messages and how to fix them.

## Reading an old version of a format

Because `puns` compares two types, it can read data in an old version of a format as the new
version, when every old value is still a valid new value. In this example, version 2 of a game's
save file adds a new kind of item, the key. `load` reads either version as the same type.

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

// Version 2 keeps the tags and fields of version 1, and adds a key of the same size.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// One struct for every version: `V` is the type of the version byte, and `B` of the items.
#[derive(Pun)]
#[repr(C)]
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // always "SAVE", as a big-endian `u32`
  version: V,
  bag: B,
}

// The derive gives this enum of references a `from_bytes` that returns the first variant that
// matches. A `u8c<N>` is a `u8` whose only valid value is `N`, so the version byte decides.
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
`load` would still compile, and would read an old potion as a key, since its bytes make a valid
key. Keep the tags of existing variants.

## When to use it

Use `puns` to read data in place: network packets, file headers, records, or your own types under
another type. Its derive also declares what a format stores about itself, such as sizes, lengths
and checksums, and the `try_` and `from_` functions check them. The [`examples`][examples]
directory reads parts of real formats this way, from WAVE and PNG to TCP and ELF.

`puns` owes a lot to `zerocopy` and `bytemuck`, for ideas and for implementation details. Prefer
them when your types accept any bytes, when you need an older version of Rust, or when you want
years of use and review behind your `unsafe` code. Most of their errors are trait bounds, which
`cargo check` and editors report.

To decode data into owned values, to write a format, or to read one whose fields are not at fixed
offsets, use a parser such as [`binrw`][binrw], [`deku`][deku] or [`nom`][nom].

## Caveats

- `cargo check` does not report a refused conversion inside a function, since the refusal happens
  when the compiler generates the code of the function. Run `cargo build`.
- Prefer a stable representation: `#[repr(C)]` for a struct, `#[repr(u8)]` or another integer for
  an enum. The default representation, `repr(Rust)`, guarantees little about the layout, so that
  the compiler can optimize it: the order of the fields, the tag of an enum and its niches may
  change with the next version of the compiler, or at random with
  [`-Zrandomize-layout`][randomize]. By default, `puns` accepts a conversion of such a type only
  when the result does not depend on these choices. The [`unstable` feature][unstable] uses the
  layout that the compiler chose and accepts more, but the next version of the compiler may refuse
  those conversions, or check other bytes.
- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type with a private invariant ([why][deriving]).
- A generic function cannot require in its bounds that a conversion is possible, since the
  analysis is a constant, not a trait. The error points inside the generic function.
- The analysis costs build time: about 17 ms per pair of types in a debug build, on an Apple M4
  Max. The build of an enum of references such as `SaveFile` grows as the square of its number of
  variants when they overlap: 64 of them took 10 s in a debug build.
- A check never allocates or panics, and its time grows with the length of the data, unless a rule
  nests a loop or a checksum inside a loop. Limit the length of untrusted input that such a type
  reads ([details][performance]).

## Learn more

- [Reference]: every function, by what it reads (a value or bytes) and which part (the whole, the
  front or the back).
- [The derive][derive]: sizes, lengths, checksums, valid ranges and tags.
- [Soundness]: what `puns` relies on, and what deriving `Pun` promises.
- [`docs/performance.md`][performance]: the machine code of proven conversions, benchmarks against
  the same checks written by hand, and build times.
- [`docs/testing.md`][testing]: a model check of the analysis against oracles written by hand, the
  tests under Miri on 32-bit and big-endian targets, and more than 100 programs that must not
  compile.

If you find a soundness bug, please open an issue.

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
[Reference]: https://docs.rs/puns/latest/puns/#reference
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
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
