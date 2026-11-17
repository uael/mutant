//! Services over UDP and TCP, in safe Rust, with nothing but `#[derive(Pun)]` and its options:
//! DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads messages in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these formats, and what tells it:
//!
//! | Specificity                                       | Told by                                   |
//! |---------------------------------------------------|-------------------------------------------|
//! | flags of a few bits (DNS opcode, NTP mode)        | `get(..)`, `assert(..)`                   |
//! | DNS names: labels, then an empty one or a pointer | `assert({ let end = walk(..); .. })`      |
//! | the name of the first question, pointers followed | `assert(questions == 0 \|\| ..)`          |
//! | a question: a name, then 4 bytes, from its end    | `regions(name: Name = .., fixed: ..)`     |
//! | a record whose name is a pointer: a fixed head    | `valid(0xc000..)`, `size = rdlength + 12` |
//! | the DHCP magic cookie                             | `u32bec<0x6382_5363>`                     |
//! | DHCP options end with "end"; pads are a byte      | `assert(walk(240, ..) < ..)`              |
//! | the DHCP message type option is there             | another `walk`, in another `assert`       |
//! | NTP extensions: 16 bytes or more, by 4 bytes      | `size = length`, `assert(length % 4 ..)`  |
//! | RADIUS attributes fill the packet                 | `assert(walk(20, ..) == length)`          |
//! | a RADIUS user name: UTF-8, of a counted length    | `len = length - 2` on a `str`             |
//! | NetFlow records, counted by the header            | `len = count` on `[FlowRecord]`           |
//! | STUN attributes, padded to 4 bytes                | `size = 4 + (length + 3) / 4 * 4`         |
//! | the STUN fingerprint: a CRC-32 XOR a constant     | `assert(crc32(..) ^ 0x5354_554e == ..)`   |
//! | the XOR-mapped address of STUN                    | `get(port: u16 = x_port ^ 0x2112, ..)`    |
//! | RTP: CSRCs counted by 4 bits                      | `regions(csrcs: [u32be] = .., ..)`        |
//! | RTP: an extension told by a bit, of its own size  | `if first & 0x10 != 0 { .. } else { 0 }`  |
//! | RTP: padding whose size is its last byte          | `be(size_of_val(self) - 1, 1)`            |
//! | RTCP: a size in 32 bits words, minus one          | `size = (length + 1) * 4`                 |
//! | a compound RTCP packet: packets that fill it      | `assert(walk(0, ..) == ..)`               |
//! | a datagram is one message or another              | an enum of references ([`AnyMessage`])    |
//!
//! What is not covered, and why:
//!
//! - DNS: a message checks the name of its first question only. Checking every name and the
//!   fixed part of every record is one walk over the message whose step is 61 operations; an
//!   expression of a walk has at most 48. A name, and a record whose name is not a single
//!   pointer, cannot be `from_head`ed from a stream: their size is only found by walking them, and a
//!   `size` rule cannot walk. Name compression is followed, but a name is not decompressed:
//!   that would be a copy, not a view of the bytes.
//! - DHCP options, NTP extensions, RADIUS attributes, STUN attributes are read one by one by use
//!   code (`from_head` in a loop): a sequence of things of their own sizes is bytes.
//! - The RADIUS authenticators and the STUN MESSAGE-INTEGRITY are keyed hashes (MD5, HMAC-SHA1)
//!   with a secret: not something the bytes say.
#![deny(unsafe_code)]

use puns::{
  Error, Pun, from_head,
  num::{u8c, u16be, u16bec, u32be, u32bec, u64be},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// DNS

/// A DNS message: a header, then questions and records, as bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(get(
  response: bool = flags & 0x8000 != 0,
  opcode: u8 = (flags >> 11) & 0xf,
  authoritative: bool = flags & 0x0400 != 0,
  truncated: bool = flags & 0x0200 != 0,
  recursion_desired: bool = flags & 0x0100 != 0,
  rcode: u8 = flags & 0xf,
))]
// Opcodes 0 (query), 1 (inverse query), 2 (status), 4 (notify), 5 (update); the Z bit is 0.
#[pun(assert({ let opcode = (flags >> 11) & 0xf; opcode <= 2 || opcode == 4 || opcode == 5 }))]
#[pun(assert(flags & 0x0040 == 0))]
// The name of the first question, if any: labels, down to an empty label. A pointer (two bytes
// whose first two bits are set) is followed to the offset it tells, from the start of the
// message; a pointer that loops is cut by the limit of the steps of a walk. A label of the
// reserved forms (`0x40`, `0x80`) makes `1 / 0`: what cannot be computed makes the rule fail.
#[pun(assert(questions == 0 || {
  let end = walk(
    12,
    |at| be(at, 1) != 0,
    |at| if be(at, 1) >= 0xc0 {
      be(at, 2) & 0x3fff
    } else if be(at, 1) < 0x40 {
      at + 1 + be(at, 1)
    } else {
      1 / 0
    },
  );
  end < size_of_val(self)
}))]
pub struct Dns {
  pub id: u16be,
  pub flags: u16be,
  pub questions: u16be,
  pub answers: u16be,
  pub authorities: u16be,
  pub additionals: u16be,
  pub body: [u8],
}

/// A name as it is in a message: labels of 0 to 63 bytes, ending with an empty label or a
/// pointer, of 255 bytes at most.
///
/// Its size is not in it: it is only found by walking it, so a name is checked when its extent
/// is known (as in a [`Question`]), it is not `from_head`ed.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert({
  let end = walk(0, |at| be(at, 1) != 0 && be(at, 1) < 0x40, |at| at + 1 + be(at, 1));
  end < 255
    && ((be(end, 1) == 0 && end + 1 == size_of_val(self))
      || (be(end, 1) >= 0xc0 && end + 2 == size_of_val(self)))
}))]
pub struct Name(pub [u8]);

/// A question: a name, then a type and a class, which are known from the end.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 5))]
pub struct Question {
  #[pun(regions(name: Name = size_of_val(self) - 4, fixed: QuestionFixed))]
  pub data: [u8],
}

/// The type and the class of a question.
#[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
pub struct QuestionFixed {
  pub kind: u16be,
  pub class: u16be,
}

/// A resource record whose name is a single pointer, as answers usually are: its head is of a
/// fixed size, so it can be `from_head`ed.
#[derive(Pun)]
#[repr(C)]
#[pun(size = rdlength + 12)]
pub struct PointerRecord {
  #[pun(valid(0xc000..))]
  pub name: u16be,
  pub kind: u16be,
  pub class: u16be,
  pub ttl: u32be,
  pub rdlength: u16be,
  pub rdata: [u8],
}

/// The data of an `A` record: an IPv4 address.
#[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
pub struct A(pub [u8; 4]);

/// A label: a length (up to 63), and that many bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 1)]
pub struct Label {
  #[pun(valid(..=63))]
  pub length: u8,
  pub text: [u8],
}

// -------------------------------------------------------------------------------------------------
// DHCP

/// A DHCP (BOOTP) message: a fixed header, the magic cookie, then options.
#[derive(Pun)]
#[repr(C)]
// The options end with an "end" option (255), in the message; pads (0) are single bytes, the
// other options are a code, a length and a value.
#[pun(assert({
  let end = walk(
    240,
    |at| be(at, 1) != 255,
    |at| if be(at, 1) == 0 { at + 1 } else { at + 2 + be(at + 1, 1) },
  );
  end < size_of_val(self)
}))]
// The DHCP message type (option 53) is there, of a known value. A second walk, in another rule:
// an expression has at most one.
#[pun(assert({
  let at = walk(
    240,
    |at| be(at, 1) != 53 && be(at, 1) != 255,
    |at| if be(at, 1) == 0 { at + 1 } else { at + 2 + be(at + 1, 1) },
  );
  be(at, 1) == 53 && be(at + 1, 1) == 1 && be(at + 2, 1) >= 1 && be(at + 2, 1) <= 8
}))]
#[pun(get(broadcast: bool = flags & 0x8000 != 0))]
pub struct Dhcp {
  /// Request (1) or reply (2).
  #[pun(valid(1..=2))]
  pub op: u8,
  pub htype: u8,
  #[pun(valid(..=16))]
  pub hlen: u8,
  pub hops: u8,
  pub xid: u32be,
  pub secs: u16be,
  pub flags: u16be,
  pub ciaddr: [u8; 4],
  pub yiaddr: [u8; 4],
  pub siaddr: [u8; 4],
  pub giaddr: [u8; 4],
  pub chaddr: [u8; 16],
  pub sname: [u8; 64],
  pub file: [u8; 128],
  pub cookie: u32bec<0x6382_5363>,
  pub options: [u8],
}

/// A DHCP option that has a length (all but pad and end).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct DhcpOption {
  #[pun(valid(1..=254))]
  pub code: u8,
  pub length: u8,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// NTP

/// An NTP packet (versions 1 to 4).
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(get(
  leap: u8 = li_vn_mode >> 6,
  version: u8 = (li_vn_mode >> 3) & 7,
  mode: u8 = li_vn_mode & 7,
))]
#[pun(assert((li_vn_mode >> 3) & 7 >= 1 && (li_vn_mode >> 3) & 7 <= 4 && li_vn_mode & 7 != 0))]
#[pun(assert(stratum <= 16))]
pub struct Ntp {
  /// Leap indicator (2 bits), version (3 bits), mode (3 bits).
  pub li_vn_mode: u8,
  pub stratum: u8,
  /// The poll interval, in log2 seconds.
  pub poll: i8,
  /// The precision of the clock, in log2 seconds.
  pub precision: i8,
  pub root_delay: u32be,
  pub root_dispersion: u32be,
  pub reference_id: [u8; 4],
  pub reference: u64be,
  pub origin: u64be,
  pub receive: u64be,
  pub transmit: u64be,
}

/// An NTP packet followed by extension fields (NTPv4, RFC 7822), as bytes.
#[derive(Pun)]
#[repr(C)]
pub struct NtpWithExtensions {
  pub header: Ntp,
  pub extensions: [u8],
}

/// An NTP extension field: at least 16 bytes, a multiple of 4.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length)]
#[pun(assert(length >= 16 && length % 4 == 0))]
pub struct Extension {
  pub kind: u16be,
  pub length: u16be,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// RADIUS

/// A RADIUS packet: attributes, each of at least 2 bytes, fill it. (Its authenticators are MD5
/// hashes with a shared secret: not something the bytes say.)
#[derive(Pun)]
#[repr(C)]
#[pun(size = length)]
#[pun(assert(walk(
  20,
  |at| at < length,
  |at| if be(at + 1, 1) < 2 { 1 / 0 } else { at + be(at + 1, 1) },
) == length))]
pub struct Radius {
  #[pun(valid(1..=5, 11..=13, 40..=45))]
  pub code: u8,
  pub identifier: u8,
  #[pun(valid(20..=4096))]
  pub length: u16be,
  pub authenticator: [u8; 16],
  pub attributes: [u8],
}

/// A RADIUS attribute.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length)]
pub struct Attribute {
  pub kind: u8,
  #[pun(valid(2..))]
  pub length: u8,
  pub value: [u8],
}

/// The User-Name attribute: text of 1 to 253 bytes, UTF-8.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(length >= 3))]
pub struct UserName {
  pub kind: u8c<1>,
  pub length: u8,
  #[pun(len = length - 2)]
  pub name: str,
}

// -------------------------------------------------------------------------------------------------
// NetFlow

/// NetFlow v5: a header, then as many flow records as it counts.
#[derive(Pun)]
#[repr(C)]
pub struct NetFlow5 {
  pub version: u16bec<5>,
  #[pun(valid(1..=30))]
  pub count: u16be,
  pub uptime: u32be,
  pub seconds: u32be,
  pub nanoseconds: u32be,
  pub sequence: u32be,
  pub engine_type: u8,
  pub engine_id: u8,
  pub sampling: u16be,
  #[pun(len = count)]
  pub records: [FlowRecord],
}

/// A flow: its first packet was seen before its last, and its masks are of at most 32 bits.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(first <= last))]
pub struct FlowRecord {
  pub source: [u8; 4],
  pub destination: [u8; 4],
  pub next_hop: [u8; 4],
  pub input: u16be,
  pub output: u16be,
  pub packets: u32be,
  pub octets: u32be,
  pub first: u32be,
  pub last: u32be,
  pub source_port: u16be,
  pub destination_port: u16be,
  pub pad1: u8,
  pub tcp_flags: u8,
  pub protocol: u8,
  pub tos: u8,
  pub source_as: u16be,
  pub destination_as: u16be,
  #[pun(valid(..=32))]
  pub source_mask: u8,
  #[pun(valid(..=32))]
  pub destination_mask: u8,
  pub pad2: u16be,
}

// -------------------------------------------------------------------------------------------------
// STUN

/// A STUN message (RFC 8489).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 20)]
#[pun(assert(length % 4 == 0))]
// The attributes fill the message, each padded to 4 bytes.
#[pun(assert(walk(
  20,
  |at| at < size_of_val(self),
  |at| at + 4 + (be(at + 2, 2) + 3) / 4 * 4,
) == size_of_val(self)))]
// A FINGERPRINT, if it is the last attribute: the CRC-32 of what is before it, XOR "STUN".
#[pun(assert(size_of_val(self) < 28
  || be(size_of_val(self) - 8, 4) != 0x8028_0004
  || (crc32(0, size_of_val(self) - 8) ^ 0x5354_554e) == be(size_of_val(self) - 4, 4)))]
#[pun(get(
  class: u8 = ((kind >> 4) & 1) | ((kind >> 7) & 2),
  method: u16 = (kind & 0xf) | ((kind >> 1) & 0x70) | ((kind >> 2) & 0xf80),
))]
pub struct Stun {
  /// The method and the class, interleaved; the two first bits are 0.
  #[pun(valid(..=0x3fff))]
  pub kind: u16be,
  pub length: u16be,
  pub cookie: u32bec<0x2112_a442>,
  pub transaction: [u8; 12],
  pub attributes: [u8],
}

/// A STUN attribute, with its padding.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + (length + 3) / 4 * 4)]
pub struct StunAttribute {
  pub kind: u16be,
  pub length: u16be,
  #[pun(regions(value = length, padding))]
  pub data: [u8],
}

/// XOR-MAPPED-ADDRESS, IPv4: the port and the address are XOR-ed with the magic cookie.
#[derive(Pun)]
#[repr(C)]
#[pun(get(port: u16 = x_port ^ 0x2112, address: u32 = x_address ^ 0x2112_a442))]
pub struct XorMappedV4 {
  pub kind: u16bec<0x0020>,
  pub length: u16bec<8>,
  pub reserved: u8,
  pub family: u8c<1>,
  pub x_port: u16be,
  pub x_address: u32be,
}

// -------------------------------------------------------------------------------------------------
// RTP and RTCP

/// An RTP packet (RFC 3550): CSRCs, an extension, a payload, padding.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(first >> 6 == 2))]
// Padding (the P bit): the last byte counts it, itself included, and it fits after the header.
#[pun(assert(first & 0x20 == 0
  || (be(size_of_val(self) - 1, 1) >= 1
    && 12 + (first & 15) * 4 + be(size_of_val(self) - 1, 1) <= size_of_val(self))))]
#[pun(get(
  marker: bool = second & 0x80 != 0,
  payload_type: u8 = second & 0x7f,
  csrc_count: usize = first & 15,
  has_extension: bool = first & 0x10 != 0,
))]
pub struct Rtp {
  /// Version (2 bits, 2), padding (1 bit), extension (1 bit), CSRC count (4 bits).
  pub first: u8,
  /// Marker (1 bit), payload type (7 bits).
  pub second: u8,
  pub sequence: u16be,
  pub timestamp: u32be,
  pub ssrc: u32be,
  #[pun(regions(
    csrcs: [u32be] = (first & 15) * 4,
    // The extension: a profile word and a length in 32 bits words, then the words.
    extension = if first & 0x10 != 0 { 4 + be(12 + (first & 15) * 4 + 2, 2) * 4 } else { 0 },
    payload = size_of_val(self) - 12 - (first & 15) * 4
      - (if first & 0x10 != 0 { 4 + be(12 + (first & 15) * 4 + 2, 2) * 4 } else { 0 })
      - (if first & 0x20 != 0 { be(size_of_val(self) - 1, 1) } else { 0 }),
    padding,
  ))]
  pub data: [u8],
}

/// An RTCP sender report: report blocks, counted, then profile-specific extensions.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (length + 1) * 4)]
#[pun(assert(first >> 6 == 2 && first & 0x20 == 0))]
pub struct SenderReport {
  pub first: u8,
  pub kind: u8c<200>,
  pub length: u16be,
  pub ssrc: u32be,
  pub ntp: u64be,
  pub rtp_timestamp: u32be,
  pub packets: u32be,
  pub octets: u32be,
  #[pun(regions(reports: [ReportBlock] = (first & 31) * 24, extensions))]
  pub data: [u8],
}

/// A reception report block.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct ReportBlock {
  pub ssrc: u32be,
  pub fraction_lost: u8,
  pub lost: [u8; 3],
  pub highest_sequence: u32be,
  pub jitter: u32be,
  pub last_sr: u32be,
  pub delay: u32be,
}

/// Any RTCP packet.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (length + 1) * 4)]
#[pun(assert(first >> 6 == 2))]
pub struct Rtcp {
  pub first: u8,
  #[pun(valid(192..=223))]
  pub kind: u8,
  pub length: u16be,
  pub body: [u8],
}

/// A compound RTCP packet: it starts with a sender or a receiver report, and its packets, of
/// version 2 each, fill it.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(be(1, 1) == 200 || be(1, 1) == 201))]
#[pun(assert(walk(
  0,
  |at| at < size_of_val(self),
  |at| if be(at, 1) >> 6 != 2 { 1 / 0 } else { at + (be(at + 2, 2) + 1) * 4 },
) == size_of_val(self)))]
pub struct Compound(pub [u8]);

// -------------------------------------------------------------------------------------------------
// One thing or another

/// A datagram, by what it is: the first of these that the bytes are.
#[derive(Pun)]
pub enum AnyMessage<'a> {
  Stun(&'a Stun),
  Dhcp(&'a Dhcp),
  NetFlow(&'a NetFlow5),
  Ntp(&'a Ntp),
  Radius(&'a Radius),
  Dns(&'a Dns),
}

// -------------------------------------------------------------------------------------------------
// Use

/// A DNS query for `example.com`, type A, class IN, as bytes.
const QUERY: [u8; 29] = [
  0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 7, b'e', b'x', b'a',
  b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0, 0x00, 0x01, 0x00, 0x01,
];

/// Its response, as bytes: the question, then an `A` record whose name points to the name of the
/// question.
const RESPONSE: [u8; 45] = [
  0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 7, b'e', b'x', b'a',
  b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0, 0x00, 0x01, 0x00, 0x01, 0xc0, 0x0c, 0x00, 0x01,
  0x00, 0x01, 0x00, 0x00, 0x0e, 0x10, 0x00, 0x04, 93, 184, 216, 34,
];

/// The header of a DHCPDISCOVER up to the hardware address of the client, as bytes.
const DISCOVER_HEADER: [u8; 34] = [
  1, 1, 6, 0, 0x39, 0x03, 0xf3, 0x26, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  0, 0x00, 0x0c, 0x29, 0x3e, 0x53, 0xf7,
];

/// The magic cookie and the options of the DHCPDISCOVER: the message type (DISCOVER), a pad, a
/// parameter request list, the end, then padding.
const DISCOVER_OPTIONS: [u8; 16] = [0x63, 0x82, 0x53, 0x63, 53, 1, 1, 0, 55, 3, 1, 3, 6, 255, 0, 0];

/// An NTP client request (version 4, mode 3), as `ntpdate` sends it.
const NTP: [u8; 48] = [
  0x23, 0, 6, 0xec, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xe9, 0x3a, 0x4b, 0x5c, 0x80, 0, 0, 0,
];

/// A RADIUS Access-Request, as bytes: User-Name "alice", NAS-IP-Address 192.0.2.1.
const RADIUS: [u8; 33] = [
  1, 42, 0, 33, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
  0x11, 0x11, 1, 7, b'a', b'l', b'i', b'c', b'e', 4, 6, 192, 0, 2, 1,
];

/// NetFlow v5, as bytes: a header, two TCP flows.
const NETFLOW: [u8; 120] = [
  0x00, 0x05, 0x00, 0x02, 0x00, 0x00, 0x10, 0x00, 0x5f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x02,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xb4,
  0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0xc8, 0xc3, 0x50, 0x00, 0x50, 0x00, 0x1b, 0x06, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x02,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xb4,
  0x00, 0x00, 0x00, 0x96, 0x00, 0x00, 0x00, 0x96, 0xc3, 0x50, 0x01, 0xbb, 0x00, 0x1b, 0x06, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00,
];

/// A STUN binding response, as bytes: XOR-MAPPED-ADDRESS 192.0.2.1:32853, SOFTWARE "test!"
/// (padded), FINGERPRINT.
const STUN: [u8; 52] = [
  0x01, 0x01, 0x00, 0x20, 0x21, 0x12, 0xa4, 0x42, 0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86,
  0xfa, 0x87, 0xdf, 0xae, 0x00, 0x20, 0x00, 0x08, 0x00, 0x01, 0xa1, 0x47, 0xe1, 0x12, 0xa6, 0x43,
  0x80, 0x22, 0x00, 0x05, b't', b'e', b's', b't', b'!', 0x00, 0x00, 0x00, 0x80, 0x28, 0x00, 0x04,
  0xa7, 0x51, 0xfd, 0x9a,
];

/// An RTP packet, as bytes: version 2, padding, an extension, a CSRC; marker, payload type 96.
const RTP: [u8; 34] = [
  0xb1, 0xe0, 0, 1, 0, 0, 0, 160, 0x11, 0x22, 0x33, 0x44, 0xaa, 0xbb, 0xcc, 0xdd, 0xbe, 0xde, 0, 1,
  1, 2, 3, 4, b'p', b'a', b'y', b'l', b'o', b'a', b'd', 0, 0, 3,
];

/// A compound RTCP packet, as bytes: a sender report of one report block, then a source
/// description.
const RTCP: [u8; 64] = [
  0x81, 200, 0, 12, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  0, 2, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x81, 202, 0, 2, 0, 0, 0, 1, 1,
  2, b'h', b'i',
];

pub fn main() {
  // ---- DNS: a query, its name, label by label.
  let query: &Dns = try_pun_ref(&QUERY[..]).unwrap();
  assert_eq!((query.id.get(), query.response(), query.recursion_desired()), (0x1234, false, true));
  assert_eq!((query.opcode(), query.questions.get()), (0, 1));
  let question: &Question = try_pun_ref(&query.body[..]).unwrap();
  assert_eq!(&question.name().unwrap().0, &QUERY[12..25]);
  assert_eq!(question.fixed().unwrap(), &QuestionFixed { kind: 1.into(), class: 1.into() });
  // The labels: a sequence of things of their own sizes is read by use code.
  let (mut rest, mut labels) = (&question.name().unwrap().0[..], vec![]);
  while let Ok((label, next)) = from_head::<Label>(rest) {
    if label.length == 0 {
      break;
    }
    labels.push(std::str::from_utf8(&label.text).unwrap());
    rest = next;
  }
  assert_eq!(labels, ["example", "com"]);
  // The response: a record whose name is a pointer to the name of the question.
  let response: &Dns = try_pun_ref(&RESPONSE[..]).unwrap();
  assert_eq!((response.response(), response.rcode(), response.answers.get()), (true, 0, 1));
  assert!(!response.authoritative() && !response.truncated());
  let (record, rest) = from_head::<PointerRecord>(&RESPONSE[29..]).unwrap();
  assert_eq!((record.name.get() & 0x3fff, record.ttl.get(), rest.len()), (12, 3600, 0));
  let address: &A = try_pun_ref(&record.rdata).unwrap();
  assert_eq!(address.0, [93, 184, 216, 34]);
  assert!(try_pun_ref::<_, Name>(&RESPONSE[29..31]).is_ok());

  // ---- DNS: what is not a message.
  // A label longer than the name: the message, and the name of its question, are not valid.
  let mut long = QUERY;
  long[20] = 9;
  assert_eq!(try_pun_ref::<_, Dns>(&long[..]).err(), Some(Error::Validity));
  let question: &Question = try_pun_ref(&long[12..]).unwrap();
  assert_eq!(question.name().err(), Some(Error::Validity));
  // A label of a reserved form (`0x40`); no end to the name; opcode 3; the Z bit.
  for (at, value) in [(12, 0x47), (2, 0x18), (3, 0x40)] {
    let mut bytes = QUERY;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Dns>(&bytes[..]).err(), Some(Error::Validity));
  }
  assert_eq!(try_pun_ref::<_, Name>(&[0x47, 0][..]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Dns>(&QUERY[..24]).err(), Some(Error::Validity));
  // A pointer to itself: the walk loops, and is cut. A pointer to a name after the question.
  let mut looping = [&QUERY[..12], &[0xc0, 0x0c, 0, 1, 0, 1]].concat();
  assert_eq!(try_pun_ref::<_, Dns>(&looping[..]).err(), Some(Error::Validity));
  looping[13] = 18;
  looping.extend([1, b'a', 0]);
  assert!(try_pun_ref::<_, Dns>(&looping[..]).is_ok());
  // A record whose name is not a pointer; one that is longer than the bytes; a name that ends
  // before the bytes do.
  let mut record = RESPONSE[29..].to_vec();
  record[0] = 0x03;
  assert_eq!(from_head::<PointerRecord>(&record).err(), Some(Error::Validity));
  let mut record = RESPONSE[29..].to_vec();
  record[11] = 5;
  assert_eq!(from_head::<PointerRecord>(&record).err(), Some(Error::Size));
  assert_eq!(try_pun_ref::<_, Name>(&QUERY[12..26]).err(), Some(Error::Validity));

  // ---- DHCP: a header, a cookie, options down to "end".
  let discover = [&DISCOVER_HEADER[..], &[0; 202], &DISCOVER_OPTIONS].concat();
  let dhcp: &Dhcp = try_pun_ref(&discover[..]).unwrap();
  assert_eq!((dhcp.op, dhcp.xid.get(), dhcp.broadcast()), (1, 0x3903_f326, true));
  assert_eq!(dhcp.chaddr[..6], [0x00, 0x0c, 0x29, 0x3e, 0x53, 0xf7]);
  let (option, rest) = from_head::<DhcpOption>(&dhcp.options).unwrap();
  assert_eq!((option.code, &option.value), (53, &[1][..]));
  // A pad, then the parameter request list.
  let (option, _) = from_head::<DhcpOption>(&rest[1..]).unwrap();
  assert_eq!((option.code, &option.value), (55, &[1, 3, 6][..]));
  // An op, a hardware address length, a cookie; no "end" (a pad instead); an option longer than
  // the message; no message type (the first option is another one); not a message type.
  for (at, value) in [(0, 3), (2, 17), (239, 0x64), (249, 0), (245, 30), (240, 54), (242, 9)] {
    let mut bytes = discover.clone();
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Dhcp>(&bytes[..]).err(), Some(Error::Validity));
  }
  assert_eq!(try_pun_ref::<_, Dhcp>(&discover[..239]).err(), Some(Error::Size));

  // ---- NTP: bit fields, signed fields, extension fields.
  let ntp: &Ntp = try_pun_ref(&NTP).unwrap();
  assert_eq!((ntp.leap(), ntp.version(), ntp.mode(), ntp.stratum), (0, 4, 3, 0));
  assert_eq!((ntp.poll, ntp.precision, ntp.transmit.get() >> 32), (6, -20, 0xe93a_4b5c));
  let mut extended = NTP.to_vec();
  extended.extend([0x01, 0x04, 0, 16, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  let packet: &NtpWithExtensions = try_pun_ref(&extended[..]).unwrap();
  let (extension, rest) = from_head::<Extension>(&packet.extensions).unwrap();
  assert_eq!((extension.kind.get(), extension.value.len(), rest.len()), (0x0104, 12, 0));
  extended[48 + 3] = 15;
  assert_eq!(from_head::<Extension>(&extended[48..]).err(), Some(Error::Validity));
  // Version 0, version 5, mode 0, stratum 17.
  for (at, value) in [(0, 0x03), (0, 0x2b), (0, 0x20), (1, 17)] {
    let mut bytes = NTP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Ntp>(&bytes).err(), Some(Error::Validity));
  }
  assert_eq!(try_pun_ref::<_, Ntp>(&NTP[..47]).err(), Some(Error::Size));

  // ---- RADIUS: attributes that fill the packet, a user name that is UTF-8.
  let mut stream = RADIUS.to_vec();
  stream.extend([0xee, 0xff]);
  let (packet, rest) = from_head::<Radius>(&stream).unwrap();
  assert_eq!((packet.code, packet.identifier, rest), (1, 42, &[0xee, 0xff][..]));
  let user: &UserName = try_pun_ref(&packet.attributes[..7]).unwrap();
  assert_eq!(&user.name, "alice");
  let (_, rest) = from_head::<Attribute>(&packet.attributes).unwrap();
  let (address, rest) = from_head::<Attribute>(rest).unwrap();
  assert_eq!((address.kind, &address.value, rest.len()), (4, &[192, 0, 2, 1][..], 0));
  // Code 6; an attribute of length 1; attributes that do not fill the packet.
  for (at, value) in [(0, 6), (21, 1), (21, 8), (28, 9)] {
    let mut bytes = RADIUS;
    bytes[at] = value;
    assert_eq!(from_head::<Radius>(&bytes).err(), Some(Error::Validity));
  }
  let mut bytes = RADIUS;
  bytes[24] = 0xff;
  assert_eq!(try_pun_ref::<_, UserName>(&bytes[20..27]).err(), Some(Error::Validity));

  // ---- NetFlow: records, counted.
  let flows: &NetFlow5 = try_pun_ref(&NETFLOW[..]).unwrap();
  assert_eq!((flows.sequence.get(), flows.records.len()), (9, 2));
  let ports: Vec<u16> = flows.records.iter().map(|record| record.destination_port.get()).collect();
  assert_eq!((ports, flows.records[0].protocol), (vec![80, 443], 6));
  // A count of 3; a mask of 33 bits; a flow that ends before it starts.
  for (at, value) in [(3, 3), (24 + 44, 33), (24 + 48 + 31, 99)] {
    let mut bytes = NETFLOW;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, NetFlow5>(&bytes[..]).err(), Some(Error::Validity));
  }
  // What follows the records is not theirs.
  let longer = [&NETFLOW[..], &[1, 2]].concat();
  assert_eq!(from_head::<NetFlow5>(&longer).unwrap().1, [1, 2]);

  // ---- STUN: attributes, a fingerprint, an address XOR-ed with the cookie.
  let stun: &Stun = try_pun_ref(&STUN[..]).unwrap();
  assert_eq!((stun.class(), stun.method()), (2, 1));
  let mapped: &XorMappedV4 = try_pun_ref(&stun.attributes[..12]).unwrap();
  assert_eq!((mapped.port(), mapped.address().to_be_bytes()), (32853, [192, 0, 2, 1]));
  let (software, rest) = from_head::<StunAttribute>(&stun.attributes[12..]).unwrap();
  assert_eq!((software.value(), software.padding().len(), rest.len()), (&b"test!"[..], 3, 8));
  // The first bits, the length, the cookie, the transaction, an attribute, the fingerprint.
  for at in [0, 3, 4, 10, 25, 36, 47] {
    let mut bytes = STUN;
    bytes[at] ^= 0x01;
    assert_eq!(try_pun_ref::<_, Stun>(&bytes[..]).err(), Some(Error::Validity));
  }
  // Without the fingerprint (and of the length that says so), it is a message.
  let mut short = STUN[..44].to_vec();
  short[3] -= 8;
  assert!(try_pun_ref::<_, Stun>(&short[..]).is_ok());
  // An attribute that runs past the message.
  short[35] = 9;
  assert_eq!(try_pun_ref::<_, Stun>(&short[..]).err(), Some(Error::Validity));

  // ---- RTP: CSRCs, an extension, padding.
  let rtp: &Rtp = try_pun_ref(&RTP[..]).unwrap();
  assert_eq!(
    (rtp.marker(), rtp.payload_type(), rtp.csrc_count(), rtp.has_extension()),
    (true, 96, 1, true)
  );
  assert_eq!(rtp.csrcs().unwrap()[0].get(), 0xaabb_ccdd);
  assert_eq!(
    (rtp.extension().len(), rtp.payload(), rtp.padding()),
    (8, &b"payload"[..], &[0, 0, 3][..])
  );
  // Version 1; padding of 0, padding longer than the packet; an extension longer than it.
  for (at, value) in [(0, 0x71), (33, 0), (33, 30), (19, 9)] {
    let mut bytes = RTP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Rtp>(&bytes[..]).err(), Some(Error::Validity));
  }
  let plain: &Rtp = try_pun_ref(&[0x80u8, 0x00, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 9, 9][..]).unwrap();
  assert_eq!(
    (plain.csrcs().unwrap().len(), plain.extension().len(), plain.payload()),
    (0, 0, &[9, 9][..])
  );

  // ---- RTCP: a compound packet, a report, then any packet.
  assert!(try_pun_ref::<_, Compound>(&RTCP[..]).is_ok());
  let (report, rest) = from_head::<SenderReport>(&RTCP).unwrap();
  let blocks = report.reports().unwrap();
  assert_eq!((blocks.len(), blocks[0].ssrc.get(), report.extensions().len()), (1, 2, 0));
  let (description, rest) = from_head::<Rtcp>(rest).unwrap();
  assert_eq!((description.kind, rest.len()), (202, 0));
  // It must start with a report; a length that is not the one of the packet; a version 1.
  let reversed = [&RTCP[52..], &RTCP[..52]].concat();
  assert_eq!(try_pun_ref::<_, Compound>(&reversed[..]).err(), Some(Error::Validity));
  for (at, value) in [(3, 13), (52, 0x41)] {
    let mut bytes = RTCP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Compound>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- One thing or another.
  let describe = |bytes: &[u8]| match AnyMessage::from_bytes(bytes) {
    Ok(AnyMessage::Stun(stun)) => format!("STUN method {}", stun.method()),
    Ok(AnyMessage::Dhcp(dhcp)) => format!("DHCP {:#x}", dhcp.xid),
    Ok(AnyMessage::NetFlow(flows)) => format!("NetFlow, {} flows", flows.records.len()),
    Ok(AnyMessage::Ntp(ntp)) => format!("NTP v{}", ntp.version()),
    Ok(AnyMessage::Radius(radius)) => format!("RADIUS code {}", radius.code),
    Ok(AnyMessage::Dns(dns)) => format!("DNS {:#x}, {} answers", dns.id, dns.answers),
    Err(error) => format!("not a message: {error}"),
  };
  assert_eq!(describe(&STUN), "STUN method 1");
  assert_eq!(describe(&discover), "DHCP 0x3903f326");
  assert_eq!(describe(&NETFLOW), "NetFlow, 2 flows");
  assert_eq!(describe(&NTP), "NTP v4");
  assert_eq!(describe(&RADIUS), "RADIUS code 1");
  assert_eq!(describe(&RESPONSE), "DNS 0x1234, 1 answers");
  assert_eq!(describe(&[0; 4]), "not a message: value is not valid for the target type");

  println!("ok");
}
