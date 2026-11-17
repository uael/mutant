//! Routing, tunnels and group management, in safe Rust, with nothing but `#[derive(Data)]` and
//! its options: BGP-4, OSPFv2, GRE, VXLAN, Geneve, IGMP v2 and v3, and the neighbor discovery of
//! ICMPv6.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads messages in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these protocols, and what tells it:
//!
//! | Specificity                                     | Told by                                     |
//! |-------------------------------------------------|---------------------------------------------|
//! | the BGP marker: 16 bytes of ones                | `[u8c<0xff>; 16]`                           |
//! | a BGP message tells its type                    | `tag(MessageType = u8c<1>)`                 |
//! | BGP UPDATE: three parts, each sized before it   | `regions(withdrawn = .., attributes = ..)`  |
//! | BGP attributes: a length of 1 or 2 bytes        | a struct per form, an enum of references    |
//! | BGP prefixes: a length in bits                  | `size = (bits + 7) / 8 + 1`                 |
//! | the OSPF checksum skips the authentication      | `assert((sum16(0, 16) + sum16(..)) ..)`     |
//! | the Fletcher checksum of OSPF LSAs              | `assert({ let state = walk(..); .. })`      |
//! | GRE: optional fields told by flags              | `regions(key = if .. { 4 } else { 0 }, ..)` |
//! | the GRE checksum, when there is one             | `assert(.. \|\| sum16(..) == 0xffff)`       |
//! | GRE and Geneve tell what they carry             | `tag(EtherType = u16bec<0x0800>)`           |
//! | VXLAN: flags and reserved bytes of fixed values | `u8c<0x08>`, `[u8c<0>; 3]`                  |
//! | Geneve options, in 4 bytes words, that fill     | `assert(walk(8, ..) == ..)`                 |
//! | IGMP checksums over the whole message           | `assert(sum16(0, size_of_val(self)) ..)`    |
//! | IGMPv3: sources counted by the query            | `len = source_count`                        |
//! | IGMPv3 reports: records of their own sizes      | a `walk` of an offset and a count           |
//! | ICMPv6 options, in units of 8 bytes, that fill  | `assert(walk(16, ..) == ..)`                |
//!
//! What is not covered, and why:
//!
//! - BGP path attributes: the size of an attribute is in 1 or 2 bytes, as a flag tells. A `size`
//!   rule reads fields, not `be(..)`: each form is a struct, and an enum of references tells them
//!   apart. This is the current way: with `be` in `size` (planned), it is one struct. The
//!   attributes are read one by one by use code: a sequence of things of their own sizes is
//!   bytes. Their meanings (AS paths, communities, MP-BGP) are more declarations of the same
//!   kind, left out here.
//! - The ICMPv6 checksum covers the addresses of the IPv6 packet: it is checked by the packet,
//!   as in `ethernet.rs`. A message is checked here on its own.
//! - OSPF cryptographic authentication (a keyed hash), BGP and OSPF state machines: not
//!   something the bytes say.
#![deny(unsafe_code)]

use core::marker::PhantomData;

use transmutant::{
  Data, Error, Tag,
  num::{u8c, u16be, u16bec, u24be, u32be},
  try_transmute_ref, unpack,
};

// -------------------------------------------------------------------------------------------------
// BGP

/// The type of a BGP message, when it is not known.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct MessageType(#[data(valid(1..=5))] pub u8);

/// A BGP-4 message (RFC 4271): a marker, a length, a type, a body.
#[derive(Data)]
#[repr(C)]
#[data(size = length)]
pub struct Message<P: ?Sized + Tag<MessageType> = [u8]> {
  pub marker: [u8c<0xff>; 16],
  #[data(valid(19..=4096))]
  pub length: u16be,
  pub kind: P::Tag,
  pub body: P,
}

/// OPEN: a version, an AS number, a hold time, an identifier, optional parameters.
#[derive(Data)]
#[repr(C)]
#[data(tag(MessageType = u8c<1>))]
#[data(size = parameters_length + 10)]
pub struct Open {
  pub version: u8c<4>,
  pub my_as: u16be,
  /// 0, or at least 3 seconds.
  #[data(valid(0, 3..))]
  pub hold_time: u16be,
  pub identifier: [u8; 4],
  pub parameters_length: u8,
  pub parameters: [u8],
}

/// KEEPALIVE: nothing.
#[derive(Data)]
#[repr(C)]
#[data(tag(MessageType = u8c<4>))]
pub struct Keepalive;

/// UPDATE: withdrawn routes, path attributes, then reachability information. Each part is
/// sized by what precedes it, and the last one takes what remains.
#[derive(Data)]
#[repr(C)]
#[data(tag(MessageType = u8c<2>))]
#[data(assert(withdrawn_length + 4 <= size_of_val(self)
  && withdrawn_length + 4 + be(2 + withdrawn_length, 2) <= size_of_val(self)))]
pub struct Update {
  pub withdrawn_length: u16be,
  #[data(regions(
    withdrawn = withdrawn_length,
    attributes_length = 2,
    attributes = be(2 + withdrawn_length, 2),
    reachable,
  ))]
  pub data: [u8],
}

/// A path attribute whose length is one byte (the "extended length" flag, `0x10`, is not set).
///
/// The flags tell the size of the length: a struct for each ([`Attribute8`], [`Attribute16`])
/// and an enum of references ([`Attribute`]). This is the current way: with `be` in `size`
/// (planned), it is one struct.
#[derive(Data)]
#[repr(C)]
#[data(size = length + 3)]
#[data(assert(flags & 0x10 == 0 && flags & 0x0f == 0))]
pub struct Attribute8 {
  pub flags: u8,
  pub kind: u8,
  pub length: u8,
  pub value: [u8],
}

/// A path attribute whose length is two bytes.
#[derive(Data)]
#[repr(C)]
#[data(size = length + 4)]
#[data(assert(flags & 0x10 != 0 && flags & 0x0f == 0))]
pub struct Attribute16 {
  pub flags: u8,
  pub kind: u8,
  pub length: u16be,
  pub value: [u8],
}

/// A path attribute.
#[derive(Data)]
pub enum Attribute<'a> {
  Short(&'a Attribute8),
  Long(&'a Attribute16),
}

/// ORIGIN: well-known and transitive, a byte of 0 (IGP) to 2 (incomplete).
#[derive(Data)]
#[repr(C)]
pub struct Origin {
  pub flags: u8c<0x40>,
  pub kind: u8c<1>,
  pub length: u8c<1>,
  #[data(valid(0..=2))]
  pub value: u8,
}

/// An IPv4 prefix: a length in bits, then as many bytes as hold them.
#[derive(Data)]
#[repr(C)]
#[data(size = (bits + 7) / 8 + 1)]
pub struct Prefix {
  #[data(valid(..=32))]
  pub bits: u8,
  pub prefix: [u8],
}

// -------------------------------------------------------------------------------------------------
// OSPF

/// An OSPFv2 packet (RFC 2328).
#[derive(Data)]
#[repr(C)]
#[data(size = length)]
#[data(assert(length >= 24))]
// The checksum of IP over the packet, but for the 8 bytes of authentication; there is none when
// the authentication is cryptographic (type 2).
#[data(assert(auth_type == 2 || (sum16(0, 16) + sum16(24, length)) % 0xffff == 0))]
pub struct Ospf {
  pub version: u8c<2>,
  /// Hello, database description, link state request, update, acknowledgment.
  #[data(valid(1..=5))]
  pub kind: u8,
  pub length: u16be,
  pub router_id: [u8; 4],
  pub area_id: [u8; 4],
  pub checksum: u16be,
  #[data(valid(0..=2))]
  pub auth_type: u16be,
  pub authentication: [u8; 8],
  pub body: [u8],
}

/// A link state advertisement: a header of 20 bytes, then what it advertises.
///
/// Its checksum is the Fletcher checksum of ISO 8473, over all of it but its age (2 bytes):
/// over data that holds it, both of its sums are 0. There is no Fletcher checksum in the
/// expressions: a walk computes it, a byte per step, with a state of three numbers in one,
/// `c1 << 40 | c0 << 32 | offset`. This is the current way, until a function for it is built in.
#[derive(Data)]
#[repr(C)]
#[data(size = length)]
#[data(assert(length >= 20))]
#[data(assert({
  let state = walk(
    2,
    |s| s & 0xffff_ffff < size_of_val(self),
    |s| {
      let at = s & 0xffff_ffff;
      let c0 = (((s >> 32) & 0xff) + be(at, 1)) % 255;
      let c1 = ((s >> 40) + c0) % 255;
      (at + 1) | (c0 << 32) | (c1 << 40)
    },
  );
  state >> 32 == 0
}))]
pub struct Lsa {
  pub age: u16be,
  pub options: u8,
  #[data(valid(1..=11))]
  pub kind: u8,
  pub id: [u8; 4],
  pub advertising_router: [u8; 4],
  pub sequence: u32be,
  pub checksum: u16be,
  pub length: u16be,
  pub body: [u8],
}

// -------------------------------------------------------------------------------------------------
// Tunnels

/// An EtherType, as GRE and Geneve tell what they carry, when it is not known.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct EtherType(pub u16be);

/// An IPv4 packet, here only as what a tunnel carries: see `ethernet.rs` for all of it.
#[derive(Data)]
#[repr(C)]
#[data(tag(EtherType = u16bec<0x0800>))]
#[data(size = total_length)]
pub struct Ipv4 {
  #[data(valid(0x45..=0x4f))]
  pub version_ihl: u8,
  pub type_of_service: u8,
  pub total_length: u16be,
  pub rest: [u8],
}

/// An Ethernet frame, as what a tunnel carries (Transparent Ethernet Bridging).
#[derive(Data)]
#[repr(C)]
#[data(tag(EtherType = u16bec<0x6558>))]
pub struct Ethernet {
  pub destination: [u8; 6],
  pub source: [u8; 6],
  pub ether_type: u16be,
  pub payload: [u8],
}

/// GRE (RFC 2784, 2890): a checksum (flag C), a key (K) and a sequence number (S), each there
/// or not, as the flags tell; version 0. Each optional field is a region of 0 or 4 bytes.
#[derive(Data)]
#[repr(C)]
#[data(assert(flags_version & 0x4ff8 == 0 && flags_version & 7 == 0))]
#[data(assert(flags_version & 0x8000 == 0 || sum16(0, size_of_val(self)) == 0xffff))]
#[data(get(
  has_checksum: bool = flags_version & 0x8000 != 0,
  has_key: bool = flags_version & 0x2000 != 0,
  has_sequence: bool = flags_version & 0x1000 != 0,
))]
pub struct Gre<P: ?Sized + Tag<EtherType> = [u8]> {
  pub flags_version: u16be,
  pub protocol: P::Tag,
  pub payload_type: PhantomData<P>,
  #[data(regions(
    checksum = if flags_version & 0x8000 != 0 { 4 } else { 0 },
    key = if flags_version & 0x2000 != 0 { 4 } else { 0 },
    sequence = if flags_version & 0x1000 != 0 { 4 } else { 0 },
    payload: P,
  ))]
  pub data: [u8],
}

/// VXLAN (RFC 7348): the I flag, a 24 bits network identifier, an Ethernet frame.
#[derive(Data)]
#[repr(C)]
pub struct Vxlan {
  pub flags: u8c<0x08>,
  pub reserved: [u8c<0>; 3],
  pub vni: u24be,
  pub reserved_end: u8c<0>,
  pub frame: Ethernet,
}

/// Geneve (RFC 8926): options of a size in 4 bytes words, then what the protocol tells.
#[derive(Data)]
#[repr(C)]
#[data(get(options_length: usize = (version_options & 0x3f) * 4, oam: bool = flags & 0x80 != 0))]
// The options are made of whole options.
#[data(assert(walk(
  8,
  |at| at < 8 + (version_options & 0x3f) * 4,
  |at| at + 4 + (be(at + 3, 1) & 0x1f) * 4,
) == 8 + (version_options & 0x3f) * 4))]
pub struct Geneve<P: ?Sized + Tag<EtherType> = [u8]> {
  /// Version (2 bits, 0), the length of the options in 4 bytes words (6 bits).
  #[data(valid(..=0x3f))]
  pub version_options: u8,
  pub flags: u8,
  pub protocol: P::Tag,
  pub vni: u24be,
  pub reserved: u8,
  pub payload_type: PhantomData<P>,
  #[data(regions(options = (version_options & 0x3f) * 4, payload: P))]
  pub data: [u8],
}

/// A Geneve option.
#[derive(Data)]
#[repr(C)]
#[data(size = (length & 0x1f) * 4 + 4)]
pub struct GeneveOption {
  pub class: u16be,
  pub kind: u8,
  /// Reserved (3 bits), the length of the data in 4 bytes words (5 bits).
  pub length: u8,
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// IGMP

/// IGMPv2 (and the messages of v1 and v2): 8 bytes, a checksum over them.
#[derive(Data)]
#[repr(C)]
#[data(assert(sum16(0, 8) == 0xffff))]
pub struct Igmp2 {
  /// Query, v1 report, v2 report, leave.
  #[data(valid(0x11, 0x12, 0x16, 0x17))]
  pub kind: u8,
  pub max_response: u8,
  pub checksum: u16be,
  pub group: [u8; 4],
}

/// An IGMPv3 membership query: a list of sources, counted.
#[derive(Data)]
#[repr(C)]
#[data(assert(sum16(0, size_of_val(self)) == 0xffff))]
#[data(get(robustness: u8 = flags_qrv & 7, suppress: bool = flags_qrv & 8 != 0))]
pub struct Query3 {
  pub kind: u8c<0x11>,
  pub max_response: u8,
  pub checksum: u16be,
  pub group: [u8; 4],
  pub flags_qrv: u8,
  pub qqic: u8,
  pub source_count: u16be,
  #[data(len = source_count)]
  pub sources: [[u8; 4]],
}

/// An IGMPv3 membership report: group records, each of a size of its own, counted.
#[derive(Data)]
#[repr(C)]
#[data(assert(sum16(0, size_of_val(self)) == 0xffff))]
// The records fill the report: a walk whose state is the offset of a record (the low 32 bits)
// and the number of records before it (the high bits).
#[data(assert({
  let end = walk(
    8,
    |s| s >> 32 < record_count,
    |s| {
      let at = s & 0xffff_ffff;
      (at + 8 + be(at + 2, 2) * 4 + be(at + 1, 1) * 4) + (((s >> 32) + 1) << 32)
    },
  );
  end & 0xffff_ffff == size_of_val(self)
}))]
pub struct Report3 {
  pub kind: u8c<0x22>,
  pub reserved: u8,
  pub checksum: u16be,
  pub reserved_end: u16be,
  pub record_count: u16be,
  pub records: [u8],
}

/// A group record: its sources, then auxiliary data, of a size in 4 bytes words.
#[derive(Data)]
#[repr(C)]
#[data(size = 8 + source_count * 4 + aux_length * 4)]
pub struct GroupRecord {
  #[data(valid(1..=6))]
  pub kind: u8,
  pub aux_length: u8,
  pub source_count: u16be,
  pub group: [u8; 4],
  #[data(regions(sources: [[u8; 4]] = source_count * 4, aux))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// Neighbor discovery

/// An ICMPv6 router advertisement (RFC 4861): its options fill it, each of a size in units of 8
/// bytes, which is not 0.
#[derive(Data)]
#[repr(C)]
#[data(assert(walk(
  16,
  |at| at < size_of_val(self),
  |at| if be(at + 1, 1) == 0 { 1 / 0 } else { at + be(at + 1, 1) * 8 },
) == size_of_val(self)))]
#[data(get(managed: bool = flags & 0x80 != 0, other: bool = flags & 0x40 != 0))]
pub struct RouterAdvertisement {
  pub kind: u8c<134>,
  pub code: u8c<0>,
  pub checksum: u16be,
  pub hop_limit: u8,
  pub flags: u8,
  pub lifetime: u16be,
  pub reachable: u32be,
  pub retransmit: u32be,
  pub options: [u8],
}

/// An ICMPv6 neighbor solicitation: a target that is not a multicast address, then options.
#[derive(Data)]
#[repr(C)]
#[data(assert(walk(
  24,
  |at| at < size_of_val(self),
  |at| if be(at + 1, 1) == 0 { 1 / 0 } else { at + be(at + 1, 1) * 8 },
) == size_of_val(self)))]
#[data(assert(be(8, 1) != 0xff))]
pub struct NeighborSolicitation {
  pub kind: u8c<135>,
  pub code: u8c<0>,
  pub checksum: u16be,
  pub reserved: u32be,
  pub target: [u8; 16],
  pub options: [u8],
}

/// A neighbor discovery option: its size is in units of 8 bytes, and is not 0 (an option of
/// size 0 would be read forever).
#[derive(Data)]
#[repr(C)]
#[data(size = length * 8)]
pub struct NdOption {
  pub kind: u8,
  #[data(valid(1..))]
  pub length: u8,
  pub data: [u8],
}

/// The prefix information option.
#[derive(Data)]
#[repr(C)]
#[data(assert(prefix_length <= 128))]
#[data(get(on_link: bool = flags & 0x80 != 0, autonomous: bool = flags & 0x40 != 0))]
pub struct PrefixInformation {
  pub kind: u8c<3>,
  pub length: u8c<4>,
  pub prefix_length: u8,
  pub flags: u8,
  pub valid_lifetime: u32be,
  pub preferred_lifetime: u32be,
  pub reserved: u32be,
  pub prefix: [u8; 16],
}

// -------------------------------------------------------------------------------------------------
// Use

/// A BGP header, as bytes: the marker; the length and the type follow.
const MARKER: [u8; 16] = [0xff; 16];

/// A BGP OPEN, as bytes: AS 65000, a hold time of 180 seconds, no optional parameters.
const OPEN: [u8; 29] = [
  0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  0, 29, 1, 4, 0xfd, 0xe8, 0, 180, 192, 0, 2, 1, 0,
];

/// A BGP UPDATE, as bytes: no withdrawn routes; `ORIGIN` IGP, `AS_PATH` (of an extended length) of
/// AS 65000, `NEXT_HOP` 192.0.2.1; the prefix 10.0.0.0/8.
const UPDATE: [u8; 46] = [
  0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  0, 46, 2, 0, 0, 0, 21, 0x40, 1, 1, 0, 0x50, 2, 0, 6, 2, 1, 0, 0, 0xfd, 0xe8, 0x40, 3, 4, 192, 0,
  2, 1, 8, 10,
];

/// An OSPF hello packet, as bytes, without authentication.
const HELLO: [u8; 44] = [
  0x02, 0x01, 0x00, 0x2c, 0xc0, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x3a, 0x9d, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x00, 0x00, 0x0a, 0x02, 0x01,
  0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// An OSPF router LSA, as bytes: one link to a stub network, a Fletcher checksum.
const LSA: [u8; 36] = [
  0x00, 0x01, 0x22, 0x01, 0xc0, 0x00, 0x02, 0x01, 0xc0, 0x00, 0x02, 0x01, 0x80, 0x00, 0x00, 0x01,
  0x1f, 0x78, 0x00, 0x24, 0x00, 0x00, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x00,
  0x03, 0x00, 0x00, 0x0a,
];

/// A GRE packet, as bytes: a checksum, a key (42), a sequence number (7), then IPv4.
const GRE: [u8; 36] = [
  0xb0, 0x00, 0x08, 0x00, 0xae, 0xa5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x07,
  0x45, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0x11, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01,
  0x0a, 0x00, 0x00, 0x02,
];

/// A VXLAN header of network 0x1234, as bytes; an Ethernet frame follows.
const VXLAN: [u8; 8] = [0x08, 0, 0, 0, 0x00, 0x12, 0x34, 0];

/// A Geneve header, as bytes: network 5, IPv4, an option of 4 bytes of data; IPv4 follows.
const GENEVE: [u8; 16] = [0x02, 0x00, 0x08, 0x00, 0, 0, 5, 0, 0x01, 0x02, 0x80, 0x01, 1, 2, 3, 4];

/// An IGMPv2 membership report, as bytes.
const IGMP2: [u8; 8] = [0x16, 0x00, 0x09, 0x04, 0xe0, 0x00, 0x00, 0xfb];

/// An IGMPv3 general query, as bytes: two sources.
const QUERY3: [u8; 20] = [
  0x11, 0x64, 0xd8, 0x19, 0x00, 0x00, 0x00, 0x00, 0x02, 0x7d, 0x00, 0x02, 0x0a, 0x00, 0x00, 0x01,
  0x0a, 0x00, 0x00, 0x02,
];

/// An IGMPv3 membership report, as bytes: a record of one source, a record of a word of
/// auxiliary data.
const REPORT3: [u8; 32] = [
  0x22, 0x00, 0xdb, 0xe0, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x00, 0x01, 0xef, 0x01, 0x01, 0x01,
  0x0a, 0x00, 0x00, 0x01, 0x04, 0x01, 0x00, 0x00, 0xef, 0x02, 0x02, 0x02, 0x09, 0x09, 0x09, 0x09,
];

/// An ICMPv6 router advertisement, as bytes: managed, a lifetime of 1800 seconds; a source
/// link-layer address, a prefix of 64 bits.
const ADVERTISEMENT: [u8; 56] = [
  134, 0, 0, 0, 64, 0x80, 0x07, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0x02, 0, 0, 0, 0, 0x0a, 3, 4,
  64, 0xc0, 0, 0, 0x0e, 0x10, 0, 0, 0x07, 0x08, 0, 0, 0, 0, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0,
  0, 0, 0, 0, 0, 0, 0,
];

pub fn main() {
  // ---- BGP: messages of typed bodies.
  let (open, rest) = unpack::<Message<Open>>(&OPEN).unwrap();
  assert_eq!((open.body.my_as.get(), open.body.hold_time.get(), rest.len()), (65000, 180, 0));
  let keepalive = [&MARKER[..], &[0, 19, 4]].concat();
  assert!(unpack::<Message<Keepalive>>(&keepalive).is_ok());
  assert_eq!(unpack::<Message<Open>>(&keepalive).err(), Some(Error::Size));
  assert_eq!(unpack::<Message>(&keepalive).unwrap().0.kind, MessageType(4));
  // A version, a hold time of 2 seconds, parameters that are not there; a marker, a type.
  for (at, value) in [(19, 3), (23, 2), (28, 1)] {
    let mut bytes = OPEN;
    bytes[at] = value;
    assert_eq!(unpack::<Message<Open>>(&bytes).err(), Some(Error::Validity));
  }
  for (at, value) in [(3, 0xfe), (18, 6)] {
    let mut bytes = OPEN;
    bytes[at] = value;
    assert_eq!(unpack::<Message>(&bytes).err(), Some(Error::Validity));
  }
  // UPDATE: three parts; attributes of their own forms.
  let (update, _) = unpack::<Message<Update>>(&UPDATE).unwrap();
  let update = &update.body;
  assert_eq!(
    (update.withdrawn().len(), update.attributes().len(), update.reachable()),
    (0, 21, &[8, 10][..])
  );
  let (mut rest, mut attributes) = (update.attributes(), vec![]);
  while let Ok((attribute, next)) = Attribute::unpack(rest) {
    attributes.push(match attribute {
      Attribute::Short(attribute) => (attribute.kind, attribute.value.len()),
      Attribute::Long(attribute) => (attribute.kind, attribute.value.len()),
    });
    rest = next;
  }
  assert_eq!((attributes, rest.len()), (vec![(1, 1), (2, 6), (3, 4)], 0));
  let origin: &Origin = try_transmute_ref(&update.attributes()[..4]).unwrap();
  assert_eq!(origin.value, 0);
  let (prefix, _) = unpack::<Prefix>(update.reachable()).unwrap();
  assert_eq!((prefix.bits, &prefix.prefix), (8, &[10][..]));
  // Attributes longer than the message; an origin of 3; a prefix of 33 bits.
  let mut bytes = UPDATE;
  bytes[22] = 30;
  assert_eq!(unpack::<Message<Update>>(&bytes).err(), Some(Error::Validity));
  assert_eq!(try_transmute_ref::<_, Origin>(&[0x40u8, 1, 1, 3][..]).err(), Some(Error::Validity));
  assert_eq!(unpack::<Prefix>(&[33, 1, 2, 3, 4, 5]).err(), Some(Error::Validity));
  // The extended length flag on ORIGIN: its length is then `[1, 0]`, beyond the bytes.
  assert!(Attribute::unpack(&[0x50, 1, 1, 0]).is_err());

  // ---- OSPF: a checksum that skips the authentication.
  let (hello, rest) = unpack::<Ospf>(&HELLO).unwrap();
  assert_eq!(
    (hello.kind, hello.router_id, hello.body.len(), rest.len()),
    (1, [192, 0, 2, 1], 20, 0)
  );
  let mut authenticated = HELLO;
  authenticated[20] = 0x55;
  assert!(unpack::<Ospf>(&authenticated).is_ok());
  // A kind, the router, the hello interval, the last byte.
  for at in [1, 5, 30, 43] {
    let mut bytes = HELLO;
    bytes[at] ^= 0x10;
    assert_eq!(unpack::<Ospf>(&bytes).err(), Some(Error::Validity));
  }
  // LSAs: the Fletcher checksum covers every byte but the age.
  let (lsa, rest) = unpack::<Lsa>(&LSA).unwrap();
  assert_eq!((lsa.kind, lsa.sequence.get(), lsa.body.len(), rest.len()), (1, 0x8000_0001, 16, 0));
  let mut aged = LSA;
  aged[1] = 200;
  assert!(try_transmute_ref::<_, Lsa>(&aged[..]).is_ok());
  for at in 2..LSA.len() {
    let mut bytes = LSA;
    bytes[at] ^= 0x01;
    assert_eq!(try_transmute_ref::<_, Lsa>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- GRE: optional fields, a checksum, a typed payload.
  let gre: &Gre<Ipv4> = try_transmute_ref(&GRE[..]).unwrap();
  assert!(gre.has_checksum() && gre.has_key() && gre.has_sequence());
  assert_eq!(
    (gre.checksum().len(), gre.key(), gre.sequence()),
    (4, &[0, 0, 0, 42][..], &[0, 0, 0, 7][..])
  );
  assert_eq!(gre.payload().unwrap().total_length.get(), 20);
  // The checksum covers what is carried.
  let mut bytes = GRE;
  bytes[30] ^= 1;
  assert_eq!(try_transmute_ref::<_, Gre>(&bytes[..]).err(), Some(Error::Validity));
  // Without a checksum, IPv6 inside: any GRE packet, not one of IPv4.
  let plain = [&[0x30, 0x00, 0x86, 0xdd][..], &GRE[8..]].concat();
  let gre: &Gre = try_transmute_ref(&plain[..]).unwrap();
  assert_eq!(
    (gre.protocol, gre.has_checksum(), gre.payload().unwrap().len()),
    (EtherType(0x86dd.into()), false, 20)
  );
  assert_eq!(try_transmute_ref::<_, Gre<Ipv4>>(&plain[..]).err(), Some(Error::Validity));
  // Version 1; a routing bit; optional fields beyond the packet.
  for (at, value) in [(1, 1), (0, 0x70)] {
    let mut bytes = plain.clone();
    bytes[at] = value;
    assert_eq!(try_transmute_ref::<_, Gre>(&bytes[..]).err(), Some(Error::Validity));
  }
  assert_eq!(try_transmute_ref::<_, Gre>(&plain[..10]).err(), Some(Error::Validity));
  // Ethernet over GRE.
  let bridged = [&[0x00u8, 0x00, 0x65, 0x58][..], &[0xff; 12], &[0x08, 0x00]].concat();
  let gre: &Gre<Ethernet> = try_transmute_ref(&bridged[..]).unwrap();
  assert_eq!(gre.payload().unwrap().ether_type.get(), 0x0800);

  // ---- VXLAN: an Ethernet frame behind fixed fields.
  let vxlan = [&VXLAN[..], &[0xff; 12], &[0x08, 0x00], &GRE[16..]].concat();
  let header: &Vxlan = try_transmute_ref(&vxlan[..]).unwrap();
  assert_eq!(
    (header.vni.get(), header.frame.ether_type.get(), header.frame.payload.len()),
    (0x1234, 0x0800, 20)
  );
  for at in [0, 2, 7] {
    let mut bytes = vxlan.clone();
    bytes[at] ^= 1;
    assert_eq!(try_transmute_ref::<_, Vxlan>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- Geneve: options of their own sizes, then IPv4.
  let geneve = [&GENEVE[..], &GRE[16..]].concat();
  let header: &Geneve<Ipv4> = try_transmute_ref(&geneve[..]).unwrap();
  assert_eq!((header.vni.get(), header.options_length(), header.oam()), (5, 8, false));
  let (option, rest) = unpack::<GeneveOption>(header.options()).unwrap();
  assert_eq!(
    (option.class.get(), option.kind, &option.data, rest.len()),
    (0x0102, 0x80, &[1, 2, 3, 4][..], 0)
  );
  assert!(header.payload().is_ok());
  // An option that runs past the options; version 1.
  for (at, value) in [(11, 2), (0, 0x42)] {
    let mut bytes = geneve.clone();
    bytes[at] = value;
    assert_eq!(try_transmute_ref::<_, Geneve>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- IGMP: checksums, a count of sources, records that fill a report.
  let report: &Igmp2 = try_transmute_ref(&IGMP2[..]).unwrap();
  assert_eq!((report.kind, report.group), (0x16, [224, 0, 0, 251]));
  let mut bytes = IGMP2;
  bytes[7] = 252;
  assert_eq!(try_transmute_ref::<_, Igmp2>(&bytes[..]).err(), Some(Error::Validity));
  let query: &Query3 = try_transmute_ref(&QUERY3[..]).unwrap();
  assert_eq!((query.robustness(), query.suppress()), (2, false));
  assert_eq!(query.sources, [[10, 0, 0, 1], [10, 0, 0, 2]]);
  let mut bytes = QUERY3;
  bytes[11] = 3;
  assert!(try_transmute_ref::<_, Query3>(&bytes[..]).is_err());
  let report: &Report3 = try_transmute_ref(&REPORT3[..]).unwrap();
  let (record, rest) = unpack::<GroupRecord>(&report.records).unwrap();
  assert_eq!(
    (record.kind, record.sources().unwrap(), record.aux().len()),
    (1, &[[10, 0, 0, 1]][..], 0)
  );
  let (record, rest) = unpack::<GroupRecord>(rest).unwrap();
  assert_eq!(
    (record.kind, record.sources().unwrap().len(), record.aux(), rest.len()),
    (4, 0, &[9; 4][..], 0)
  );
  // One record more than there are, and the checksum that agrees.
  let mut bytes = REPORT3;
  bytes[7] = 3;
  bytes[3] = 0xdf;
  assert_eq!(try_transmute_ref::<_, Report3>(&bytes[..]).err(), Some(Error::Validity));

  // ---- Neighbor discovery: options in units of 8 bytes.
  let advertisement: &RouterAdvertisement = try_transmute_ref(&ADVERTISEMENT[..]).unwrap();
  assert_eq!(
    (advertisement.managed(), advertisement.other(), advertisement.lifetime.get()),
    (true, false, 1800)
  );
  let (option, rest) = unpack::<NdOption>(&advertisement.options).unwrap();
  assert_eq!((option.kind, option.data.len()), (1, 6));
  let prefix: &PrefixInformation = try_transmute_ref(rest).unwrap();
  assert_eq!((prefix.prefix_length, prefix.on_link(), prefix.autonomous()), (64, true, true));
  assert_eq!(prefix.prefix[..4], [0x20, 0x01, 0x0d, 0xb8]);
  // An option of size 0 (which would be read forever); one longer than the message.
  for (at, value) in [(17, 0), (25, 5)] {
    let mut bytes = ADVERTISEMENT;
    bytes[at] = value;
    assert_eq!(
      try_transmute_ref::<_, RouterAdvertisement>(&bytes[..]).err(),
      Some(Error::Validity)
    );
  }
  // A solicitation, then one of a multicast target.
  let mut solicitation = [&[135u8, 0, 0, 0, 0, 0, 0, 0, 0xfe, 0x80][..], &[1; 14]].concat();
  assert!(try_transmute_ref::<_, NeighborSolicitation>(&solicitation[..]).is_ok());
  solicitation[8] = 0xff;
  assert_eq!(
    try_transmute_ref::<_, NeighborSolicitation>(&solicitation[..]).err(),
    Some(Error::Validity)
  );

  println!("ok");
}
