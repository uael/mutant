# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](LICENSE-MIT)

**Type punning, proven at compile time: a safe alternative to `transmute`, and to hand-written
parsing of bytes. On stable Rust, even for dynamically sized types.**

A *pun* reads the bytes of a value of one type of plain data, the *source*, as a value of another,
the *target*: structs of any `repr`, enums, tuples, arrays, slices, structs that end with a
slice, `str`. The layouts and valid values of both types are compared at compile time, and a pun
is

- **proven** when every source is a valid target: it compiles to nothing;
- **checked** when only some are: a `try_` function (or a `from_*` function, from bytes) checks
  at run time what could not be proven, and nothing else;
- **refused** when it can never be valid, or could read padding, or could write through a `&mut`
  what the source cannot hold: it does not compile.

What a field cannot say by its type (sizes, lengths, values it can take, checksums) is told to
`#[derive(Pun)]`.

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Position {
  x: u8,
  visible: bool,
}

// Proven: every `Position` is two initialized bytes, nothing is checked, nothing is compiled.
assert_eq!(puns::pun_ref::<_, [u8; 2]>(&Position { x: 7, visible: true }), &[7, 1]);

// Checked: not every pair of bytes is a `Position`, only the `bool` is looked at.
assert_eq!(puns::try_pun_ref(&[7u8, 1]), Ok(&Position { x: 7, visible: true }));
assert_eq!(
  puns::try_pun_ref::<_, Position>(&[7u8, 2]),
  Err(puns::Error::Validity)
);
```

Refusals are errors of the monomorphization of the generic functions: `cargo build` reports
them, `cargo check` does not.

## Examples

Each example in `puns/examples/` declares formats with the derive only (no `impl`, no
`unsafe`), then reads them from bytes and checks that corrupted ones are rejected. Each tells
what is specific to its formats and the option that tells it, and what is not covered and why.
They run as tests (`cargo test --test ethernet`...), or one by one
(`cargo run --example ethernet`).

- `ethernet.rs`: Ethernet II and 802.3, VLANs, ARP, IPv4, IPv6 and its extension headers, UDP,
  TCP, ICMP, MPLS, PPPoE, 802.1X, LLDP.
- `ip_services.rs`: DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP.
- `transport.rs`: TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket, SSH
  binary packets, MQTT, CoAP, SCTP.
- `routing.rs`: BGP-4, OSPFv2 (and the Fletcher checksum of its LSAs), GRE, VXLAN, Geneve, IGMP
  v2 and v3, ICMPv6 neighbor discovery.
- `link.rs`: IEEE 802.11, Bluetooth HCI and L2CAP, CAN and CAN FD (SocketCAN), Modbus RTU (and
  its CRC-16) and TCP, SLIP, PPP in HDLC-like framing.
- `images.rs`: PNG, BMP, the table directory of TrueType and OpenType fonts.
- `media.rs`: RIFF and WAVE, MP4 (ISO-BMFF) boxes.
- `archives.rs`: ZIP, tar (ustar), gzip.
- `executables.rs`: ELF (32 and 64 bits, either byte order), PE/COFF, Mach-O, Java class files,
  WebAssembly modules.
- `storage.rs`: the SQLite header, FAT boot sectors, GPT partition tables, USB descriptors.
- `encodings.rs`: ASN.1 DER (SNMP, Kerberos), Protocol Buffers, MessagePack, CBOR, and text: UTF-8
  fields, C strings, an HTTP/1.1 request line.
- `codegen.rs`: the functions whose machine code `puns/DESIGN.md` measures.

`puns/DESIGN.md` tells how it works.

## Status

Not released yet: the API may still change. It builds on stable Rust 1.100, is `no_std`, and its
tests pass under Miri. See "Not done yet" in `puns/DESIGN.md` for what is missing.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
