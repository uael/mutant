# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](LICENSE-MIT)

**Type punning, proven at compile time: a safe alternative to `transmute`, and to hand-written
parsing of bytes.**

`puns` reads a value of one type as a value of another, in place: a struct as bytes, bytes as a
packet, a `[bool; 4]` as a `u32`. It compares the two types when the program is built (their
sizes, their alignments, their padding, the values that each byte can take), and the pun is

- **proven** when every value of the source is a valid target: it compiles to no instruction;
- **checked** when only some are: a `try_` or a `from_` function checks at run time what could
  not be proven, and nothing else;
- **refused** when it can never be valid, or would read padding, or would let safe code write an
  invalid value through a `&mut`: it does not compile.

You write no `unsafe` and implement no trait: `#[derive(Pun)]` is all. It works on stable Rust,
without `std`, for dynamically sized types too.

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Position {
  x: u8,
  visible: bool,
}

// Proven: every `Position` is two initialized bytes. No instruction.
let bytes: &[u8; 2] = puns::pun_ref(&Position { x: 7, visible: true });
assert_eq!(bytes, &[7, 1]);

// Checked: not every pair of bytes is a `Position`. Only the `bool` is looked at: one `cmp`.
assert_eq!(puns::try_pun_ref(&[7u8, 1]), Ok(&Position { x: 7, visible: true }));
assert_eq!(puns::try_pun_ref::<_, Position>(&[7u8, 2]), Err(puns::Error::Validity));
```

```rust,compile_fail
let mut visible = true;
// Refused: through a `&mut u8`, safe code could write a 2 where a `bool` is.
let byte: &mut u8 = puns::pun_mut(&mut visible);
*byte = 2;
```

```text
error[E0080]: evaluation panicked:
              cannot pun: some values of the target are not valid for the source (or are
              padding where the source is not), and could be written through the `&mut`.
...
note: the above error was encountered while instantiating `fn pun_mut::<bool, u8>`
 --> src/main.rs:4:23
  |
4 |   let byte: &mut u8 = puns::pun_mut(&mut visible);
  |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## A format is a type

What the type of a field cannot say (a size that is in a header, a checksum, a value that tells
what follows) is told to the derive. This is UDP over IPv4 over Ethernet. There is no parsing
code below, and none is hidden: the declarations are the format.

```rust
use puns::{
  Pun, Tag,
  num::{u8c, u16be, u16bec},
};

/// An Ethernet frame. A `Frame` carries bytes, whatever its EtherType; a `Frame<Ipv4>` is a
/// frame whose EtherType is the one of IPv4.
#[derive(Pun)]
#[repr(C)]
struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  destination: [u8; 6],
  source: [u8; 6],
  ether_type: P::Tag,
  payload: P,
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct EtherType(u16be);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0800>))] // the EtherType it comes with
#[pun(size = total_length)] // its size is in its header
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))] // the checksum of the header
#[pun(get(header_length: usize = (version_ihl & 0x0f) * 4))] // a bit field, as a method
struct Ipv4<P: ?Sized + Tag<Protocol> = [u8]> {
  #[pun(valid(0x45..=0x4f))] // version 4, a header of 5 to 15 words
  version_ihl: u8,
  type_of_service: u8,
  total_length: u16be,
  identification: u16be,
  flags_fragment_offset: u16be,
  time_to_live: u8,
  protocol: P::Tag,
  checksum: u16be,
  source: [u8; 4],
  destination: [u8; 4],
  // The options, whose size is in the header too, then what the packet carries.
  #[pun(regions(options = (version_ihl & 0x0f) * 4 - 20, payload: P))]
  data: [u8],
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct Protocol(u8);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<17>))]
#[pun(size = length)]
struct Udp {
  source_port: u16be,
  destination_port: u16be,
  length: u16be,
  checksum: u16be,
  payload: [u8],
}

/// A frame is one thing or another: the first of these that the bytes are.
#[derive(Pun)]
enum Packet<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Ipv4(&'a Frame<Ipv4>),
  Other(&'a Frame),
}

let bytes: [u8; 47] = [
  2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x08, 0x00, // Ethernet: to, from, the EtherType of IPv4
  0x45, 0, 0, 33, 0, 1, 0x40, 0, 64, 17, 0x26, 0xc9, 10, 0, 0, 1, 10, 0, 0, 2, // IPv4
  0x30, 0x39, 0, 53, 0, 13, 0, 0, b'h', b'e', b'l', b'l', b'o', // UDP, to port 53
];

// One call checks the EtherType, the version, the sizes, the checksum of the header and the
// protocol. The frame is read where it is: nothing is copied, nothing is allocated.
let frame: &Frame<Ipv4<Udp>> = puns::from_bytes(&bytes).unwrap();
let ip = &frame.payload;
assert_eq!((ip.header_length(), ip.time_to_live), (20, 64));
assert!(ip.options().is_empty());
let udp = ip.payload().unwrap();
assert_eq!((udp.destination_port.get(), &udp.payload), (53, &b"hello"[..]));

// What is known is not checked again. A frame that carries UDP is a frame: no instruction.
let any: &Frame = puns::pun_ref(frame);
assert_eq!(any.payload.len(), 33);

// One bit of the header is flipped: its checksum does not hold, this is not IPv4.
let mut corrupted = bytes;
corrupted[22] ^= 1;
assert_eq!(puns::from_bytes::<Frame<Ipv4>>(&corrupted).err(), Some(puns::Error::Validity));
assert!(matches!(Packet::from_bytes(&corrupted), Ok(Packet::Other(_))));
assert!(matches!(Packet::from_bytes(&bytes), Ok(Packet::Udp(_))));

// From a stream: the header tells where the packet ends, `from_head` returns what follows.
let stream = [&bytes[14..], &[0xaa, 0xbb]].concat();
let (packet, rest) = puns::from_head::<Ipv4>(&stream).unwrap();
assert_eq!((size_of_val(packet), rest), (33, &[0xaa, 0xbb][..]));
```

| What the format says                                    | Told by                                      |
|---------------------------------------------------------|----------------------------------------------|
| numbers are big-endian                                  | `u16be`                                      |
| the EtherType tells what the frame carries              | `P::Tag`, `tag(EtherType = u16bec<0x0800>)`  |
| version 4, a header of 5 to 15 words                    | `valid(0x45..=0x4f)`                         |
| the size of a packet is in its header                   | `size = total_length`                        |
| the header has a checksum                               | `assert(sum16(..) == 0xffff)`                |
| options, whose size is in the header, then the payload  | `regions(options = .., payload: P)`          |
| bit fields                                              | `get(header_length: usize = ..)`             |
| a frame is one thing or another                         | an enum of references                        |

[`puns/examples/ethernet.rs`][ethernet] is the complete version: the checksums
of UDP and TCP, IPv6 behind any chain of extension headers, VLAN tags, the frame check sequence.

## What the derive is told

| Option                                     | What it tells                                              |
|--------------------------------------------|------------------------------------------------------------|
| `u16be`, `u32le`, `i64be`, `f32le`...      | the byte order of a field (types of `puns::num`)           |
| `u8c<17>`, `u32bec<0x89504e47>`...         | a field that has one value: a magic number, a tag          |
| `#[pun(valid(1..=9, 0x80))]`               | the values that an integer field takes                     |
| `#[pun(size = EXPR)]`                      | the size of the value, in bytes                            |
| `#[pun(len = EXPR)]`                       | the number of elements of the slice that ends it           |
| `#[pun(assert(EXPR))]`                     | anything else: bits, relations between fields, checksums   |
| `#[pun(regions(a = EXPR, b: Type))]`       | consecutive regions of the tail, and a method for each     |
| `#[pun(get(name: Type = EXPR))]`           | a method that computes from fields: a bit field            |
| `#[pun(tag(Key = Type))]`                  | the value of the `Key` that comes with this type           |
| an enum whose variants hold references     | one thing or another: `from_bytes`, `from_head`            |

An `EXPR` is a Rust expression over the unsigned integer fields of the struct and
`size_of_val(self)`: operators, `if`, `let`, constants. It can also read the value itself:

- `le(offset, n)`, `be(offset, n)`: the integer of `n` bytes at an offset;
- `sum16`, `crc32`, `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>`, `fletcher16`, `adler32`, `sum`:
  checksums over a range of bytes;
- `leb128`, `quic`, `vlq`: variable-length integers;
- `matches(DFA, start, end)`: a pattern of bytes (ASCII, digits, UTF-8, or an automaton of yours);
- `walk(start, |at| condition, |at| next)`: what is reached step by step, such as the end of a
  chain of headers or of a list of options.

Where Rust would panic (an overflow, a division by zero, a read past the end), the rule does not
hold: a rule cannot fail in any other way. The
[documentation of the derive](https://docs.rs/puns/latest/puns/derive.Pun.html) is the reference.

## The functions

What is read (a typed value, or bytes), by how much of it:

|               | Whole                                   | Front                          | Back                           |
|---------------|-----------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut`, `try_pun*` | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `as_bytes`                | `from_head`                    | `from_tail`                    |

- **A typed value** is read by value (`pun`), by reference (`pun_ref`) or by mutable reference
  (`pun_mut`). These are proven, or refused; `try_pun`, `try_pun_ref` and `try_pun_mut` are
  checked, and return a `Result`.
- **Bytes** are checked: the size, the address, then what bytes do not prove of the type.
  `from_bytes` wants all the bytes to be the value; `from_head` and `from_tail` take a value
  from the front or from the back, and return what remains. The three have a `_mut` twin, and
  `as_bytes` is the way back, proven.
- **A part** is the first or the last bytes of a value: `head_ref::<Ipv4, [u8; 20]>` is the
  fixed header of a packet.

```rust
let mut bytes = [0u8, 1, 2, 3, 4];
let (first, rest) = puns::from_head::<[u8; 2]>(&bytes).unwrap();
let (_, last) = puns::from_tail::<[u8; 2]>(rest).unwrap();
assert_eq!((first, last), (&[0, 1], &[3, 4]));

assert_eq!(puns::tail::<[u8; 5], [u8; 2]>(bytes), [3, 4]);
*puns::head_mut::<_, [u8; 2]>(&mut bytes) = [7, 7];
assert_eq!(bytes, [7, 7, 2, 3, 4]);

// By value, sizes are equal; by reference, a slice is as long as its bytes allow.
assert_eq!(puns::pun::<[bool; 4], u32>([true; 4]), 0x0101_0101);
assert_eq!(puns::pun_ref::<[u16], [[u8; 2]]>(&[1, 2, 3]).len(), 3);
assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("café"));
```

All are `const fn`s.

## What it knows

- Integers, floating-point numbers, `bool`, `char`, `NonZero`, and enums without fields, whatever
  their `repr`: each has a set of valid values, and so has each of their bytes.
- Structs of any `repr` (`C`, `transparent`, `packed`, `align`, or none), tuples, arrays, slices,
  and structs that end with a slice: their padding, and what each of their bytes can be.
- Enums with fields (`repr(C)`, `repr(u8)`...): their tag, and what each variant is made of.
- `str`: bytes that are UTF-8.
- What the derive is told, above.

An enum is known by its tag, and a variant proves the variant of the same tag:

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

// Proven: every `Shape` is a `Drawing`, variant by variant. So is a slice of them.
let shapes = [Shape::Rect(3, true), Shape::Circle { radius: 9 }];
let drawings: &[Drawing] = puns::pun_ref(&shapes[..]);
assert_eq!(drawings, [Drawing::Rect(3, 1), Drawing::Circle { radius: 9 }]);

// Checked: the tag, then the `bool` of a `Rect`.
assert_eq!(puns::try_pun(Drawing::Rect(3, 1)), Ok(Shape::Rect(3, true)));
assert_eq!(puns::try_pun::<_, Shape>(Drawing::Rect(3, 2)), Err(puns::Error::Validity));
assert_eq!(puns::try_pun::<_, Shape>(Drawing::Nothing), Err(puns::Error::Validity));
```

Two things follow from "a type is its bytes", by design. Types that differ only by a
`PhantomData` or a lifetime convert to each other freely, and deriving `Pun` makes fields as
accessible as if they were public. A type whose parameter is a brand, or whose private fields
keep an invariant that the derive is not told of, should not derive `Pun`: see
[What deriving `Pun` means](https://docs.rs/puns/latest/puns/#what-deriving-pun-means).

## What is refused

Each of these is a compile error, with a message that tells why and at which byte:

```rust,compile_fail
// Reading padding: the byte after the `u8` is not initialized.
let _: &[u8; 4] = puns::pun_ref(&(0u8, 0u16));
```

```rust,compile_fail
// A value that may not be valid, without a check: `try_pun_ref` is the function for this.
let _: &str = puns::pun_ref(&[0u8; 4]);
```

```rust,compile_fail
// A target that needs a greater alignment than the source has.
let _: &u32 = puns::pun_ref(&[0u8; 4]);
```

```rust,compile_fail
// A `&mut` through which bytes that are not UTF-8 could be written.
let mut text = String::from("text");
let _: &mut [u8] = puns::pun_mut(text.as_mut_str());
```

```rust,compile_fail
// A check that can never succeed.
let _ = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
```

A refusal is an error of the monomorphization of a generic function: **`cargo build` reports it,
`cargo check` does not.**

## Machine code

What is proven never reaches code generation, and which checks remain is decided at compile time,
for each pair of types. On x86-64, `opt-level = 3` (the functions are in
[`puns/examples/codegen.rs`][codegen], the whole table is in
[`puns/DESIGN.md`][design]):

| Function                                           | Machine code                         |
|----------------------------------------------------|--------------------------------------|
| `&Position -> &[u8; 2]`, `[bool; 4] -> u32`        | `mov rax, rdi; ret`                  |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`    |
| `&Frame<Ipv4> -> &[u8]`                            | `lea rdx, [rsi + 34]` (the length)   |
| `&[u8; 2] -> Option<&Position>`                    | one `cmp byte ptr [rdi + 1], 2`      |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool` |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 43 instructions, no loop, no call    |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, without a branch |

[`puns/benches/checks.rs`][checks] times what is checked at run time next to the
same checks written by hand.

## Next to `transmute`, `bytemuck` and `zerocopy`

`transmute` checks sizes, and the rest is yours to prove in a comment.

[`bytemuck`](https://crates.io/crates/bytemuck) and [`zerocopy`](https://crates.io/crates/zerocopy)
describe each type on its own, by marker traits (roughly: "it can be read as bytes", "any bytes
make one", "bytes can be checked to make one"), and allow a conversion when the source has one
trait and the target another. `puns` compares the two types with each other, byte by byte. What
this changes:

- **What the source proves is not checked again.** A `[bool; 2]` is a `Position`, a `Shape` is
  a `Drawing`, a frame of UDP is a frame: proven, for no instruction, where a trait of the target
  alone asks for a check of its bytes.
- **Types that have invalid values convert to each other**, by `&mut` too: `&mut [bool; 4]` as
  `&mut [[bool; 2]; 2]`, an enum as another when each accepts what the other holds.
- **The rules of a format are in its type**: sizes and lengths that are in the data, values that
  tell what follows, checksums. With a marker trait, a cast gives a header, and the code that
  finds where the packet ends is still to be written, and to be trusted.

What they have and `puns` does not: years of use and of review, an older minimum Rust version,
helpers to allocate, and refusals that are trait bounds, which `cargo check` reports and an
editor shows. The analysis of `puns` is run by the compiler when it evaluates constants, which
takes build time: a fraction of a second for each new pair of types.

## How it is verified

- **A model check.** Every verdict of the analysis is compared with oracles that do not use it:
  about a hundred types of 1 to 8 bytes, every ordered pair of the same size, every value of
  the source; every part, by writing every value through every `&mut`. It runs in full every
  night, a sample on every push.
- **Miri**, on every push, with tree borrows and with stacked borrows, on x86-64, on a 32-bit
  target (`i686`) and on a big-endian one (`s390x`).
- **Compile-fail tests**: more than eighty programs that must not build, each with its message.
- **A `core` that lies**: the derive is run in a crate whose `core` is another crate, with an
  `offset_of!` that swaps fields: what it generates does not trust a name that safe code can
  redefine.

[`puns/DESIGN.md`][design] tells how it works, what its soundness rests on, and what is
not done yet.

## Self-relative pointers

A crate that has self-relative pointers (offsets from their own address to a target in the same
buffer, as in the regions of [`nearest`](https://crates.io/crates/nearest)) declares them with
`puns::SelfRelative`. `puns::view_ref` then reads a value that holds them as another type, with
everything that they aim at proven at compile time: a tree in a buffer, read as the next
version of its type, for no instruction.

## Examples

Each file of [`puns/examples/`][examples] declares formats with the derive only (no `impl`,
no `unsafe`), reads them from bytes, and checks that corrupted ones are rejected. Each tells
what is specific to its formats and the option that tells it, and what is not covered and why.
They run as tests (`cargo test --test ethernet`), or one by one (`cargo run --example ethernet`).

| Example          | Formats                                                                        |
|------------------|--------------------------------------------------------------------------------|
| `ethernet.rs`    | Ethernet II and 802.3, VLANs, ARP, IPv4, IPv6 and its extension headers, UDP, TCP, ICMP, MPLS, PPPoE, 802.1X, LLDP |
| `ip_services.rs` | DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP                         |
| `transport.rs`   | TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket, SSH, MQTT, CoAP, SCTP |
| `routing.rs`     | BGP-4, OSPFv2 and its Fletcher checksum, GRE, VXLAN, Geneve, IGMP, ICMPv6 neighbor discovery |
| `link.rs`        | IEEE 802.11, Bluetooth HCI and L2CAP, CAN and CAN FD, Modbus RTU and TCP, SLIP, PPP |
| `images.rs`      | PNG, BMP, the table directory of TrueType and OpenType fonts                  |
| `media.rs`       | RIFF and WAVE, MP4 (ISO-BMFF) boxes                                            |
| `archives.rs`    | ZIP, tar (ustar), gzip                                                         |
| `executables.rs` | ELF (32 and 64 bits, either byte order), PE/COFF, Mach-O, Java class files, WebAssembly |
| `storage.rs`     | the SQLite header, FAT boot sectors, GPT partition tables, USB descriptors     |
| `encodings.rs`   | ASN.1 DER (SNMP, Kerberos), Protocol Buffers, MessagePack, CBOR, UTF-8 fields, C strings, an HTTP/1.1 request line |

## Getting started

```toml
[dependencies]
puns = "0.1"
```

Rust 1.100 or later. `puns` is `no_std` and depends on nothing but its derive. Build with
`cargo build` to see refusals: `cargo check` does not evaluate what decides them.

`0.1`: the API may still change before `1.0`. "Not done yet", in
[`puns/DESIGN.md`][design], lists what is missing.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[checks]: https://github.com/uael/puns/blob/main/puns/benches/checks.rs
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[design]: https://github.com/uael/puns/blob/main/puns/DESIGN.md
