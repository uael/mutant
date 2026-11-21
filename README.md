# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` lets you read a value as another type, or a byte slice as a typed value, without writing
`unsafe`. Each such conversion is a *pun*. At compile time, `puns` compares the layout and the
valid values of the source type with those of the target type. If it can show that every value of
the source is a valid value of the target, the pun is *proven*, and needs no check at run time.
Otherwise, a check at run time may still make it safe.

Which you get depends on the function you call. `pun`, `pun_ref` and `pun_mut` accept only proven
puns, return the target directly, and compile to no check. `try_pun`, `from_bytes` and the other
`try_` and `from_` functions also accept puns that need a check, return a `Result`, and check only
what `puns` could not prove. A pun that no function accepts, such as one that would read padding,
does not compile.

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: every `Switch` is 0 or 1, and both are valid `bool`s.
let switches = [Switch::On, Switch::Off, Switch::On];
let bools: &[bool; 3] = puns::pun_ref(&switches);
assert_eq!(bools, &[true, false, true]);

// Checked: a byte is a `Switch` only if it is 0 or 1.
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 0]), Ok(&[Switch::On, Switch::Off]));
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 2]), Err(puns::Error::Validity));
```

`bools` borrows `switches`, and nothing is copied. If `Switch` gained a variant with the value 2,
the first pun would stop compiling, and you could use `try_pun_ref` there instead.

`puns` works with plain data: integers, floats, `bool`, `char`, arrays, slices, `str`, and the
structs, enums and unions that derive `Pun`.

```sh
cargo add puns
```

`puns` is `no_std`, and needs Rust 1.99 or later ([why][msrv]). It is young: the first release
was in November 2026, and the API may change before 1.0.

## Reading a packet

Many formats include length and checksum fields. You declare them on the struct as *rules*, and
the `from_` functions check them along with the fields. In this packet, 8 bytes of header give the
kind of the packet, its time to live, the length of its payload and the CRC-32 of the payload. The
header is big-endian, so its integers are `u16be` and `u32be`, which have an alignment of 1. The
struct has no padding, and can be read at any address.

```rust
use puns::{Pun, num::{u16be, u32be}};

#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Pong = 2,
}

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

A rule is an expression over the unsigned integer fields, with operators, constants, and the
functions that the derive provides, like `crc32`. The [documentation of the derive][derive] lists
them, and the other attributes.

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

If version 2 drops a variant, or gives a field fewer valid values, the `pun_ref` stops compiling.
What `puns` cannot see is meaning. Variants are matched by their tag, so if version 2 gave the tag
1 to the key, this would still compile, and would read an old potion as a key. Keep the tags of
existing variants.

To read either version from the same file, the derive also accepts an enum of references, a
*view enum*, whose `from_bytes` returns the first variant that the bytes match, for example by a
version byte.

## When to use it

Use `puns` to read data in place: network packets, file headers, records, or values of your own
types. It pays off when the source and the target have something in common that `puns` can prove,
as in the examples above. The checks that remain are about as fast as the same checks written by
hand: on an Apple M4 Max, checking an IPv4 packet with 1472 bytes of UDP takes 22.1 ns, against
22.4 ns by hand, and checking 64 KiB of bytes as `bool`s takes 0.63 µs, against 16.6 µs for a
loop. Some cases are slower than by hand. [`docs/performance.md`][performance] has them all.

`puns` owes a lot to [`zerocopy`][zerocopy] and [`bytemuck`][bytemuck], for ideas and for
implementation details. They decide which bytes are valid for each type on its own, so with them,
reading a `&[Switch]` as a `&[bool]` checks every byte and returns a `Result`. With `puns` it
needs no check ([code and machine code][comparison]). Prefer them when you need an older version
of Rust, or a crate that has had years of use and review. Most of their errors are trait bounds,
which `cargo check` and editors report. Rust's [Safe Transmute][safe transmute] project also
compares the two types, in the compiler, and is available on nightly only.

To decode data into owned values, to write a format, or to read a format whose structure is more
than lengths and offsets, use a declarative crate like [`binrw`][binrw] or [`deku`][deku], or a
parser like [`nom`][nom].

## Functions

| To read | Proven only | Checked |
|---|---|---|
| a value or a reference as another type | `pun`, `pun_ref`, `pun_mut` | `try_pun`, `try_pun_ref`, `try_pun_mut` |
| the first or last bytes of a value | `head`, `tail`, and their `_ref` and `_mut` forms | - |
| a byte slice, all of it | - | `from_bytes`, `from_bytes_mut` |
| the start or the end of a byte slice, with the rest | - | `from_head`, `from_tail`, and their `_mut` forms |
| a value as its bytes | `as_bytes` | - |

`pun`, `try_pun`, `head` and `tail` take values of `Copy` types, and the others take references.
Through a `&mut`, you can write any value of the target, so every value of the target must also
be a valid value of the source, and `puns` must prove it. A `&mut bool` can become a
`&mut Switch`, but not a `&mut u8`, even with `try_pun_mut`. The [reference][api] documents every
function.

## Errors

When the function you call cannot accept a pun, the build fails:

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

1 is a valid `bool`, but other `u8`s are not, and `pun` accepts only proven puns. The fix is the
one that the message suggests: `puns::try_pun::<u8, bool>(1)` returns `Ok(true)`.

The first lines say what is wrong, and where in the target when it can. The `...` stands for
about 30 lines inside `puns`, which you can skip. In a debug build, the last note points at your
call. An optimized build may leave that note out, and the error then names only the two types.
[Compile-time errors] lists the common messages and their fixes.

At run time, an [`Error`][error-type] tells whether the size, the alignment or a value was wrong,
but not which field.

## Things to know

- **`cargo check` misses most errors.** `puns` compares the types when the compiler generates
  code, so `cargo check`, `cargo clippy` and editors do not report a pun refused inside a
  function. `cargo build` does, so let your CI build the code.
- **Give your types a stable layout.** Use `#[repr(C)]` for a struct, and `#[repr(u8)]` or
  another integer type for an enum. `repr(C)` fixes the order of the fields, but an ordinary
  integer field still has the byte order of the machine, and may leave padding before it. The
  integers of [`puns::num`][num], like `u16be`, have a fixed byte order and an alignment of 1.
- **The default layout of Rust may change.** Without a `repr`, Rust may reorder the fields of a
  struct or a tuple, and choose the tags and niches of an enum, differently from one compiler to
  the next, or at random with [`-Zrandomize-layout`][randomize]. `puns` refuses a pun whose result
  depends on these choices. With the [`unstable` feature][unstable]
  (`cargo add puns --features unstable`), `puns` uses the layout that the compiler chose, and
  accepts more. Despite its name, the feature works on stable Rust, but the next compiler may
  change what those puns check, or refuse them.
- **Deriving `Pun` bypasses the privacy of fields.** Code that holds a value can read every field
  through bytes, and write any valid value to it, without your constructors. Do not derive it on a
  type with a private invariant ([details][deriving]). The types of the fields must also be at
  least as visible as the struct.
- **Generic code cannot name the requirement.** A generic function cannot require in its bounds
  that a pun is possible, since the analysis is a constant, not a trait. The error then points
  inside the generic function.
- **Some rules cost more.** A check never allocates or panics, and its time grows with the length
  of the data. A rule that nests loops over the data, with `walk`, can take time in proportion to
  a power of the length, up to the fifth. Limit the length of untrusted input for such a type.
- **The analysis costs build time.** Reading a struct of five fields from bytes added about 17 ms
  to a debug build, on average over 100 such structs. A view enum with 64 overlapping variants took
  10 s in debug and 25 s in release ([details][performance]).

## Safety and testing

`puns` uses `unsafe` code, and the analysis decides when that code runs. The [Soundness] section
of the documentation lists what `puns` relies on, including two behaviors of `rustc` that no
documentation states. [`docs/testing.md`][testing] describes the tests:

- A model check runs the analysis on every ordered pair of test types of the same size, and
  checks each verdict against oracles written by hand.
- The tests run under [Miri][miri-repo] on x86-64, on a 32-bit target and on a big-endian target.
- More than 100 programs must fail to compile, each with its expected message.

If you find a soundness bug, please [open an issue][issues].

## Learn more

- The [reference][api] documents every function, and [Limitations] lists what `puns` cannot do
  yet.
- [`docs/performance.md`][performance] has the machine code of proven puns, the benchmarks, and
  build times.
- The [examples] read parts of real formats: PNG, ZIP, TCP, ELF and more. In a clone of the
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
