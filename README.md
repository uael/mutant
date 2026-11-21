# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, in place and without `unsafe`.
`#[derive(Pun)]` describes the layout and the valid values of a type as a constant. When the
compiler builds a conversion, `puns` compares the constant of the source type with the one of the
target type, and the conversion is one of three kinds:

- **Proven**: `puns` can show that every value of the source is a valid value of the target. The
  conversion needs no check at run time. `pun`, `pun_ref`, `pun_mut`, and the `head` and `tail`
  functions accept only proven conversions.
- **Checked**: some values of the source may be invalid for the target, or the address may be
  misaligned. The `try_` and `from_` functions check at run time what `puns` could not prove.
- **Refused**: the conversion can never be valid, or needs something that `puns` can neither prove
  nor check, such as reading padding. It does not compile.

The comparison runs when the compiler generates code, so `cargo build` reports a refusal, but
`cargo check` and editors do not.

```rust
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is 0 or 1, and both are valid `bool`s.
let on: bool = puns::pun(Switch::On);
assert!(on);

// Checked: a `u8` is a `bool` only if it is 0 or 1. `pun::<u8, bool>` would not compile.
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
```

`puns` works with plain data: integers, floats, `bool`, `char`, `NonZero`, arrays, slices, `str`,
`MaybeUninit`, tuples, `Option`, and the structs, enums and unions that derive `Pun`. Rust leaves
part of the layout of tuples and of most `Option`s to the compiler, which limits what `puns`
accepts of them by default (see [Representation](#representation)).

```sh
cargo add puns
```

`puns` is `no_std` and needs Rust 1.99 or later ([why][msrv]). It is young. The first release was
in November 2026, and the API may change before 1.0.

## Reading a packet

Formats store their own lengths and checksums. The derive declares them as *rules*, which the
`from_` and `try_` functions check along with the fields:

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
unsigned integer fields, with operators and the functions that the derive provides, such as
`crc32`. The derive also declares the valid values of a field, the length of a slice, and getters
for flags packed into a field. Its [documentation][derive] lists every attribute and function.

A rule is checked when bytes are read. It is not an invariant of the type: code that holds a
`&mut Packet` can still change `length` and break it.

## When to use it

Use `puns` to read data in place: network packets, file headers and records, or a value of one of
your types as another. It checks the lengths and checksums that the derive declares. It proves
that an enum of an old format is valid in the new one, as the next section shows. And it lets you
write through a `&mut` only when every value that you can write is valid for both types.

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They decide what a conversion needs from each type on its own. A `bool`
does not accept every byte, so with them, a `Switch` becomes a `bool` through a checked
conversion, whose check the optimizer may or may not remove. `puns` proves it from the two types.
Prefer them when you need an older version of Rust, or years of use and review behind your
`unsafe` code. Their errors are mostly trait bounds, which `cargo check` and editors report.
Rust's [Safe Transmute][safe transmute] project also compares the two types, in the compiler, and
is available on nightly only.

To decode data into owned values, or to write a format, a declarative crate such as
[`binrw`][binrw] or [`deku`][deku] fits better. For formats read token by token, such as text, use
a parser such as [`nom`][nom].

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

Variants are matched by their tag, not by their name. If version 2 gave the tag 1 to the key,
this would still compile, and would read an old potion as a key, since its bytes make a valid key.
Keep the tags of existing variants.

To read either version from the same file, the derive also accepts an enum of references, called a
*view enum*. Its `from_bytes` returns the first variant that the bytes match, so a variant can
require a version byte of 1, and another a version byte of 2. The [documentation of the
derive][derive] has an example.

## Functions

| | Whole | Front | Back |
|---|---|---|---|
| Proven | [`pun`][pun], [`pun_ref`][pun_ref], [`pun_mut`][pun_mut] | [`head`][head], [`head_ref`][head_ref], [`head_mut`][head_mut] | [`tail`][tail], [`tail_ref`][tail_ref], [`tail_mut`][tail_mut] |
| Checked | [`try_pun`][try_pun], [`try_pun_ref`][try_pun_ref], [`try_pun_mut`][try_pun_mut] | none | none |
| Bytes, checked | [`from_bytes`][from_bytes], [`from_bytes_mut`][from_bytes_mut] | [`from_head`][from_head], [`from_head_mut`][from_head_mut] | [`from_tail`][from_tail], [`from_tail_mut`][from_tail_mut] |

The functions of the first two rows read a value as another type: all of it, or its first or last
bytes. Those of the last row read bytes as a type. A function ending in `_ref` or `_mut` takes and
returns a reference, and one without either takes a value of a `Copy` type. [`as_bytes`][as_bytes]
reads a value as its bytes, and is proven. All of them are `const fn`.

Through a `&mut`, you can write any value of the target, so every value of the target must also be
a valid value of the source, and `puns` must prove it. A `&mut bool` can become a `&mut Switch`,
but not a `&mut u8`, even with `try_pun_mut`.

## Errors

A refused conversion is a compile error:

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

The first lines say what is wrong, and at which byte of the target. The last note gives the line
of your code. The `...` stands for about 30 lines inside `puns`, which you can skip.
[Compile-time errors] explains the common messages and how to fix them.

At run time, an [`Error`][error-type] says whether the size, the alignment or a value was
wrong, not which field.

## Representation

Give your types a stable representation: `#[repr(C)]` for a struct, and `#[repr(u8)]` or another
integer for an enum. Even then, a field of an ordinary integer type has the byte order of the
machine, and may leave padding before it. The integers of [`puns::num`][num], such as `u16be`,
have a fixed byte order and an alignment of 1.

The default representation, `repr(Rust)`, guarantees little about the layout, so that the
compiler can optimize it. The order of the fields, the tag of an enum and its niches may change
with the next version of the compiler, or at random with [`-Zrandomize-layout`][randomize]. By
default, `puns` refuses a conversion of such a type when its result depends on these choices.

With the [`unstable` feature][unstable], `puns` uses the layout that the compiler chose, and
accepts more conversions. The next version of the compiler may change what they check, or make
`puns` refuse them.

## Caveats

- `cargo check` and editors do not show refused conversions. Run `cargo build`.
- Deriving `Pun` lets any code read and write every field through bytes, as if the fields were
  public. Do not derive it on a type with a private invariant ([why][deriving]).
- A generic function cannot require in its bounds that a conversion is possible, since the
  analysis is a constant, not a trait. The error then points inside the generic function.
- A check never allocates or panics. Its time grows with the length of the data, and faster when a
  rule nests loops over the data. [`docs/performance.md`][performance] gives the bounds.
- The analysis costs build time. On an Apple M4 Max, reading a struct of five fields from bytes
  added about 17 ms to a debug build, and a view enum of 64 overlapping variants took 10 s.

## Learn more

- [The reference][api] documents every function, with examples.
- [Limitations] lists what `puns` cannot prove yet.
- [Soundness] says what `puns` relies on, and what deriving `Pun` promises.
- [Performance][performance] shows the machine code of proven conversions, benchmarks against the
  same checks written by hand, and build times.
- [Testing][testing] describes a model check of the analysis against oracles written by hand, Miri
  on 32-bit and big-endian targets, and more than 100 programs that must not compile.
- The [examples] read parts of real formats, from WAVE and PNG to TCP and ELF. In a clone of the
  repository, `cargo run --example media` runs the shortest, [`media.rs`][media].

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
[error-type]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[pun]: https://docs.rs/puns/latest/puns/fn.pun.html
[pun_ref]: https://docs.rs/puns/latest/puns/fn.pun_ref.html
[pun_mut]: https://docs.rs/puns/latest/puns/fn.pun_mut.html
[head]: https://docs.rs/puns/latest/puns/fn.head.html
[head_ref]: https://docs.rs/puns/latest/puns/fn.head_ref.html
[head_mut]: https://docs.rs/puns/latest/puns/fn.head_mut.html
[tail]: https://docs.rs/puns/latest/puns/fn.tail.html
[tail_ref]: https://docs.rs/puns/latest/puns/fn.tail_ref.html
[tail_mut]: https://docs.rs/puns/latest/puns/fn.tail_mut.html
[try_pun]: https://docs.rs/puns/latest/puns/fn.try_pun.html
[try_pun_ref]: https://docs.rs/puns/latest/puns/fn.try_pun_ref.html
[try_pun_mut]: https://docs.rs/puns/latest/puns/fn.try_pun_mut.html
[from_bytes]: https://docs.rs/puns/latest/puns/fn.from_bytes.html
[from_bytes_mut]: https://docs.rs/puns/latest/puns/fn.from_bytes_mut.html
[from_head]: https://docs.rs/puns/latest/puns/fn.from_head.html
[from_head_mut]: https://docs.rs/puns/latest/puns/fn.from_head_mut.html
[from_tail]: https://docs.rs/puns/latest/puns/fn.from_tail.html
[from_tail_mut]: https://docs.rs/puns/latest/puns/fn.from_tail_mut.html
[as_bytes]: https://docs.rs/puns/latest/puns/fn.as_bytes.html
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
