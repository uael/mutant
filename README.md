# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`, and through a
reference without copying. Crates such as `zerocopy` and `bytemuck` judge each type on its own.
`puns` compares the source type with the target type at compile time, and calls each conversion a
*pun*:

- When `puns` can prove that every value of the source is a valid value of the target, the pun is
  **proven**. It returns no `Result`, and compiles to no check.
- When only some values are valid, the pun is **checked**. A `try_` or `from_` function checks at
  run time what `puns` could not prove.
- When the pun can never be valid, or needs something that `puns` can neither prove nor check,
  such as reading padding, the build fails. So does a proven pun whose types change so that it no
  longer is.

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is 0 or 1, and both are valid `bool`s.
let switches = [Switch::On, Switch::Off, Switch::On];
let bools: &[bool] = puns::pun_ref(&switches[..]);
assert_eq!(bools, [true, false, true]);

// Checked: a byte is a `Switch` only if it is 0 or 1.
assert_eq!(puns::from_bytes::<[Switch]>(&[1, 0]), Ok(&[Switch::On, Switch::Off][..]));
assert_eq!(puns::from_bytes::<[Switch]>(&[1, 2]), Err(puns::Error::Validity));
```

`puns` works with plain data: integers, floats, `bool`, `char`, arrays, slices, `str`, and the
structs, enums and unions that derive `Pun`. It is `no_std`.

```sh
cargo add puns
```

`puns` needs Rust 1.99 or later ([why][msrv]). It is young: the first release was in November
2026, and the API may change before 1.0. `cargo build` reports a refused pun, but `cargo check`
and editors do not ([why](#errors)).

## Reading a packet

Many formats include length and checksum fields. You declare them with the derive, as *rules*,
and the functions that read bytes check the rules along with the fields:

```rust
use puns::{Pun, num::{u16be, u32be}};

// `repr(u8)` makes the tag of `Kind` one byte, with the values given.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Pong = 2,
}

// `repr(C)` keeps the fields in this order. `u16be` and `u32be` are big-endian integers with an
// alignment of 1, so `Packet` has no padding, and can be read at any address.
//
// `size` is the size of the whole packet: 8 bytes of fields, then `length` bytes of payload.
// `assert` compares the CRC-32 of the payload, from byte 8 to the end, with the field `crc`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 8 + length)]
#[pun(assert(crc32(8, 8 + length) == crc))]
struct Packet {
  kind: Kind,
  ttl: u8,
  length: u16be,
  crc: u32be,
  payload: [u8],
}

// A ping with a payload of 3 bytes, "abc", then one more byte.
let bytes = [1, 64, 0, 3, 0x35, 0x24, 0x41, 0xc2, b'a', b'b', b'c', 0xff];

// `from_head` reads one packet from the front of the bytes, and returns the bytes after it.
let (packet, rest) = puns::from_head::<Packet>(&bytes).unwrap();
assert_eq!(packet.kind, Kind::Ping);
assert_eq!(&packet.payload, b"abc");
assert_eq!(rest, [0xff]);

// Too few bytes for the length, a wrong CRC, and an unknown kind.
let error = |bytes: &[u8]| puns::from_head::<Packet>(bytes).err();
assert_eq!(error(&bytes[..10]), Some(puns::Error::Size));
assert_eq!(error(&[1, 64, 0, 0, 0, 0, 0, 1]), Some(puns::Error::Validity));
assert_eq!(error(&[9, 64, 0, 0, 0, 0, 0, 0]), Some(puns::Error::Validity));
```

`packet` and `rest` borrow `bytes`, and nothing is copied. A rule is an expression over the
unsigned integer fields, with operators, constants, and the functions that the derive provides,
like `crc32`. You can also declare the valid values of a field, the length of a slice, and getters
for flags packed into a field. The [documentation of the derive][derive] lists every attribute and
function.

A rule holds when the bytes are read, and nothing keeps it true afterwards. Through a
`&mut Packet`, code can write any `length`, and the CRC will not follow.

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

// Proven: an `OldItem` is always a valid `Item`.
let items: &[Item] = puns::pun_ref(old);
assert_eq!(items[1], Item::Potion { heal: 25, cursed: false });
```

If version 2 removes a variant, or gives a field fewer valid values, the `pun_ref` stops
compiling. Variants are matched by their tag, not by their name, though. If version 2 gave the tag
1 to the key, this would still compile, and would read an old potion as a key, since its bytes make
a valid key. Keep the tags of existing variants.

## When to use it

Use `puns` to read data in place: network packets, file headers and records, or to read a
value of one of your types as another. Choose it when the types carry what you know about the
data: lengths and checksums, enums whose values are a subset of other enums, or versions of a
format that keep their old values valid.

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They judge each type on its own, and a `bool` does not accept every byte.
So with them, reading a `&[Switch]` as a `&[bool]` is a checked conversion: it returns a `Result`,
and with zerocopy 0.8.63 and bytemuck 1.25.2 it compiles to a loop over the bytes. With `puns` it
compiles to a `ret` ([code and command][comparison]).

Prefer them when you need an older version of Rust, or years of use and review behind your
`unsafe` code. Most of their errors are trait bounds, which `cargo check` and editors report.

Rust's [Safe Transmute][safe transmute] project also compares the two types, in the compiler, and
is available on nightly only. To decode data into owned values, to write a format, or to read one
that rules cannot describe, a declarative crate like [`binrw`][binrw] or [`deku`][deku], or a
parser like [`nom`][nom], fits better.

## Functions

`pun`, `pun_ref` and `pun_mut` read a whole value as another type, and `head` and `tail` read its
first or last bytes, with `_ref` and `_mut` forms. All of them accept only proven puns.
`try_pun`, `try_pun_ref` and `try_pun_mut` also accept checked ones. `from_bytes`, `from_head` and
`from_tail` read bytes as a type, with `_mut` forms, and `as_bytes` reads a value as its bytes.
`pun`, `try_pun`, `head` and `tail` take values of `Copy` types. The [reference][api] has a table
of every function, with an example of each.

Through a `&mut`, you can write any value of the target, so every value of the target must also be
a valid value of the source, and `puns` must prove it. A `&mut bool` can become a `&mut Switch`,
but not a `&mut u8`, even with `try_pun_mut`.

## Errors

`puns` compares the types when the compiler generates code. `cargo build` reports a refused pun,
but `cargo check` and editors do not. The error looks like this:

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

The first lines say what is wrong, and at which byte of the target. Here, 1 is a valid `bool`, but
other `u8`s are not, and `pun` accepts only proven conversions, so the message suggests a `try_`
function. When the conversion can never be valid, the message says so instead. The `...` stands
for about 30 lines inside `puns`, which you can skip. In a debug build, the last note gives the
line of your code. An optimized build may leave that note out, and the error then names only the
two types. [Compile-time errors] explains the common messages and how to fix them.

At run time, an [`Error`][error-type] says whether the size, the alignment or a value was wrong,
not which field.

## Representation

Give your types a stable representation: `#[repr(C)]` for a struct, and `#[repr(u8)]` or another
integer type for an enum. Even then, a field of an ordinary integer type has the byte order of the
machine, and may leave padding before it, and a type with padding cannot be read as bytes. The
integers of [`puns::num`][num], like `u16be`, have a fixed byte order and an alignment of 1.

The default representation, `repr(Rust)`, guarantees little about the layout, so that the
compiler can optimize it. The order of the fields, the tag of an enum and its niches may change
with the next version of the compiler, or at random with [`-Zrandomize-layout`][randomize]. By
default, `puns` refuses a conversion of such a type when its result depends on these choices. This
includes tuples and most `Option`s.

With the [`unstable` feature][unstable], which builds on stable Rust, `puns` uses the layout that
the compiler chose, and accepts more conversions. The next version of the compiler may change what
they check, or make `puns` refuse them.

## Caveats

- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type with a private invariant ([why][deriving]).
- A generic function cannot require in its bounds that a conversion is possible, since the
  analysis of the types is a constant, not a trait. The error then points inside the generic
  function.
- A check never allocates or panics. Its time is in proportion to the length of the data, unless a
  rule nests loops over the data. Then it can grow up to the fifth power of the length, so limit
  the length of untrusted input for such a type ([details][performance]).
- Comparing the types costs build time. On an Apple M4 Max, reading a struct of five fields from
  bytes added about 17 ms to a debug build. Some types cost much more, as
  [`docs/performance.md`][performance] shows.

## Safety and testing

`puns` uses `unsafe` code, and the analysis of the types decides when that code runs. The
[Soundness] section of the documentation says what `puns` relies on, and
[`docs/testing.md`][testing] how it is tested:

- A model check runs the analysis on every ordered pair of test types of the same size, and
  checks each verdict against oracles written by hand.
- The tests run under [Miri][miri-repo] on x86-64, on a 32-bit target and on a big-endian target.
- More than 100 programs must fail to compile, each with its expected message.

If you find a soundness bug, please [open an issue][issues].

## Learn more

- The [reference][api] documents every function, and [Limitations] what `puns` cannot do yet.
- [`docs/performance.md`][performance] shows the machine code of proven conversions, benchmarks
  against the same checks written by hand, and build times.
- The [examples] read parts of real formats, such as PNG, ZIP, TCP and ELF. In a clone of the
  repository, `cargo run --example media` runs the shortest, [`media.rs`][media].

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
[error-type]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[randomize]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[comparison]: https://github.com/uael/puns/blob/main/docs/performance.md#compared-with-zerocopy-and-bytemuck
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[issues]: https://github.com/uael/puns/issues
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[miri-repo]: https://github.com/rust-lang/miri
[zerocopy]: https://crates.io/crates/zerocopy
[bytemuck]: https://crates.io/crates/bytemuck
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
