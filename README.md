# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value as another type, or bytes as a typed value, without `unsafe`. It compares the
two types at compile time, and leaves out every check that it can prove unnecessary:

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

`bools` borrows `switches`, and nothing is copied. Each such conversion is a *pun*. It is
*proven* when every value of the source is a valid value of the target, of a size the target
allows, and, for a reference, at an address aligned for it: `pun` and `pun_ref` accept only these,
and add no check at run time. It is *checked* when some of these may not hold: `try_pun`,
`from_bytes` and the other `try_` and `from_` functions check them at run time, and return a
`Result`. It is *refused* when it can never be valid, would read padding, or needs what `puns` can
neither prove nor check.

If `Switch` gained a variant with the value 2, the call to `pun_ref` above would stop compiling,
with an error that starts with:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

`cargo build` reports these errors. `cargo check` and editors miss most of them.

```sh
cargo add puns
```

`puns` is `no_std`, never allocates, and needs Rust 1.99 or later ([why][msrv]). It is young: the
first release was in November 2026, and the API may change before 1.0.

## What it reads

`puns` is for data read in place: network packets, file headers, records, or a value of one of
your types read as another. To decode data into owned values, or to transform it as you read it,
consider [`binrw`][binrw], [`deku`][deku] or [`nom`][nom] instead.

- **Plain data.** Integers, floats, `bool`, `char`, arrays, slices and `str`, and the types that
  derive `Pun`, such as `repr(C)` structs, which may end with a slice, enums with an integer
  `repr`, with or without fields, and `repr(C)` unions.
- **Fixed byte order.** [`puns::num`][num] has numbers of each byte order with an alignment of 1,
  like `u16be`, constants like `u8c<7>`, and ranges like `u8in<0, 0x7f>`, which is ASCII.
- **Rules.** The size of a value, the length of its last slice, and conditions on its fields, as
  expressions over its unsigned integer fields. They can call checksums like `crc32` and `sum16`,
  read variable-length integers, and `walk` a chain of headers.
- **Bit fields and regions.** `get` generates a getter for the bits of a field. `regions` splits
  the last field into named parts whose sizes are in the data.
- **Views.** For data that is one thing or another, an enum of references like
  `enum Frame<'a> { Ipv4(&'a Ipv4), Arp(&'a Arp), Other(&'a [u8]) }` gets a `from_bytes` that
  returns the first variant the bytes match. It checks each variant at most once.
- **Parts.** `from_head` reads the front of the bytes and returns the rest, and `from_tail` reads
  the back. `head` and `tail` do the same for typed values. Like `pun` and `try_pun`, they take
  values of `Copy` types; the other functions take references.
- **Both directions.** `as_bytes` reads a value as its bytes. `from_bytes_mut` and `pun_mut`
  return a `&mut`, which also needs every value of the target to be valid for the source, since
  you can write any of them through it.
- **Constants.** `from_bytes`, `from_head` and `from_tail` are `const fn`, so bytes in a `const` are
  checked when you build. Since `unwrap` is not `const`, [match on the result][const] there.

The [reference][api] has a table of the functions, and the [documentation of the derive][derive]
lists its attributes and the functions of rules.

## Reading a packet

Many formats store the length of their data and a checksum of it. You write these relations on
the struct as *rules*, and the `from_` functions check them along with the fields:

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

`Packet` has no padding, because `u16be` and `u32be` have an alignment of 1, so it can be read at
any address. A rule cannot call your own functions on the values of fields, and `puns` does not
prove a rule from a different rule that implies it. The error says that the bytes are invalid,
but not which field or rule.

A rule is checked by the function that reads the bytes, and nothing keeps it true afterwards.
Through a `&mut Packet`, code can set `length` to 10, and the payload still has 3 bytes.

## Reading an old version of a format

Because `puns` compares two types, it can read data in an old version of a format as the new
version, when the bytes of every old value are also a valid new value. Nothing is moved or
converted, so the two versions need the same layout for what they share:

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
a `Result`. With `puns`, it needs no check ([code and machine code][comparison]). For a single
`Switch` passed by value, the optimizer removes the check of `zerocopy` and `bytemuck`, and the
three compile to the same code. Comparing the valid values of the two types is also what proves the
version example.

Neither crate proves a rule like the CRC of `Packet` at compile time. `zerocopy` can check one at
run time through an experimental attribute, off by default, and `bytemuck` through an `unsafe`
implementation of its `CheckedBitPattern` trait.

They also do what `puns` does not. `bytemuck` casts a `Vec`, `zerocopy` handles atomics, and their
errors are mostly trait bounds, which `cargo check` and editors report. Prefer them for these, for
an older version of Rust, or for a crate with years of use and review behind it. Rust's
[Safe Transmute][safe transmute] project also compares the two types, in the compiler, and is
available on nightly only.

## Performance

[`docs/performance.md`][performance] times the checks of `puns` next to the same checks written
by hand, on an Apple M4 Max. For Ethernet, IPv4, IPv6 extension headers, UDP and TCP, `puns` takes
0.55 to 1.13 times as long as the hand-written checks, in the median of five runs. The slowest
case of the page, a slice of an enum whose layout needs the `unstable` feature, takes 1.5 times as
long.

Comparing the types takes build time: about 17 ms of a debug build for each struct read from
bytes, on average over 100 of them. A view takes longer, since its build grows as the square of
the number of its variants: with 64 variants that overlap, 25 s in a release build.

## Caveats

- **Build the code to see the errors.** `puns` compares the types when the compiler generates
  code, so `cargo check`, `cargo clippy` and editors miss most refused calls, and `cargo build`
  reports them. In an optimized build, the error may not name the line of your code.
  [Compile-time errors] shows what an error looks like, and how to fix it.
- **Generic code is checked when it is used.** A generic function that calls `pun_ref` compiles
  on its own. Calling it with two types that `pun_ref` does not accept fails at `cargo build`, and
  the error points inside the generic function, since no trait bound can state that two types
  pun.
- **Give your types a stable layout.** Use `#[repr(C)]` for a struct, and `#[repr(u8)]` or
  another integer type for an enum. Without a `repr`, the layout may change with the compiler, and
  `puns` refuses what depends on it, unless you turn on the [`unstable` feature][unstable] (which
  works on stable Rust). `repr(C)` fixes the order of the fields, not the byte order of a `u32` or
  the alignment of a `u64`, which differ between targets: for data that leaves the program, use
  the numbers of `puns::num`.
- **Deriving `Pun` bypasses your constructors.** Any code can then make a value of the type from
  bytes, and read or write its fields, private ones included. Do not derive `Pun` on a type with a
  private invariant ([details][deriving]).
- **Nested loops in rules cost more.** A rule like the CRC of `Packet` takes time in proportion
  to the length of the data. A checksum or a `walk` inside a `walk` takes time in proportion to
  the square of the length, or more. For such a type, check the length of untrusted bytes before
  you read them ([details][costs]).
- **Errors are coarse.** An [`Error`][error-type] says whether the size, the alignment or a value
  was wrong, but not which field or rule.

## Safety and testing

Code that uses `puns` can keep `#![forbid(unsafe_code)]`, deriving `Pun` included. The `unsafe` code
is in `puns` and in what its derive generates, and the analysis of the types decides when it runs.
The [Soundness] section of the documentation lists what `puns` relies on. Two of these are behaviors
of `rustc` that no documentation states. One is the representation of a pointer to an unsized type,
which `puns` checks at compile time for each type. The other is that the compiler evaluates the
constants of every function that compiled code names, even one that is never called. Only `view_ref`
and `try_view_ref`, which read values that hold pointers, rely on it, and tests check it in debug
and optimized builds.

[`docs/testing.md`][testing] describes the tests. A model check compares the analysis with oracles,
code written by hand that does not use it, on every ordered pair of test types of the same size: for
every value of the source up to 2 bytes, and for chosen values beyond. The tests of `puns` run under
Miri, also on 32-bit and big-endian targets, and so do a few of those pairs for each `unsafe` path
that they reach, on x86-64. More than 100 programs must fail to compile, each with its expected
message. If you find a soundness bug, please [open an issue][issues].

## Learn more

[Limitations] lists what `puns` cannot do yet. The [examples] read parts of more than 40 formats,
from TLS and DNS to ELF and SQLite, and Bluetooth in [`link.rs`][link]. The shortest,
[`media.rs`][media], reads WAVE files and MP4 boxes; in a clone of the repository, `cargo run
--example media` runs it.

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
