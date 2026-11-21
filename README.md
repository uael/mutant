# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, in place and without `unsafe`.
Each type describes its layout and its valid values, and `puns` compares the descriptions of the
two types when your code is compiled. Where the comparison proves a conversion valid, no check is
needed:

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

`bools` borrows `switches`, and nothing is copied. `pun_ref` accepts only what `puns` proves: every
`Switch` is a valid `bool`, and the array has the size and the alignment of the target.
`from_bytes` checks at run time what it cannot prove, here that each byte is 0 or 1, and returns a
`Result`. A conversion that can never be valid, or that would read padding, does not compile.

If `Switch` gained a variant with the value 2, `pun_ref` would refuse the call above. The error
starts with these lines, and `try_pun_ref`, which checks each element, is the fix:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

The comparison runs when the compiler generates code. So `cargo build` reports this error, but
`cargo check` and editors miss most errors of this kind.

```sh
cargo add puns
```

`puns` is `no_std`, never allocates, and needs Rust 1.99 or later ([why][msrv]). It is young: the
first release was in November 2026, and the API may change before 1.0.

## Reading a packet

Many formats store the length of their data and a checksum of it. You write these relations on
the struct as *rules*, and the `from_` functions check them with the fields:

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

// `from_head` returns a `&Packet` that borrows the start of the bytes, and the bytes after it.
let (packet, rest) = puns::from_head::<Packet>(&bytes).unwrap();
assert_eq!(packet.kind, Kind::Ping);
assert_eq!(packet.length.get(), 3);
assert_eq!(&packet.payload, b"abc");
assert_eq!(rest, [0xff]);

// The same packet with a wrong CRC.
let wrong = [1, 64, 0, 3, 0, 0, 0, 0, b'a', b'b', b'c'];
assert_eq!(puns::from_head::<Packet>(&wrong).err(), Some(puns::Error::Validity));
```

`u16be` and `u32be` have an alignment of 1, so `Packet` has no padding, and can be read at any
address. A rule is an expression over the unsigned integer fields and the bytes of the value.
Besides `size` and `assert`, the derive can restrict a field to some values, give the length of the
last slice, generate getters for bit fields, and split the last field into regions whose sizes are
in the data. Rules can compute other checksums, read variable-length integers, and walk a chain of
headers. The [documentation of the derive][derive] lists all of it, and the [reference][api] has a
table of the functions.

A rule is a condition of the format, not of Rust. `puns` checks it when it reads the bytes, and
nothing keeps it true afterwards. Through a `&mut Packet`, code can set `length` to 10: the
payload still has 3 bytes, and the `Packet` is still a valid Rust value.

For a format whose data can be one of several types, an enum of references gets a `from_bytes` that
returns the first variant that the bytes match. [`link.rs`][link] reads Bluetooth HCI packets this
way.

`puns` also writes in place. `as_bytes` reads a value as its bytes, and `from_bytes_mut` gives a
`&mut` to a valid header in a buffer, whose fields you can then set.

`from_bytes`, `from_head` and `from_tail` are `const fn`, so bytes in a `const` are checked when
you build. Since `unwrap` is not `const`, [match on the result][const] there.

## Reading an old version of a format

Because `puns` compares two types, it can prove that every value of an old version of a format is
a valid value of the new version. Then data already read as the old version can be read as the new
one with no check, and the build fails on the day the new version stops accepting the old one.
Nothing is moved or converted, so the two versions need the same layout for what they share:

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

// A file of version 1: a sword that deals 300 damage, and a potion that heals 25.
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

## Compared with other crates

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. Their traits say what each type accepts on its own, and a `bool` does not
accept every byte. So with them, reading a `&[Switch]` as a `&[bool]` checks every byte and returns
a `Result`. With `puns`, it needs no check ([code and machine code][comparison]). `zerocopy` can
check a rule like the CRC of `Packet` through an experimental attribute, and `bytemuck` through an
`unsafe` implementation of a trait.

They also do what `puns` does not. `bytemuck` casts a `Vec`, `zerocopy` handles atomics, and their
errors are mostly trait bounds, which `cargo check` and editors report. Prefer them for these, for
an older version of Rust, or for a crate with years of use and review behind it. To decode data
into owned values, or to transform it as you read it, consider [`binrw`][binrw], [`deku`][deku] or
[`nom`][nom]. Rust's
[Safe Transmute][safe transmute] project also compares the two types, in the compiler, and is
available on nightly only.

## Caveats

- In an optimized build, the error may not name the line of your code. A generic function that calls
  `pun_ref` compiles on its own. Used with two types that do not pun, it fails to build, and the
  error names the line of `pun_ref` in the generic function, not the caller. [Compile-time errors]
  explains the messages.
- Without a `repr`, the layout of a type may change with the compiler, and `puns` refuses what
  depends on it, unless you turn on the [`unstable` feature][unstable]. `repr(C)` fixes the order
  of the fields, not the byte order of a `u32` or the alignment of a `u64`, which differ between
  targets: for data that leaves the program, use the integers of [`puns::num`][num], like
  `u32be`.
- Deriving `Pun` bypasses your constructors. Any code can then make a value of the type from bytes,
  and read or write its fields, private ones included. Do not derive `Pun` on a type with a private
  invariant ([details][deriving]).
- A checksum inside a `walk` multiplies the time of a check by the size of the data, and each
  level of nesting multiplies it again. Limit the size of untrusted bytes for such types
  ([details][costs]).
- An [`Error`][error-type] says whether the size, the alignment or a value was wrong, but not which
  field or rule.

## Performance

In [`docs/performance.md`][performance], the checks of Ethernet, IPv4, IPv6, UDP and TCP take 0.55
to 1.13 times as long as the same checks written by hand (medians of five runs). The comparison of
the types adds to the build: in its measurements, about 17 ms to a debug build for each struct read
from bytes. A view of 64 variants that overlap took 25 s to build in release.

## Safety and testing

Code that uses `puns` needs no `unsafe`, and can keep `#![forbid(unsafe_code)]`, deriving `Pun`
included. The [Soundness] section lists what `puns` relies on. Two of these are behaviors of
`rustc` that no documentation states. One is the representation of a pointer to an unsized type,
which `puns` checks at compile time. The other is that `rustc` evaluates the constants of a
function that is named but never called. Only self-relative pointers rely on it, and a crate
declares those with an `unsafe` trait.

[`docs/testing.md`][testing] describes the tests: a model check of the analysis against oracles
written by hand, Miri on 32-bit and big-endian targets, and more than 100 programs that must fail
to compile. If you find a soundness bug, please [open an issue][issues].

## Learn more

[Limitations] lists what `puns` cannot do yet. The [examples] read parts of more than 40 formats,
from TLS and DNS to ELF and SQLite. The shortest, [`media.rs`][media], reads WAVE files and MP4
boxes, and `cargo run --example media` runs it from a clone of the repository.

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
[error-type]: https://docs.rs/puns/latest/puns/enum.Error.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[const]: https://docs.rs/puns/latest/puns/#using-the-functions
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[comparison]: https://github.com/uael/puns/blob/main/docs/performance.md#compared-with-zerocopy-and-bytemuck
[costs]: https://github.com/uael/puns/blob/main/docs/performance.md#costs
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[link]: https://github.com/uael/puns/blob/main/puns/examples/link.rs
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
