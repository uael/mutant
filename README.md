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

`bools` borrows `switches`, and nothing is copied. Each such conversion is a *pun*, of one of three
kinds:

- **Proven**: `puns` can show that every value of the source is a valid value of the target, with
  a size that the target can have, at an address aligned for the target. `pun` and `pun_ref`
  accept only proven puns, and add no check at run time.
- **Checked**: some values of the source may be invalid for the target, or its length or its
  address may not suit the target. `try_pun`, `from_bytes` and the other functions whose names
  start with `try_` or `from_` check at run time what `puns` could not prove, and return a
  `Result`. Given a checked pun, `pun` and `pun_ref` do not compile, and the error suggests a
  `try_` function.
- **Refused**: the pun can never be valid, or it would read padding, or `puns` can neither prove
  nor check what it needs. No function accepts it.

If `Switch` gained a variant with the value 2, the call to `pun_ref` above would stop compiling:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

`try_pun_ref` would then check each element. `cargo build` reports such errors, but `cargo check`
and editors miss most of them.

`puns` works with plain data: integers, floats, `bool`, `char`, arrays, slices, `str`, and the
structs, enums and unions that derive `Pun`, whose fields are plain data too. `pun` and `try_pun`
take values of `Copy` types; most other functions take references. A function that returns a
`&mut` also needs every value of the target to be valid for the source, since you can write any of
them through it.

```sh
cargo add puns
```

`puns` is `no_std`, and needs Rust 1.99 or later ([why][msrv]). It is young: the first release
was in November 2026, and the API may change before 1.0.

## When to use it

Use `puns` to read data in place, such as network packets, file headers and records, or to read a
value of one of your types as another. It pays off when the format stores its own lengths and
checksums, or when the target type accepts every value of the source, as in the examples below.
It also goes the other way: `as_bytes` reads a value as its bytes, and `from_bytes_mut` gives a
`&mut` to a valid header in a buffer, to change its fields in place.

To decode data into owned values, or to transform data as you read it, consider a declarative
crate like [`binrw`][binrw] or [`deku`][deku], or a parser like [`nom`][nom].

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

`u16be` and `u32be` are big-endian integers with an alignment of 1, so `Packet` has no padding,
and can be read at any address. A rule is an expression over the unsigned integer fields, with
operators, constants, and functions that the derive macro provides, like `crc32`, and `walk` to
loop over a chain of headers. A rule cannot call your own functions on the values of fields, and
`puns` does not prove a rule from a different rule that implies it. The
[documentation of the derive][derive] lists the functions, and the other attributes, such as bit
fields and regions whose sizes are in the data.

A rule is checked by the function that reads the bytes, and nothing keeps it true afterwards.
Through a `&mut Packet`, code can set `length` to 10, and the payload still has 3 bytes.

`from_bytes`, `from_head` and `from_tail` are `const fn`, so bytes in a `const` are checked when
you build. Since `unwrap` is not `const`, [match on the result][const] there.

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
accept every byte. So with them, reading a `&[Switch]` as a `&[bool]` checks every byte and
returns a `Result`. With `puns`, it needs no check ([code and machine code][comparison]). For a
single value read by value, the optimizer removes the check of all three. Comparing the valid
values of the two types is also what proves the version example, and the rules of a format, such
as lengths and checksums, have no counterpart in their traits.

They also do what `puns` does not. `bytemuck` casts a `Vec`, `zerocopy` handles atomics, and their
errors are mostly trait bounds, which `cargo check` and editors report. Prefer them for these, for
an older version of Rust, or for a crate with years of use and review behind it. Rust's
[Safe Transmute][safe transmute] project also compares the two types, in the compiler, and is
available on nightly only.

## Performance

[`docs/performance.md`][performance] times the checks of `puns` next to the same checks written
by hand, on an Apple M4 Max. For Ethernet, IPv4, IPv6 extension headers, UDP and TCP, the median
of five runs is between 0.55 and 1.13 times the time by hand. The slowest case there, a slice of
an enum whose layout needs the `unstable` feature, takes 1.5 times as long.

Comparing the types takes build time: about 17 ms of a debug build for each struct read from
bytes, on average over 100 of them. An enum of references that returns the first variant that
the bytes match, a *view enum*, takes longer: with 64 overlapping variants, 25 s in a release
build.

## Caveats

- **Build the code to see the errors.** `puns` compares the types when the compiler generates
  code, so `cargo check`, `cargo clippy` and editors miss most refused calls, and `cargo build`
  reports them. In an optimized build, the error may not name the line of your code.
  [Compile-time errors] shows what an error looks like, and how to fix it.
- **Generic code is checked when it is used.** A generic function that calls `pun_ref` compiles
  on its own. A call of it with types that do not pun fails at `cargo build`, and the error points
  inside the generic function, since no trait bound can state that two types pun.
- **Give your types a stable layout.** Use `#[repr(C)]` for a struct, and `#[repr(u8)]` or
  another integer type for an enum, with fields like the integers of [`puns::num`][num]. Without
  a `repr`, the layout may change with the compiler, and `puns` refuses what depends on it, unless
  you turn on the [`unstable` feature][unstable] (which works on stable Rust).
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

Code that uses `puns` can keep `#![forbid(unsafe_code)]`, deriving `Pun` included. The `unsafe`
code is in `puns` and in what its derive generates, and the analysis of the types decides when it
runs. The [Soundness] section of the documentation lists
what `puns` relies on. Two of these are behaviors of `rustc` that no documentation states. One is
the representation of a pointer to an unsized type, which `puns` checks at compile time for each
type. The other is that the compiler evaluates the constants of every function that compiled code
names, even one that is never called. Only `view_ref` and `try_view_ref`, which read values that
hold pointers, rely on it, and tests check it in debug and optimized builds.

[`docs/testing.md`][testing] describes the tests. One compares the analysis with answers written
by hand, on every ordered pair of test types of the same size: for every value of the source up to
2 bytes, and for chosen values beyond. The tests of `puns`, and a selection of those pairs that
reaches every `unsafe` path, run under Miri, also on 32-bit and big-endian targets. More than 100
programs must fail to compile, each with its expected message. If you find a soundness bug, please
[open an issue][issues].

## Learn more

The [reference][api] has a table of the functions, and [Limitations] lists what `puns` cannot do
yet. The [examples] read parts of more than 40 formats, from TLS and DNS to ELF and SQLite, and
Bluetooth in [`link.rs`][link]. The shortest, [`media.rs`][media], reads WAVE files and MP4
boxes; in a clone of the repository, `cargo run --example media` runs it.

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
