# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` reads a value of one type as a value of another, and it compares the two types at compile
time to decide how. Each such *pun* is either *proven*, in which case nothing is checked,
*checked* at run time for what the types leave open, or *refused* by the compiler.

```toml
[dependencies]
puns = "0.1"
```

It needs Rust 1.99 or later, and it is `no_std`. This is version 0.1, so expect the API to change
before 1.0.

## Usage

```rust
use puns::{Pun, num::u16le};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Proven: a `Switch` is always 0 or 1, which is all a `bool` requires. Nothing is checked.
let on: &bool = puns::pun_ref(&Switch::On);
assert!(*on);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Lamp {
  switch: Switch,
  level: u8,
  watts: u16le,
}

// Checked: bytes can hold anything, so `from_bytes` checks the length and every `switch`.
let bytes = [1, 200, 60, 0, 0, 0, 40, 0];
let lamps: &[Lamp] = puns::from_bytes(&bytes).unwrap();
assert_eq!(lamps[0], Lamp { switch: Switch::On, level: 200, watts: 60.into() });
assert_eq!(puns::from_bytes::<[Lamp]>(&[2, 0, 0, 0]).err(), Some(puns::Error::Validity));

// Proven again on the way back: every `Lamp` is four initialized bytes.
assert_eq!(puns::as_bytes(lamps), bytes);
```

Neither `on` nor `lamps` is a copy. Both borrow memory that was already there. `u16le` is a
little-endian `u16` with an alignment of 1, so a `Lamp` has no padding and can sit at any address.

The first pun is the unusual one. That every `Switch` is a valid `bool` is a fact about the two
types together, not about either one, which is why `puns` compares pairs of types.

Before you derive `Pun` on your own types, give them a `repr`. Without one, Rust may reorder the
fields, and `puns` refuses any pun whose checks would depend on the order, unless you enable the
[`unstable` feature][unstable]. Keep in mind, too, that the derive lets any code read and write
every field through bytes, as if it were public. Don't put it on a type whose fields protect a
private invariant ([details][deriving]).

## Reading two versions of a file

Version 2 of a game adds keys to its items. The `load` function below reads a save file of either
version as one type, `Save`, without copying it or checking it twice.

```rust
use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

// Items as version 1 wrote them. `#[repr(u8)]` puts a one-byte tag first, so a sword is the
// byte 0 followed by its damage: three bytes, like a potion.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 adds keys. Swords and potions keep their tags and their fields, and a key is no
// bigger, so every `OldItem` is already a valid `Item`.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// One declaration for every version: `V` is the type of the version byte, and `B` the type of
// the bag. With the defaults, a `Save` is a save of any version that holds current items.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // a rule of the format, checked when bytes are read
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // a big-endian `u32` that can only be "SAVE"
  version: V,
  level: u8,
  hp: u16le,
  bag: B, // the rest of the file
}

// A view enum: `from_bytes` returns the first variant, in order, that the bytes match. A
// `u8c<N>` is a `u8` that can only be `N`, so the version byte picks the variant. A view enum
// is never stored in the bytes itself, which is why it needs no `repr`.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  Ok(match SaveFile::from_bytes(file)? {
    // Proven: an old item is an item, a `u8c<1>` is a `u8`, and the rule is the same on both
    // sides. Give `Potion` another tag in `Item`, and this line stops compiling.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

// A version 1 file: level 7, 42 hp, a sword that deals 300 and a potion that heals 25.
let mut file = b"SAVE\x01\x07\x2a\x00\x00\x2c\x01\x01\x19\x00".to_vec();
assert_eq!(load(&file).unwrap().bag[0], Item::Sword { damage: 300.into() });

// Upgrade the file to version 2, and add a key to floor 3.
file[4] = 2;
file.extend_from_slice(&[2, 3, 0]);
assert_eq!(load(&file).unwrap().bag[2], Item::Key { floor: 3, door: 0 });

// A cheater sets the hero's hp to 999. Writing a field checks nothing, but the file no longer
// satisfies the rule, so the next `load` rejects it.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```

The cheater's write goes through because `assert` is a requirement on the bytes, checked when
they are read as a `Save`. It is not an invariant that the type maintains afterwards. What a `&Save`
does guarantee is what its field types say: `magic` is "SAVE", and every item has a known tag
([guarantees and requirements][guarantees]).

The derive has more attributes of this kind, for value ranges, sizes stored in the data, checksums
and tagged payloads. The [examples](#examples) use them on real formats.

## How it works

`#[derive(Pun)]` gives each type a constant that describes its layout: its size and alignment,
where its padding is, and which values each field can hold. Every function in `puns` compares the
constants of its two types in a `const` block, which the compiler evaluates once for each pair of
types the function is used with.

`pun`, `pun_ref`, `pun_mut` and `as_bytes` accept only a proven pun. The `try_` and `from_`
functions also accept one that needs a check, and return an [`Error`] when a value fails it:

```rust
assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
```

Ask `pun` for the same thing, and the build fails:

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

The first lines say what is wrong and at which byte, and the last note points at your code. The
`...` stands for about thirty lines on constants inside `puns`, which you can ignore.
[Compile-time errors] lists every message and how to fix it. A check can't rescue every refusal,
though: a pun that would read padding is refused by the `try_` functions too.

## Caveats

- Refusals come from code generation, not from type checking, so `cargo build` reports them but
  `cargo check` and your editor don't.
- For the same reason, a generic function can't state in its bounds that its type parameters pun.
  A wrong instantiation fails in the caller's build.
- The analysis costs build time for each new pair of types. View enums are the costly case: their
  analysis grows with the square of the number of variants.
- A failed check tells you whether the size, the alignment or a value was wrong, but not where.

## Examples

The [`examples`][examples] directory declares real formats with the derive alone, without `impl`
or `unsafe`, and reads them in place. Each file covers the structures it names, not the whole
format, and says what it leaves out. They are listed from shortest to longest, so start with
`media.rs` (`cargo run --example media`, from a clone of the repository).

| Example | Formats | Shows |
|---|---|---|
| [`media.rs`][media] | WAVE, MP4 boxes | four-character codes, a pad byte, box sizes of 32 or 64 bits |
| [`images.rs`][images] | PNG, BMP, font tables | a CRC per chunk, the order of chunks, sorted records |
| [`archives.rs`][archives] | ZIP, tar, gzip | an end record found from the end of the file, octal sizes, optional fields |
| [`storage.rs`][storage] | SQLite, FAT, GPT, USB | a sector size as a const parameter, a CRC over a header |
| [`link.rs`][link] | 802.11, Bluetooth, CAN, Modbus | bit fields, CRC-16, two lengths that must agree |
| [`routing.rs`][routing] | BGP, OSPF, GRE, VXLAN, IGMP | Fletcher checksums, optional fields announced by flags |
| [`encodings.rs`][encodings] | DER, Protocol Buffers, CBOR | varints, shortest forms, C strings |
| [`ip_services.rs`][ip_services] | DNS, DHCP, NTP, STUN, RTP | DNS compression pointers, options up to an end marker |
| [`executables.rs`][executables] | ELF, PE, Mach-O, WebAssembly | byte order and word size as type parameters, LEB128 |
| [`ethernet.rs`][ethernet] | Ethernet, IPv4, IPv6, UDP, TCP | EtherType tags, IP checksums, extension headers |
| [`transport.rs`][transport] | TLS, QUIC, HTTP/2, SSH, MQTT | 24-bit lengths, variable-length integers |

For smaller snippets, such as reading from the front or the back of a buffer, see
[Using the functions][using].

## Testing

`puns` is built on `unsafe` code, and its analysis decides when that code runs, so most of the
testing is aimed at catching the analysis in a wrong answer. Every night, a model check compares
its verdicts with hand-written oracles that share no code with it, on every ordered pair of
same-sized types from a test set. The tests also run under [Miri] on x86-64, on 32-bit `i686` and
on big-endian `s390x`, and more than a hundred programs must fail to compile with their expected
message. [`docs/testing.md`][testing] has the details, and [Soundness] states what `puns` assumes.

## Performance

A proven pun compiles to no check: in a release build on x86-64, `pun::<[bool; 4], u32>` is
`mov eax, edi; ret`. A check never allocates or panics, and in the benchmarks it keeps up with the
same check written by hand. [`docs/performance.md`][performance] has the machine code and every
timing, including the cases that `puns` loses.

One warning if you take untrusted input: a `walk` rule that contains a checksum or another `walk`
takes time that grows with the square of the size of the value.

## Related crates

[`bytemuck`][bytemuck] and [`zerocopy`][zerocopy] describe each type on its own, with marker
traits. `puns` compares two types instead, which is what makes a pun like `Switch` as `bool`
possible. Reach for them when you want a crate with years of use behind it, or errors that
`cargo check` and your editor report.

I owe both a great deal. Much of what I know about layout and validity comes from their
documentation and their source, and so do many of the implementation techniques in `puns`.

Rust's [safe transmute] project also compares two types, inside the compiler, and is nightly-only
for now. If you need to build values of a different shape than the bytes, use a parser such as
[`binrw`][binrw], [`deku`][deku] or [`nom`][nom].

## History

I started this crate in November 2023, as `mutant`. I was working on Bluetooth at the time, which
means a lot of packing and unpacking of protocol structures, and I had been amazed by how much
[`typic`][typic] gets out of the type system. Compile-time evaluation was being stabilized piece
by piece, inline `const` in particular, and I wanted to see how far the same idea would go as
ordinary code run by the compiler, and how far the compiler could be pushed.

Not very far, at first. Compile-time evaluation was too limited, so in late 2024 I switched to
types, as `typic` does, and found even more limits there. Many prototypes later, mixing the two, I
shelved the project.

Two things brought me back in November 2026. Rust had stabilized what was missing, down to the
last piece in 1.99: the size and alignment of unsized types in `const`. And AI models had become
good enough that I wanted to try again. The name `mutant` was taken by then, so the crate became
`puns`.

## Documentation

- [Reference][reference]: every function, by what it reads and how much of it.
- [`#[derive(Pun)]`][derive] and its attributes, and the integer types of [`puns::num`][num].
- [Compile-time errors], [Soundness], [Cargo features][features],
  [Minimum Rust version][msrv], [Limitations].
- [`CHANGELOG.md`][changelog].

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[reference]: https://docs.rs/puns/latest/puns/#reference
[using]: https://docs.rs/puns/latest/puns/#using-the-functions
[`Error`]: https://docs.rs/puns/latest/puns/enum.Error.html
[num]: https://docs.rs/puns/latest/puns/num/index.html
[derive]: https://docs.rs/puns/latest/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
[guarantees]: https://docs.rs/puns/latest/puns/#guarantees-and-requirements
[deriving]: https://docs.rs/puns/latest/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/latest/puns/#compile-time-errors
[features]: https://docs.rs/puns/latest/puns/#cargo-features
[unstable]: https://docs.rs/puns/latest/puns/#the-unstable-feature
[msrv]: https://docs.rs/puns/latest/puns/#minimum-rust-version
[Limitations]: https://docs.rs/puns/latest/puns/#limitations
[performance]: https://github.com/uael/puns/blob/main/docs/performance.md
[testing]: https://github.com/uael/puns/blob/main/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/main/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[media]: https://github.com/uael/puns/blob/main/puns/examples/media.rs
[images]: https://github.com/uael/puns/blob/main/puns/examples/images.rs
[archives]: https://github.com/uael/puns/blob/main/puns/examples/archives.rs
[storage]: https://github.com/uael/puns/blob/main/puns/examples/storage.rs
[link]: https://github.com/uael/puns/blob/main/puns/examples/link.rs
[routing]: https://github.com/uael/puns/blob/main/puns/examples/routing.rs
[encodings]: https://github.com/uael/puns/blob/main/puns/examples/encodings.rs
[ip_services]: https://github.com/uael/puns/blob/main/puns/examples/ip_services.rs
[executables]: https://github.com/uael/puns/blob/main/puns/examples/executables.rs
[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[transport]: https://github.com/uael/puns/blob/main/puns/examples/transport.rs
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
[Miri]: https://github.com/rust-lang/miri
[bytemuck]: https://crates.io/crates/bytemuck
[zerocopy]: https://crates.io/crates/zerocopy
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
