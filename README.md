# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`. It compares the
two types at compile time, so it knows which values of the source are valid for the target:

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Every `Switch` is 0 or 1, and both are valid `bool`s: no check is needed.
let switches = [Switch::On, Switch::Off, Switch::On];
let bools: &[bool; 3] = puns::pun_ref(&switches);
assert_eq!(bools, &[true, false, true]);

// A byte is a `Switch` only if it is 0 or 1: `from_bytes` checks it.
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 0]), Ok(&[Switch::On, Switch::Off]));
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 2]), Err(puns::Error::Validity));
```

Each such conversion is a *pun*. When `puns` can prove that every value of the source is a valid
value of the target, as for `Switch` to `bool`, the pun is *proven*: `pun`, `pun_ref` and
`pun_mut` accept it, and it compiles to no check. When it cannot, `try_pun`, `from_bytes` and the
other `try_` and `from_` functions check at run time what it could not prove, and return a
`Result`. A pun that would read padding, or that can never be valid, does not compile at all.

`bools` borrows `switches`, and nothing is copied. If `Switch` gained a variant with the value 2,
the call to `pun_ref` would stop compiling.

```sh
cargo add puns
```

`puns` is `no_std`, and needs Rust 1.99 or later ([why][msrv]). It is young: the first release
was in November 2026, and the API may change before 1.0.

## Reading a packet

Many formats include length and checksum fields. You declare them on the struct as *rules*, and
the `from_` functions check them along with the fields:

```rust
use puns::{Pun, num::{u16be, u32be}};

#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Pong = 2,
}

// A header of 8 bytes, then the payload. The size of the packet is 8 bytes plus `length`, and
// `crc` is the CRC-32 of the bytes from offset 8 to the end. The last field, a slice, takes the
// rest of the packet.
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

// A ping with the payload "abc", followed by one more byte.
let bytes = [1, 64, 0, 3, 0x35, 0x24, 0x41, 0xc2, b'a', b'b', b'c', 0xff];

// `from_head` reads one packet from the start of the bytes, and returns the bytes after it.
let (packet, rest) = puns::from_head::<Packet>(&bytes).unwrap();
assert_eq!(packet.kind, Kind::Ping);
assert_eq!(packet.length.get(), 3);
assert_eq!(&packet.payload, b"abc");
assert_eq!(rest, [0xff]);

// The same packet with a wrong CRC.
assert!(puns::from_head::<Packet>(&[1, 64, 0, 3, 0, 0, 0, 0, b'a', b'b', b'c']).is_err());
```

`u16be` and `u32be` are big-endian integers with an alignment of 1, so `Packet` has no padding,
and can be read at any address. A rule is an expression over the unsigned integer fields, with
operators, constants, and the functions that the derive macro provides, like `crc32`. Its
[documentation][derive] lists them, with the other attributes.

A rule is checked when bytes are read, and nothing keeps it true afterwards. Through a
`&mut Packet`, code can set `length` to 10, and the payload still has 3 bytes.

## Reading an old version of a format

Because `puns` compares two types, it can read data in an old version of a format as the new
version, as long as every old value is still a valid new value:

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

If version 2 drops a variant, or gives a field fewer valid values, this stops compiling. But
`puns` cannot see meaning. Variants are matched by their tag, so if version 2 gave the tag 1 to
the key, this would still compile, and would read an old potion as a key. Keep the tags of
existing variants.

## When to use it

Use `puns` to read data in place, such as network packets, file headers and records, or to read a
value of one of your types as another. The checks that remain cost about what the same checks cost
by hand: on an Apple M4 Max, checking an IPv4 packet with 1472 bytes of UDP, both checksums
included, takes 22.1 ns, against 22.4 ns by hand. [`docs/performance.md`][performance] has the
other cases, slower ones included.

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They decide which bytes each type accepts without looking at the source
type, so with them, reading a `&[Switch]` as a `&[bool]` checks every byte and returns a
`Result`. With `puns` it needs no check ([code and machine code][comparison]). Prefer them when
you need an older version of Rust, or a crate with years of use and review behind it. Rust's
[Safe Transmute][safe transmute] project also compares the two types, in the compiler, and is
available on nightly only.

To decode data into owned values, to write a format, or to read a format whose structure is more
than lengths and offsets, use a declarative crate like [`binrw`][binrw] or [`deku`][deku], or a
parser like [`nom`][nom].

## Before you use it

- `puns` compares the types when the compiler generates code. `cargo build` reports a refused pun
  in the code that it builds, but `cargo check`, `cargo clippy` and editors do not, so let your CI
  build the code. [Compile-time errors] shows what such an error looks like, and how to fix it.
- Give your types a stable layout: `#[repr(C)]` for a struct, and `#[repr(u8)]` or another
  integer type for an enum. For fields, prefer the integers of [`puns::num`][num], like `u16be`:
  their byte order is fixed, and their alignment of 1 leaves no padding. Without a `repr`, the
  layout may change with the compiler, and `puns` refuses what depends on it, unless you turn on
  the [`unstable` feature][unstable].
- Deriving `Pun` lets code read every field through bytes, and write any valid value to it,
  without your constructors. Do not derive it on a type with a private invariant
  ([details][deriving]).

[Compile-time errors] also explains the errors of generic code, and [Limitations] lists what else
to know, such as the types that take long to build or to check.

## Safety and testing

`puns` uses `unsafe` code, and the analysis of the types decides when that code runs. The
[Soundness] section of the documentation says what `puns` relies on, and
[`docs/testing.md`][testing] how it is tested: a model check of the analysis against oracles
written by hand, Miri on 32-bit and big-endian targets, and more than 100 programs that must not
compile. If you find a soundness bug, please [open an issue][issues].

## Learn more

The [reference][api] documents every function. The [examples] read parts of real formats, such as
PNG, ZIP, TCP and ELF; `media.rs` is the shortest.

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
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[comparison]: https://github.com/uael/puns/blob/main/docs/performance.md#compared-with-zerocopy-and-bytemuck
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
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
