//! Transport and session protocols, in safe Rust, with nothing but `#[derive(Pun)]` and its
//! options: TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket frames, SSH
//! binary packets, MQTT control packets, CoAP messages and SCTP packets.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads messages in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these protocols, and what tells it:
//!
//! | Specificity                                     | Told by                                    |
//! |-------------------------------------------------|--------------------------------------------|
//! | a TLS record tells what it carries              | `tag(ContentType = u8c<22>)`               |
//! | lengths of 24 bits (TLS handshakes, HTTP/2)     | `u24be`, `size = length + 4`               |
//! | vectors that follow each other                  | `regions(items = length, rest: R)`         |
//! | the chain of vectors fills the message          | `assert({ let .. = be(..); .. })`          |
//! | a server name is text                           | a `str` field                              |
//! | QUIC connection IDs of variable length          | `regions(dcid = dcid_length, ..)`          |
//! | QUIC integers of 1, 2, 4 or 8 bytes             | `1 << (be(at, 1) >> 6)`, `be(at, 8) >> ..` |
//! | HTTP/2 SETTINGS: an array of settings           | `[Setting]`: a multiple of 6 bytes         |
//! | HTTP/2 padding and priority, told by flags      | `if flags & 8 != 0 { .. }` in `regions`    |
//! | the HTTP/2 connection preface                   | `u128bec<..>`, `u64bec<..>`                |
//! | WebSocket lengths of 7, 16 or 64 bits           | a struct per form, an enum of references   |
//! | WebSocket: shortest forms, short control frames | `valid(126..)`, a head type per kind       |
//! | SSH: a MAC whose size the bytes do not tell     | a const parameter: `SshPacket<_, MAC>`     |
//! | SSH: padding to the block size                  | `assert((packet_length + 4) % BLOCK ..)`   |
//! | SSH name-lists of US-ASCII                      | `regions(names: [u8in<0x21, 0x7e>] = ..)`  |
//! | MQTT: a remaining length of 1 to 4 bytes        | a struct per length, an enum of references |
//! | MQTT: flags that depend on the packet type      | `assert({ let kind = ..; .. })`            |
//! | MQTT strings: a length, then UTF-8              | `regions(text: str = length, rest: R)`     |
//! | CoAP options: nibbles that tell 0, 1 or 2 bytes | a struct per form, an enum of references   |
//! | SCTP chunks, padded to 4 bytes                  | `size = (length + 3) / 4 * 4`              |
//!
//! What is not covered, and why:
//!
//! - WebSocket, MQTT, CoAP: the size of a frame, a packet or an option is at an offset, or of a
//!   width, that a byte of its head tells. A `size` rule reads fields, not `be(..)`: each form is
//!   a struct of its own, and an enum of references tells them apart. This is the current way;
//!   with `be` in `size` (planned), each is one struct.
//! - CoAP: that the options are well formed, up to the payload marker, is a walk over them whose
//!   step is 75 operations; an expression of a walk has at most 48 (planned: more). So a payload
//!   marker without a payload is not rejected by the message, and options are checked one by one
//!   by use code. The option numbers are sums of the deltas before them: state carried along a
//!   sequence, which use code keeps.
//! - SCTP: the checksum is a CRC-32C. There is no CRC-32C in the expressions, and computing one
//!   by a walk, a byte per step, takes 50 operations a step, 2 more than allowed (planned: more).
//!   It is not checked.
//! - TLS: a handshake message can span records, and records are encrypted after the handshake;
//!   reassembly and decryption are not views of the bytes. Typed regions are checked when they
//!   are read: the cipher suites of a ClientHello are only checked when asked for.
//! - QUIC: what follows the long header is protected (header protection, AEAD): not a view.
//!   HTTP/2 header blocks are HPACK, Huffman codes and indexes into a table: not a view either.
//! - WebSocket payloads from a client are masked: unmasking is a transformation, done by use code.
#![deny(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, Tag, from_head,
  num::{u8c, u8in, u16be, u16bec, u24be, u32be, u32bec, u32le, u64be, u64bec, u128bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// TLS

/// The content type of a record, when what it carries is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct ContentType(#[pun(valid(20..=24))] pub u8);

/// A TLS record: its length is at most 2^14 + 2048 (`TLSCiphertext`).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 5)]
#[pun(assert(length <= 18432))]
pub struct Record<P: ?Sized + Tag<ContentType> = [u8]> {
  pub content_type: P::Tag,
  #[pun(valid(0x0300..=0x0304))]
  pub version: u16be,
  pub length: u16be,
  pub fragment: P,
}

/// The type of a handshake message, when it is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct HandshakeType(pub u8);

/// A handshake message.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(ContentType = u8c<22>))]
#[pun(size = length + 4)]
pub struct Handshake<P: ?Sized + Tag<HandshakeType> = [u8]> {
  pub msg_type: P::Tag,
  pub length: u24be,
  pub body: P,
}

/// A vector of up to 255 bytes, then what follows it: `R`.
#[derive(Pun)]
#[repr(C)]
pub struct V8<R: ?Sized = [u8]> {
  pub length: u8,
  /// A type that only a region names must be named by a field too.
  pub rest_type: PhantomData<R>,
  #[pun(regions(items = length, rest: R))]
  pub data: [u8],
}

/// A vector of up to 65535 bytes, then what follows it.
#[derive(Pun)]
#[repr(C)]
pub struct V16<R: ?Sized = [u8]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(items = length, rest: R))]
  pub data: [u8],
}

/// Cipher suites: 2 to 65534 bytes of `u16`s, then what follows them.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(length >= 2 && length % 2 == 0))]
pub struct CipherSuites<R: ?Sized = [u8]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(suites: [u16be] = length, rest: R))]
  pub data: [u8],
}

/// A ClientHello: a version, a random, then vectors that follow each other.
///
/// The vectors are the session, the cipher suites, the compression methods and the extensions,
/// a type each, whose `rest` is the next one. The extensions end the message: what remains
/// after them is `[u8; 0]`.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(HandshakeType = u8c<1>))]
// The chain fills the message (checked here, at once: typed regions are checked when read).
#[pun(assert({
  let suites = 35 + be(34, 1);
  let compression = suites + 2 + be(suites, 2);
  let extensions = compression + 1 + be(compression, 1);
  extensions + 2 + be(extensions, 2) == size_of_val(self)
}))]
pub struct ClientHello {
  #[pun(valid(0x0300..=0x0303))]
  pub legacy_version: u16be,
  pub random: [u8; 32],
  pub session: V8<CipherSuites<V8<V16<[u8; 0]>>>>,
}

/// An extension.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 4)]
pub struct TlsExtension {
  pub kind: u16be,
  pub length: u16be,
  pub data: [u8],
}

/// The server name extension, of a single host name. (A host name is ASCII: a `str` is UTF-8,
/// which is more.)
#[derive(Pun)]
#[repr(C)]
#[pun(assert(list_length + 2 == size_of_val(self) && name_length + 5 == size_of_val(self)))]
pub struct ServerName {
  pub list_length: u16be,
  pub name_type: u8c<0>,
  pub name_length: u16be,
  pub name: str,
}

// -------------------------------------------------------------------------------------------------
// QUIC

/// Any long header packet of QUIC: the form and fixed bits set, connection IDs of up to 20 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(6 + dcid_length, 1) <= 20))]
#[pun(get(packet_type: u8 = (first >> 4) & 3))]
pub struct LongHeader {
  #[pun(valid(0xc0..=0xff))]
  pub first: u8,
  pub version: u32be,
  #[pun(valid(..=20))]
  pub dcid_length: u8,
  #[pun(regions(dcid = dcid_length, scid_length = 1, scid = be(6 + dcid_length, 1), rest))]
  pub data: [u8],
}

/// An Initial packet of QUIC v1: a token, the length of what remains of the packet (packet
/// number and protected payload), then other packets, coalesced.
///
/// The length of the token and the length of the packet are variable-length integers: their two
/// first bits tell their size, 1, 2, 4 or 8 bytes (`1 << (be(at, 1) >> 6)`), the other bits are
/// the value. Each is read as 8 bytes, shifted (`be(at, 8) >> (64 - 8 * n)`), and masked: at
/// least 20 bytes follow both in a valid packet. A region cannot name what another one computed:
/// each recomputes the offsets it needs.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(6 + dcid_length, 1) <= 20))]
// The packet number (1 to 4 bytes) and the 16 bytes of the AEAD tag, at least.
#[pun(assert({
  let at = 7 + dcid_length + be(6 + dcid_length, 1);
  let n = 1 << (be(at, 1) >> 6);
  let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
  let m = 1 << (be(at, 1) >> 6);
  ((be(at, 8) >> (64 - 8 * m)) & ((1 << (8 * m - 2)) - 1)) >= 20
}))]
pub struct Initial {
  /// Long header, fixed bit, type 0 (Initial); the 4 low bits are protected.
  #[pun(valid(0xc0..=0xcf))]
  pub first: u8,
  pub version: u32bec<1>,
  #[pun(valid(..=20))]
  pub dcid_length: u8,
  #[pun(regions(
    dcid = dcid_length,
    scid_length = 1,
    scid = be(6 + dcid_length, 1),
    token_length = 1 << (be(7 + dcid_length + be(6 + dcid_length, 1), 1) >> 6),
    token = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      (be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1)
    },
    length = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
      1 << (be(at, 1) >> 6)
    },
    protected = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
      let m = 1 << (be(at, 1) >> 6);
      (be(at, 8) >> (64 - 8 * m)) & ((1 << (8 * m - 2)) - 1)
    },
    coalesced,
  ))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// HTTP/2

/// The type of an HTTP/2 frame, when it is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct FrameType(pub u8);

/// An HTTP/2 frame, of any type unless told. (The maximum size of a frame is a setting of the
/// connection: not in the bytes.)
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(get(stream_id: u32 = stream & 0x7fff_ffff))]
pub struct Frame<P: ?Sized + Tag<FrameType> = [u8]> {
  pub length: u24be,
  pub kind: P::Tag,
  pub flags: u8,
  /// A reserved bit, then the stream identifier (31 bits).
  pub stream: u32be,
  pub payload: P,
}

/// A setting.
#[derive(Pun, Debug, Copy, Clone)]
#[repr(C)]
pub struct Setting {
  pub id: u16be,
  pub value: u32be,
}

/// The payload of SETTINGS: settings, of 6 bytes each, which the slice says.
#[derive(Pun)]
#[repr(transparent)]
#[pun(tag(FrameType = u8c<4>))]
pub struct Settings(pub [Setting]);

/// A whole SETTINGS frame, with what involves both its header and its payload: it is on stream
/// 0, and an ACK is empty.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff == 0))]
#[pun(assert(flags & 1 == 0 || length == 0))]
pub struct SettingsFrame {
  pub length: u24be,
  pub kind: u8c<4>,
  pub flags: u8,
  pub stream: u32be,
  pub settings: [Setting],
}

/// A DATA frame: if PADDED (flag `0x8`), a pad length, the data, then that much padding. A field
/// that a flag tells is a region: the pad length is read again with `be`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff != 0))]
#[pun(get(end_stream: bool = flags & 1 != 0, padded: bool = flags & 8 != 0))]
pub struct DataFrame {
  pub length: u24be,
  pub kind: u8c<0>,
  pub flags: u8,
  pub stream: u32be,
  #[pun(regions(
    pad_length = if flags & 8 != 0 { 1 } else { 0 },
    data = if flags & 8 != 0 { length - 1 - be(9, 1) } else { length },
    padding,
  ))]
  pub payload: [u8],
}

/// A HEADERS frame: a pad length (PADDED), a priority (PRIORITY, `0x20`), a header block
/// fragment, padding.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff != 0))]
pub struct HeadersFrame {
  pub length: u24be,
  pub kind: u8c<1>,
  pub flags: u8,
  pub stream: u32be,
  #[pun(regions(
    pad_length = if flags & 8 != 0 { 1 } else { 0 },
    priority = if flags & 0x20 != 0 { 5 } else { 0 },
    block = length
      - (if flags & 8 != 0 { 1 + be(9, 1) } else { 0 })
      - (if flags & 0x20 != 0 { 5 } else { 0 }),
    padding,
  ))]
  pub payload: [u8],
}

/// The connection preface of a client: `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n`.
#[derive(Pun)]
#[repr(C)]
pub struct Preface {
  pub magic: u128bec<0x5052_4920_2a20_4854_5450_2f32_2e30_0d0a>,
  pub magic_end: u64bec<0x0d0a_534d_0d0a_0d0a>,
}

// -------------------------------------------------------------------------------------------------
// WebSocket

/// The first byte of a frame: FIN, RSV1-3 (0 without extension), an opcode (0 to 2 for data, 8 to
/// 10 for control frames).
#[derive(Pun, Debug, Copy, Clone)]
#[repr(transparent)]
#[pun(assert(self.0 & 0x70 == 0))]
#[pun(assert({ let opcode = self.0 & 0x0f; opcode <= 2 || (opcode >= 8 && opcode <= 10) }))]
// A control frame is not fragmented.
#[pun(assert(self.0 & 0x08 == 0 || self.0 & 0x80 != 0))]
#[pun(get(fin: bool = self.0 & 0x80 != 0, opcode: u8 = self.0 & 0x0f))]
pub struct Head(pub u8);

/// The first byte of a data frame: the lengths of 16 and 64 bits are not for control frames.
#[derive(Pun, Debug, Copy, Clone)]
#[repr(transparent)]
#[pun(assert(self.0 & 0x70 == 0 && self.0 & 0x0f <= 2))]
#[pun(get(fin: bool = self.0 & 0x80 != 0, opcode: u8 = self.0 & 0x0f))]
pub struct DataHead(pub u8);

/// A frame of up to 125 bytes. `L` is the second byte: the mask bit and the length; `K` the key
/// (`[u8; 4]`, or `[u8; 0]` when not masked).
///
/// The size of a frame depends on the form of its length, which the second byte tells. A `size`
/// rule reads fields: each form is a struct ([`Short`], [`Medium`], [`Long`]), and an enum of
/// references ([`ServerFrame`], [`ClientFrame`]) tells them apart. This is the current way: with
/// `be` in `size` (planned), it is one struct.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (length & 0x7f) + 2 + size_of::<K>())]
pub struct Short<L, K> {
  pub head: Head,
  pub length: L,
  pub key: K,
  pub payload: [u8],
}

/// A frame of 126 to 65535 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = extended + 4 + size_of::<K>())]
pub struct Medium<L, K> {
  pub head: DataHead,
  pub length: L,
  /// The shortest form is required.
  #[pun(valid(126..))]
  pub extended: u16be,
  pub key: K,
  pub payload: [u8],
}

/// A frame of 65536 bytes or more (the most significant bit of the length is 0).
#[derive(Pun)]
#[repr(C)]
#[pun(size = extended + 10 + size_of::<K>())]
pub struct Long<L, K> {
  pub head: DataHead,
  pub length: L,
  #[pun(valid(0x1_0000..=0x7fff_ffff_ffff_ffff))]
  pub extended: u64be,
  pub key: K,
  pub payload: [u8],
}

/// A frame from a server: never masked.
#[derive(Pun)]
pub enum ServerFrame<'a> {
  Short(&'a Short<u8in<0x00, 0x7d>, [u8; 0]>),
  Medium(&'a Medium<u8c<0x7e>, [u8; 0]>),
  Long(&'a Long<u8c<0x7f>, [u8; 0]>),
}

/// A frame from a client: always masked.
#[derive(Pun)]
pub enum ClientFrame<'a> {
  Short(&'a Short<u8in<0x80, 0xfd>, [u8; 4]>),
  Medium(&'a Medium<u8c<0xfe>, [u8; 4]>),
  Long(&'a Long<u8c<0xff>, [u8; 4]>),
}

// -------------------------------------------------------------------------------------------------
// SSH

/// An SSH binary packet (RFC 4253), before encryption or with the "none" cipher.
///
/// A length, 4 to 255 bytes of padding, a payload, and a MAC of `MAC` bytes, which the
/// negotiation tells, not the bytes. The packet, without its MAC, is a multiple of the block
/// size, at least 8.
#[derive(Pun)]
#[repr(C)]
#[pun(size = packet_length + 4 + MAC)]
#[pun(assert(packet_length >= padding_length + 1 && packet_length <= 35000))]
#[pun(assert((packet_length + 4) % (if BLOCK > 8 { BLOCK } else { 8 }) == 0))]
pub struct SshPacket<const BLOCK: usize, const MAC: usize> {
  pub packet_length: u32be,
  #[pun(valid(4..))]
  pub padding_length: u8,
  #[pun(regions(payload = packet_length - padding_length - 1, padding = padding_length, mac))]
  pub data: [u8],
}

/// A name-list (a length, then comma-separated US-ASCII names), then what follows it.
#[derive(Pun)]
#[repr(C)]
pub struct NameList<R: ?Sized> {
  pub length: u32be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(names: [u8in<0x21, 0x7e>] = length, rest: R))]
  pub data: [u8],
}

/// What ends KEXINIT.
#[derive(Pun)]
#[repr(C)]
pub struct KexEnd {
  pub first_kex_packet_follows: bool,
  pub reserved: u32bec<0>,
}

/// KEXINIT: a cookie, ten name-lists (key exchange, host key, ciphers, MACs and compressions of
/// both directions, languages), then the end: a chain of ten types.
#[derive(Pun)]
#[repr(C)]
pub struct KexInit {
  pub message: u8c<20>,
  pub cookie: [u8; 16],
  pub lists: NameList<NameList<NameList<NameList<NameList<Lists5>>>>>,
}

/// The five last name-lists of KEXINIT, and its end.
pub type Lists5 = NameList<NameList<NameList<NameList<NameList<KexEnd>>>>>;

// -------------------------------------------------------------------------------------------------
// MQTT

/// The first byte of an MQTT 3.1.1 control packet: a packet type and flags.
///
/// The type is 4 bits, 1 to 14. The flags are 4 bits, which depend on the type: `QoS` and retain
/// of PUBLISH, 2 for PUBREL, SUBSCRIBE and UNSUBSCRIBE, 0 otherwise.
#[derive(Pun, Debug, Copy, Clone)]
#[repr(transparent)]
#[pun(assert(self.0 >> 4 >= 1 && self.0 >> 4 <= 14))]
#[pun(assert({
  let kind = self.0 >> 4;
  let flags = self.0 & 15;
  (kind == 3 && flags & 6 != 6)
    || ((kind == 6 || kind == 8 || kind == 10) && flags == 2)
    || (kind != 3 && kind != 6 && kind != 8 && kind != 10 && flags == 0)
}))]
#[pun(get(kind: u8 = self.0 >> 4, flags: u8 = self.0 & 15))]
pub struct Fixed(pub u8);

/// A packet whose remaining length is 1 byte.
///
/// The remaining length is a variable-length integer of 1 to 4 bytes, 7 bits each, least
/// significant first: a different head for each of its lengths, so four structs ([`Packet1`] to
/// [`Packet4`]) and an enum of references ([`Packet`]). This is the current way: with `be` in
/// `size` (planned), it is one struct.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct Packet1<P: ?Sized = [u8]> {
  pub header: Fixed,
  #[pun(valid(..=0x7f))]
  pub length: u8,
  pub body: P,
}

/// A packet whose remaining length is 2 bytes (the last one is not 0: the shortest form).
#[derive(Pun)]
#[repr(C)]
#[pun(size = (l0 & 0x7f) + (l1 << 7) + 3)]
pub struct Packet2<P: ?Sized = [u8]> {
  pub header: Fixed,
  #[pun(valid(0x80..))]
  pub l0: u8,
  #[pun(valid(1..=0x7f))]
  pub l1: u8,
  pub body: P,
}

/// A packet whose remaining length is 3 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (l0 & 0x7f) + ((l1 & 0x7f) << 7) + (l2 << 14) + 4)]
pub struct Packet3<P: ?Sized = [u8]> {
  pub header: Fixed,
  #[pun(valid(0x80..))]
  pub l0: u8,
  #[pun(valid(0x80..))]
  pub l1: u8,
  #[pun(valid(1..=0x7f))]
  pub l2: u8,
  pub body: P,
}

/// A packet whose remaining length is 4 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (l0 & 0x7f) + ((l1 & 0x7f) << 7) + ((l2 & 0x7f) << 14) + (l3 << 21) + 5)]
pub struct Packet4<P: ?Sized = [u8]> {
  pub header: Fixed,
  #[pun(valid(0x80..))]
  pub l0: u8,
  #[pun(valid(0x80..))]
  pub l1: u8,
  #[pun(valid(0x80..))]
  pub l2: u8,
  #[pun(valid(1..=0x7f))]
  pub l3: u8,
  pub body: P,
}

/// Any MQTT packet.
#[derive(Pun)]
pub enum Packet<'a> {
  One(&'a Packet1),
  Two(&'a Packet2),
  Three(&'a Packet3),
  Four(&'a Packet4),
}

/// A UTF-8 string of MQTT (a 16 bits length, then UTF-8), then what follows it. (MQTT also
/// forbids U+0000, which a `str` allows: no other text pattern can be declared yet.)
#[derive(Pun)]
#[repr(C)]
pub struct Utf8<R: ?Sized = [u8; 0]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(text: str = length, rest: R))]
  pub data: [u8],
}

/// The body of CONNECT: the protocol name "MQTT", level 4, flags, a keep alive, then the payload:
/// a client identifier, then what the flags tell (will, user name, password).
#[derive(Pun)]
#[repr(C)]
#[pun(assert(flags & 1 == 0))]
// Will QoS and will retain only with the will flag; QoS 3 is not one; a password needs a user.
#[pun(assert(flags & 4 != 0 || flags & 0x38 == 0))]
#[pun(assert(flags & 0x18 != 0x18))]
#[pun(assert(flags & 0x80 != 0 || flags & 0x40 == 0))]
#[pun(get(clean_session: bool = flags & 2 != 0, has_user: bool = flags & 0x80 != 0))]
pub struct Connect {
  pub name_length: u16bec<4>,
  pub name: u32bec<0x4d51_5454>,
  pub level: u8c<4>,
  pub flags: u8,
  pub keep_alive: u16be,
  pub client_id: Utf8<[u8]>,
}

// -------------------------------------------------------------------------------------------------
// CoAP

/// A CoAP message (RFC 7252): a header of 4 bytes, a token of 0 to 8 bytes, then options and a
/// payload.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(version_type_tkl >> 6 == 1))]
#[pun(assert(version_type_tkl & 15 <= 8))]
// Code classes 0 (request), 2, 4, 5 (responses) and 7 (signaling); an empty message (code 0.00)
// is only its header.
#[pun(assert({
  let class = code >> 5;
  class == 0 || class == 2 || class == 4 || class == 5 || class == 7
}))]
#[pun(assert(code != 0 || size_of_val(self) == 4))]
#[pun(get(kind: u8 = (version_type_tkl >> 4) & 3, class: u8 = code >> 5, detail: u8 = code & 31))]
pub struct Coap {
  /// Version (2 bits, 1), type (2 bits), token length (4 bits).
  pub version_type_tkl: u8,
  pub code: u8,
  pub message_id: u16be,
  #[pun(regions(token = version_type_tkl & 15, options_and_payload))]
  pub data: [u8],
}

/// An option whose delta and length are both below 13: in the first byte.
///
/// Each nibble of the first byte is the value (0 to 12), or tells 1 (13) or 2 (14) more bytes:
/// nine forms of head, a struct each, and an enum of references ([`CoapOption`]). This is the
/// current way: with `be` in `size` (planned), it is one struct.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (head & 15) + 1)]
#[pun(assert(head >> 4 < 13 && head & 15 < 13))]
pub struct Option0 {
  pub head: u8,
  pub value: [u8],
}

/// A delta of 1 more byte (13 to 268), a length below 13.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (head & 15) + 2)]
#[pun(assert(head >> 4 == 13 && head & 15 < 13))]
pub struct OptionD8 {
  pub head: u8,
  pub delta: u8,
  pub value: [u8],
}

/// A delta of 2 more bytes (269 to 65804), a length below 13.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (head & 15) + 3)]
#[pun(assert(head >> 4 == 14 && head & 15 < 13))]
pub struct OptionD16 {
  pub head: u8,
  pub delta: u16be,
  pub value: [u8],
}

/// A delta below 13, a length of 1 more byte.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 15)]
#[pun(assert(head >> 4 < 13 && head & 15 == 13))]
pub struct OptionL8 {
  pub head: u8,
  pub length: u8,
  pub value: [u8],
}

/// A delta below 13, a length of 2 more bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 272)]
#[pun(assert(head >> 4 < 13 && head & 15 == 14))]
pub struct OptionL16 {
  pub head: u8,
  pub length: u16be,
  pub value: [u8],
}

/// A delta and a length of 1 more byte each.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 16)]
pub struct OptionD8L8 {
  pub head: u8c<0xdd>,
  pub delta: u8,
  pub length: u8,
  pub value: [u8],
}

/// A delta of 1 more byte, a length of 2.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 273)]
pub struct OptionD8L16 {
  pub head: u8c<0xde>,
  pub delta: u8,
  pub length: u16be,
  pub value: [u8],
}

/// A delta of 2 more bytes, a length of 1.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 17)]
pub struct OptionD16L8 {
  pub head: u8c<0xed>,
  pub delta: u16be,
  pub length: u8,
  pub value: [u8],
}

/// A delta and a length of 2 more bytes each.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 274)]
pub struct OptionD16L16 {
  pub head: u8c<0xee>,
  pub delta: u16be,
  pub length: u16be,
  pub value: [u8],
}

/// A CoAP option, of any of the nine forms. (The nibble 15 is the payload marker, or reserved.)
#[derive(Pun)]
pub enum CoapOption<'a> {
  Small(&'a Option0),
  D8(&'a OptionD8),
  D16(&'a OptionD16),
  L8(&'a OptionL8),
  L16(&'a OptionL16),
  D8L8(&'a OptionD8L8),
  D8L16(&'a OptionD8L16),
  D16L8(&'a OptionD16L8),
  D16L16(&'a OptionD16L16),
}

// -------------------------------------------------------------------------------------------------
// SCTP

/// An SCTP packet: a common header, then chunks. Its checksum is a CRC-32C of the packet, its
/// own field taken as 0, which is not checked (see the top of this file).
#[derive(Pun)]
#[repr(C)]
pub struct Sctp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub verification_tag: u32be,
  /// The CRC-32C, least significant byte first.
  pub checksum: u32le,
  pub chunks: [u8],
}

/// An SCTP chunk: its length does not count the padding to 4 bytes that follows it.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (length + 3) / 4 * 4)]
#[pun(assert(length >= 4))]
pub struct Chunk {
  pub kind: u8,
  pub flags: u8,
  pub length: u16be,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// Use

/// A TLS record, as bytes: a ClientHello of three cipher suites and a server name.
const CLIENT_HELLO: [u8; 108] = [
  0x16, 0x03, 0x01, 0x00, 0x67, 0x01, 0x00, 0x00, 0x63, 0x03, 0x03, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x20, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0x00, 0x06, 0x13, 0x01,
  0x13, 0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x10, 0x00, 0x0e, 0x00, 0x00,
  0x0b, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm',
];

/// The same, of 3 bytes of cipher suites: still a chain that fills the message.
const ODD_HELLO: [u8; 105] = [
  0x16, 0x03, 0x01, 0x00, 0x64, 0x01, 0x00, 0x00, 0x60, 0x03, 0x03, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x20, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0x00, 0x03, 0x13, 0x01,
  0x13, 0x01, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x10, 0x00, 0x0e, 0x00, 0x00, 0x0b, b'e', b'x',
  b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm',
];

/// A QUIC Initial packet, as bytes: a destination connection ID of 8 bytes, no source connection
/// ID, no token, a length of 2 bytes (`0x4020`: 32), 32 protected bytes, then 3 bytes of another
/// packet.
const INITIAL: [u8; 53] = [
  0xc3, 0, 0, 0, 1, 8, 0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08, 0, 0, 0x40, 0x20, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 1, 2,
  3,
];

/// HTTP/2 SETTINGS, as bytes: at most 100 concurrent streams, an initial window of 65535; then a
/// SETTINGS ACK.
const SETTINGS: [u8; 30] = [
  0, 0, 12, 4, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 100, 0, 4, 0, 0, 0xff, 0xff, 0, 0, 0, 4, 1, 0, 0, 0, 0,
];

/// A padded HTTP/2 DATA frame on stream 1, as bytes: a pad length of 3, "hello", 3 bytes of
/// padding, `END_STREAM`.
const DATA: [u8; 18] = [0, 0, 9, 0, 9, 0, 0, 0, 1, 3, b'h', b'e', b'l', b'l', b'o', 0, 0, 0];

/// An HTTP/2 HEADERS frame on stream 3, as bytes: PRIORITY and PADDED, two bytes of header block.
const HEADERS: [u8; 18] = [0, 0, 9, 1, 0x2c, 0, 0, 0, 3, 1, 0, 0, 0, 0, 15, 0x82, 0x86, 0];

/// The examples of RFC 6455, 5.7: "Hello" from a server, the same from a client (masked), a ping.
const HELLO: [u8; 7] = [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
const MASKED_HELLO: [u8; 11] = [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
const PING: [u8; 7] = [0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];

/// An SSH binary packet, as bytes: a KEXINIT, padding, a MAC of 32 bytes.
const KEXINIT: [u8; 192] = [
  0x00, 0x00, 0x00, 0x9c, 0x0b, 0x14, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
  0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x00, 0x00, 0x00, 0x11, b'c', b'u', b'r', b'v', b'e', b'2',
  b'5', b'5', b'1', b'9', b'-', b's', b'h', b'a', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x0b, b's',
  b's', b'h', b'-', b'e', b'd', b'2', b'5', b'5', b'1', b'9', 0x00, 0x00, 0x00, 0x0a, b'a', b'e',
  b's', b'1', b'2', b'8', b'-', b'c', b't', b'r', 0x00, 0x00, 0x00, 0x0a, b'a', b'e', b's', b'1',
  b'2', b'8', b'-', b'c', b't', b'r', 0x00, 0x00, 0x00, 0x0d, b'h', b'm', b'a', b'c', b'-', b's',
  b'h', b'a', b'2', b'-', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x0d, b'h', b'm', b'a', b'c', b'-',
  b's', b'h', b'a', b'2', b'-', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x04, b'n', b'o', b'n', b'e',
  0x00, 0x00, 0x00, 0x04, b'n', b'o', b'n', b'e', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
];

/// MQTT CONNECT, as bytes: client identifier "abc", a clean session, a keep alive of 60 seconds.
const CONNECT: [u8; 17] =
  [0x10, 15, 0, 4, b'M', b'Q', b'T', b'T', 4, 2, 0, 60, 0, 3, b'a', b'b', b'c'];

/// A confirmable CoAP GET, as bytes: message ID `0x7d34`, token `0x71`, Uri-Path "temperature"
/// (option 11).
const GET: [u8; 17] = [
  0x41, 0x01, 0x7d, 0x34, 0x71, 0xbb, b't', b'e', b'm', b'p', b'e', b'r', b'a', b't', b'u', b'r',
  b'e',
];

/// Its response, 2.05 Content, as bytes: Max-Age (option 14, delta 14 in a byte of extension:
/// "13 + 1"), the payload marker, a payload.
const CONTENT: [u8; 16] =
  [0x61, 0x45, 0x7d, 0x34, 0x71, 0xd1, 0x01, 0x3c, 0xff, b'2', b'2', b'.', b'3', b' ', b'C', 0];

/// An SCTP packet, as bytes: an INIT chunk, and the CRC-32C of the packet.
const SCTP: [u8; 32] = [
  0x13, 0x88, 0x13, 0x88, 0x00, 0x00, 0x00, 0x00, 0x94, 0x09, 0x49, 0x12, 0x01, 0x00, 0x00, 0x14,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x01,
];

pub fn main() {
  // ---- TLS: a record, a handshake message, a ClientHello: a chain of vectors.
  let record: &Record<Handshake<ClientHello>> = try_pun_ref(&CLIENT_HELLO[..]).unwrap();
  assert_eq!(record.version.get(), 0x0301);
  let hello = &record.fragment.body;
  assert_eq!((hello.legacy_version.get(), hello.random), (0x0303, [0x5a; 32]));
  let session = &hello.session;
  assert_eq!(session.items(), [0xe0; 32]);
  let suites = session.rest().unwrap();
  let suites_list: Vec<u16> = suites.suites().unwrap().iter().map(|suite| suite.get()).collect();
  assert_eq!(suites_list, [0x1301, 0x1302, 0xc02f]);
  let compression = suites.rest().unwrap();
  assert_eq!(compression.items(), [0]);
  let extensions = compression.rest().unwrap();
  assert!(extensions.rest().is_ok());
  let (extension, rest) = from_head::<TlsExtension>(extensions.items()).unwrap();
  assert_eq!((extension.kind.get(), rest.len()), (0, 0));
  let server_name: &ServerName = try_pun_ref(&extension.data).unwrap();
  assert_eq!(&server_name.name, "example.com");
  // It is any record and any handshake message, too.
  let any: &Record = try_pun_ref(&CLIENT_HELLO[..]).unwrap();
  assert_eq!(any.content_type, ContentType(22));
  let (handshake, rest) = from_head::<Handshake>(&any.fragment).unwrap();
  assert_eq!((handshake.msg_type, rest.len()), (HandshakeType(1), 0));

  // ---- TLS: what is not a ClientHello.
  // Application data, not a content type, a version, a ServerHello, the lengths of the
  // handshake and of the record, a session ID longer than it is (the chain then does not fill
  // the message), the length of the cipher suites.
  for (at, value) in [(0, 23), (0, 19), (2, 5), (5, 2), (8, 0), (4, 0), (43, 33), (77, 8)] {
    let mut bytes = CLIENT_HELLO;
    bytes[at] = value;
    assert_eq!(
      try_pun_ref::<_, Record<Handshake<ClientHello>>>(&bytes[..]).err(),
      Some(Error::Validity)
    );
  }
  // An odd number of bytes of cipher suites, in a chain that still fills the message: the
  // message is valid, its typed region is not, when read.
  let odd: &Record<Handshake<ClientHello>> = try_pun_ref(&ODD_HELLO[..]).unwrap();
  assert_eq!(odd.fragment.body.session.rest().err(), Some(Error::Validity));
  // A server name that is not UTF-8.
  let mut bytes = CLIENT_HELLO;
  bytes[107] = 0xff;
  let record: &Record<Handshake<ClientHello>> = try_pun_ref(&bytes[..]).unwrap();
  let session = &record.fragment.body.session;
  let extensions = session.rest().unwrap().rest().unwrap().rest().unwrap();
  let (extension, _) = from_head::<TlsExtension>(extensions.items()).unwrap();
  assert_eq!(try_pun_ref::<_, ServerName>(&extension.data).err(), Some(Error::Validity));
  // A record too long for TLS.
  let mut long = vec![23u8, 3, 3, 0x48, 0x01];
  long.resize(5 + 0x4801, 0);
  assert_eq!(try_pun_ref::<_, Record>(&long[..]).err(), Some(Error::Validity));

  // ---- QUIC: connection IDs, then variable-length integers.
  let packet: &Initial = try_pun_ref(&INITIAL[..]).unwrap();
  assert_eq!(packet.dcid(), [0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08]);
  assert_eq!((packet.scid().len(), packet.token_length(), packet.token().len()), (0, &[0][..], 0));
  assert_eq!((packet.length(), packet.protected().len()), (&[0x40, 0x20][..], 32));
  assert_eq!(packet.coalesced(), [1, 2, 3]);
  let header: &LongHeader = try_pun_ref(&INITIAL[..]).unwrap();
  assert_eq!((header.packet_type(), header.version.get(), header.rest().len()), (0, 1, 38));
  // A token of 3 bytes; a length of 8 bytes (the shortest form is not required).
  let token = [&INITIAL[..15], &[3, 7, 7, 7], &INITIAL[16..]].concat();
  let packet: &Initial = try_pun_ref(&token[..]).unwrap();
  assert_eq!((packet.token(), packet.protected().len()), (&[7, 7, 7][..], 32));
  let wide = [&INITIAL[..16], &[0xc0, 0, 0, 0, 0, 0, 0, 0x20], &INITIAL[18..]].concat();
  let packet: &Initial = try_pun_ref(&wide[..]).unwrap();
  assert_eq!(
    (packet.length().len(), packet.protected().len(), packet.coalesced().len()),
    (8, 32, 3)
  );
  // A short header; a 0-RTT packet; version 2; connection IDs longer than 20 bytes; too short
  // for a packet number and a tag; a length beyond the packet.
  for (at, value) in [(0, 0x43), (0, 0xd3), (4, 2), (5, 21), (14, 21), (17, 0x10), (17, 0x30)] {
    let mut bytes = INITIAL;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Initial>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- HTTP/2: the preface, then frames of typed payloads.
  let preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
  assert!(try_pun_ref::<_, Preface>(&preface[..]).is_ok());
  let mut wrong = *preface;
  wrong[4] = b'X';
  assert_eq!(try_pun_ref::<_, Preface>(&wrong[..]).err(), Some(Error::Validity));
  let (frame, rest) = from_head::<Frame<Settings>>(&SETTINGS).unwrap();
  let settings: Vec<_> = frame.payload.0.iter().map(|s| (s.id.get(), s.value.get())).collect();
  assert_eq!((settings, frame.stream_id()), (vec![(3, 100), (4, 65535)], 0));
  let (ack, rest) = from_head::<SettingsFrame>(rest).unwrap();
  assert_eq!((ack.flags, ack.settings.len(), rest.len()), (1, 0, 0));
  assert!(from_head::<SettingsFrame>(&SETTINGS).is_ok());
  // A length that is not a multiple of 6; on a stream; an ACK that is not empty; not SETTINGS,
  // which is still a frame.
  let mut odd = SETTINGS;
  odd[2] = 11;
  assert!(from_head::<Frame<Settings>>(&odd).is_err());
  for at in [8, 4] {
    let mut bytes = SETTINGS;
    bytes[at] = 1;
    assert_eq!(from_head::<SettingsFrame>(&bytes).err(), Some(Error::Validity));
  }
  let mut other = SETTINGS;
  other[3] = 5;
  assert_eq!(from_head::<Frame<Settings>>(&other).err(), Some(Error::Validity));
  assert_eq!(from_head::<Frame>(&other).unwrap().0.kind, FrameType(5));
  // DATA, padded and not.
  let (data, rest) = from_head::<DataFrame>(&DATA).unwrap();
  assert_eq!(
    (data.data(), data.padding(), data.pad_length(), rest.len()),
    (&b"hello"[..], &[0; 3][..], &[3][..], 0)
  );
  assert!(data.end_stream() && data.padded());
  let plain = [0, 0, 5, 0, 0, 0, 0, 0, 1, b'h', b'e', b'l', b'l', b'o'];
  let (data, _) = from_head::<DataFrame>(&plain).unwrap();
  assert_eq!((data.data(), data.padding().len(), data.pad_length().len()), (&b"hello"[..], 0, 0));
  // Padding longer than the frame; DATA on stream 0.
  for (at, value) in [(9, 9), (8, 0)] {
    let mut bytes = DATA;
    bytes[at] = value;
    assert_eq!(from_head::<DataFrame>(&bytes).err(), Some(Error::Validity));
  }
  // HEADERS, with a priority and padding.
  let (headers, _) = from_head::<HeadersFrame>(&HEADERS).unwrap();
  assert_eq!(
    (headers.priority().len(), headers.block(), headers.padding()),
    (5, &[0x82, 0x86][..], &[0][..])
  );

  // ---- WebSocket: a form per length, a view per side.
  let Ok((ServerFrame::Short(frame), rest)) = ServerFrame::from_head(&HELLO) else { panic!() };
  assert_eq!(
    (frame.head.fin(), frame.head.opcode(), &frame.payload, rest.len()),
    (true, 1, &b"Hello"[..], 0)
  );
  assert!(
    matches!(ServerFrame::from_bytes(&PING), Ok(ServerFrame::Short(ping)) if ping.head.opcode() == 9)
  );
  // What a frame carries, whatever the form of its length.
  let payload = |frame: ServerFrame<'_>| match frame {
    ServerFrame::Short(frame) => frame.payload.len(),
    ServerFrame::Medium(frame) => frame.payload.len(),
    ServerFrame::Long(frame) => frame.payload.len(),
  };
  // Unmasking is a transformation: use code.
  let unmask = |frame: ClientFrame<'_>| {
    let (key, payload) = match frame {
      ClientFrame::Short(frame) => (frame.key, &frame.payload),
      ClientFrame::Medium(frame) => (frame.key, &frame.payload),
      ClientFrame::Long(frame) => (frame.key, &frame.payload),
    };
    payload.iter().enumerate().map(|(i, byte)| byte ^ key[i % 4]).collect::<Vec<u8>>()
  };
  assert_eq!(unmask(ClientFrame::from_bytes(&MASKED_HELLO).unwrap()), b"Hello");
  // 256 bytes, then 65536 bytes, not masked.
  let mut medium = vec![0x82, 0x7e, 0x01, 0x00];
  medium.resize(4 + 256, 7);
  assert!(matches!(ServerFrame::from_bytes(&medium), Ok(ServerFrame::Medium(_))));
  assert_eq!(payload(ServerFrame::from_bytes(&medium).unwrap()), 256);
  let mut long = vec![0x82, 0x7f, 0, 0, 0, 0, 0, 1, 0, 0];
  long.resize(10 + 65536, 7);
  assert!(matches!(ServerFrame::from_bytes(&long), Ok(ServerFrame::Long(_))));
  assert_eq!(payload(ServerFrame::from_bytes(&long).unwrap()), 65536);
  // 256 bytes from a client, masked by a key of zeros.
  let mut masked = vec![0x82, 0xfe, 0x01, 0x00, 0, 0, 0, 0];
  masked.resize(8 + 256, 7);
  assert_eq!(unmask(ClientFrame::from_bytes(&masked).unwrap()), [7; 256]);
  // Masked from a server, not masked from a client.
  assert_eq!(ServerFrame::from_bytes(&MASKED_HELLO).err(), Some(Error::Validity));
  assert_eq!(ClientFrame::from_bytes(&HELLO).err(), Some(Error::Validity));
  // RSV bits, a reserved opcode, a fragmented control frame; a length that is not the one of
  // the bytes.
  for value in [0xc1, 0x83, 0x09] {
    let mut bytes = HELLO;
    bytes[0] = value;
    assert_eq!(ServerFrame::from_bytes(&bytes).err(), Some(Error::Validity));
  }
  assert!(ServerFrame::from_bytes(&HELLO[..6]).is_err());
  // Not the shortest form; a control frame of a long form; a length whose first bit is set.
  medium[2] = 0;
  medium[3] = 0x7d;
  medium.truncate(4 + 125);
  assert!(ServerFrame::from_bytes(&medium).is_err());
  let mut ping = vec![0x89, 0x7e, 0x01, 0x00];
  ping.resize(4 + 256, 7);
  assert!(ServerFrame::from_bytes(&ping).is_err());
  long[2] = 0x80;
  assert!(ServerFrame::from_bytes(&long).is_err());

  // ---- SSH: a packet of a block size and a MAC size that the bytes do not tell.
  let (packet, rest) = from_head::<SshPacket<8, 32>>(&KEXINIT).unwrap();
  assert_eq!(
    (packet.payload().len(), packet.padding().len(), packet.mac(), rest.len()),
    (144, 11, &[0xaa; 32][..], 0)
  );
  let kex: &KexInit = try_pun_ref(packet.payload()).unwrap();
  assert_eq!(
    kex.lists.names().unwrap().iter().map(|c| c.get()).collect::<Vec<_>>(),
    b"curve25519-sha256"
  );
  // Ten levels down, each list checked when it is read.
  let lists = &kex.lists;
  let fifth = lists.rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap();
  assert_eq!(fifth.names().unwrap().len(), "hmac-sha2-256".len());
  let tenth = fifth.rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap();
  assert_eq!(tenth.names().unwrap().len(), 0);
  let end = tenth.rest().unwrap();
  assert!(!end.first_kex_packet_follows);
  // Not a multiple of the block size; padding under 4 bytes; another MAC size.
  let mut bytes = KEXINIT;
  bytes[3] += 1;
  assert!(from_head::<SshPacket<8, 32>>(&bytes).is_err());
  let mut bytes = KEXINIT;
  bytes[4] = 3;
  assert_eq!(from_head::<SshPacket<8, 32>>(&bytes).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, SshPacket<8, 20>>(&KEXINIT[..]).err(), Some(Error::Validity));
  // A name with a space in it: the typed region is checked when it is read.
  let mut bytes = KEXINIT;
  bytes[5 + 21 + 3] = b' ';
  let (packet, _) = from_head::<SshPacket<8, 32>>(&bytes).unwrap();
  let kex: &KexInit = try_pun_ref(packet.payload()).unwrap();
  assert_eq!(kex.lists.names().err(), Some(Error::Validity));

  // ---- MQTT: a packet of one length form, of a typed body.
  let connect: &Packet1<Connect> = try_pun_ref(&CONNECT[..]).unwrap();
  assert_eq!((connect.header.kind(), connect.body.keep_alive.get()), (1, 60));
  assert!(connect.body.clean_session() && !connect.body.has_user());
  assert_eq!(connect.body.client_id.text(), Ok("abc"));
  let Ok((Packet::One(any), rest)) = Packet::from_head(&CONNECT) else { panic!() };
  assert_eq!((any.body.len(), rest.len()), (15, 0));
  // Flags of CONNECT, a reserved type, type 15; the length; the protocol name and level; a
  // reserved flag, a will QoS without a will, will QoS 3.
  for (at, value) in [
    (0, 0x12),
    (0, 0x00),
    (0, 0xf0),
    (1, 14),
    (4, b'm'),
    (8, 3),
    (9, 3),
    (9, 0x0a),
    (9, 0x1c),
    (9, 0x42),
  ] {
    let mut bytes = CONNECT;
    bytes[at] = value;
    assert!(try_pun_ref::<_, Packet1<Connect>>(&bytes[..]).is_err());
  }
  // A client identifier that is not UTF-8: checked when read.
  let mut bytes = CONNECT;
  bytes[16] = 0xff;
  let connect: &Packet1<Connect> = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!(connect.body.client_id.text(), Err(Error::Validity));
  // PUBLISH of QoS 1 and 200 bytes (two bytes of length), then DISCONNECT.
  let mut publish = vec![0x32, 0xc8, 0x01];
  publish.resize(3 + 200, 0);
  publish.extend([0xe0, 0x00]);
  // What a packet carries, whatever the form of its length.
  let body = |packet: Packet<'_>| match packet {
    Packet::One(packet) => packet.body.len(),
    Packet::Two(packet) => packet.body.len(),
    Packet::Three(packet) => packet.body.len(),
    Packet::Four(packet) => packet.body.len(),
  };
  assert_eq!(body(Packet::from_bytes(&CONNECT).unwrap()), 15);
  let Ok((Packet::Two(packet), rest)) = Packet::from_head(&publish) else { panic!() };
  assert_eq!((packet.header.flags(), packet.body.len(), rest), (2, 200, &[0xe0, 0][..]));
  let Ok((Packet::One(disconnect), _)) = Packet::from_head(rest) else { panic!() };
  assert_eq!(disconnect.header.kind(), 14);
  // 16384 bytes: three bytes of length.
  let mut big = vec![0x30, 0x80, 0x80, 0x01];
  big.resize(4 + 16384, 0);
  assert!(matches!(Packet::from_bytes(&big), Ok(Packet::Three(_))));
  assert_eq!(body(Packet::from_bytes(&big).unwrap()), 16384);
  // Not the shortest form; PUBLISH of QoS 3; SUBSCRIBE without its flags; a fifth byte of length.
  for bytes in
    [&[0x30, 0x80, 0x00][..], &[0x36, 0x00], &[0x80, 0x00], &[0x30, 0xff, 0xff, 0xff, 0xff, 0x01]]
  {
    assert_eq!(Packet::from_bytes(bytes).err(), Some(Error::Validity));
  }
  assert!(Packet::from_bytes(&[0x82, 0x00]).is_ok());

  // ---- CoAP: a header, a token, then options of their own forms.
  let get: &Coap = try_pun_ref(&GET[..]).unwrap();
  assert_eq!((get.kind(), get.class(), get.detail(), get.token()), (0, 0, 1, &[0x71][..]));
  let Ok((CoapOption::Small(option), rest)) = CoapOption::from_head(get.options_and_payload())
  else {
    panic!()
  };
  assert_eq!((option.head >> 4, &option.value, rest.len()), (11, &b"temperature"[..], 0));
  let content: &Coap = try_pun_ref(&CONTENT[..]).unwrap();
  assert_eq!((content.class(), content.detail()), (2, 5));
  let Ok((CoapOption::D8(option), rest)) = CoapOption::from_head(content.options_and_payload())
  else {
    panic!()
  };
  assert_eq!(
    (option.delta + 13, &option.value, rest[0], &rest[1..]),
    (14, &[0x3c][..], 0xff, &b"22.3 C\0"[..])
  );
  // Options of the forms of 1 and 2 more bytes of length and delta.
  let mut long = vec![0x1d, 7];
  long.resize(2 + 20, b'x');
  // The size of the value of an option, whatever its form.
  let size = |option: CoapOption<'_>| match option {
    CoapOption::Small(option) => option.value.len(),
    CoapOption::D8(option) => option.value.len(),
    CoapOption::D16(option) => option.value.len(),
    CoapOption::L8(option) => option.value.len(),
    CoapOption::L16(option) => option.value.len(),
    CoapOption::D8L8(option) => option.value.len(),
    CoapOption::D8L16(option) => option.value.len(),
    CoapOption::D16L8(option) => option.value.len(),
    CoapOption::D16L16(option) => option.value.len(),
  };
  assert!(matches!(CoapOption::from_bytes(&long), Ok(CoapOption::L8(_))));
  assert_eq!(size(CoapOption::from_bytes(&long).unwrap()), 20);
  let mut longer = vec![0xee, 0x01, 0x00, 0x00, 0x10];
  longer.resize(5 + 269 + 16, b'x');
  assert!(matches!(CoapOption::from_bytes(&longer), Ok(CoapOption::D16L16(_))));
  assert_eq!(size(CoapOption::from_bytes(&longer).unwrap()), 285);
  // The payload marker, and a reserved nibble, are no option; a length past the end.
  for bytes in [&[0xff][..], &[0xf1, 0], &[0x1f], &[0x13, 1, 2]] {
    assert!(CoapOption::from_bytes(bytes).is_err());
  }
  // Version 2; a token of 9 bytes; class 1; an empty message with a token.
  for (at, value) in [(0, 0x81), (0, 0x49), (1, 0x21), (1, 0x00)] {
    let mut bytes = GET;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Coap>(&bytes[..]).err(), Some(Error::Validity));
  }
  // An option that runs past the message is seen by reading the options (use code), not by the
  // message: its walk over the options is longer than allowed (see the top of this file).
  let mut bytes = GET;
  bytes[5] = 0xbc;
  let get: &Coap = try_pun_ref(&bytes[..]).unwrap();
  assert!(CoapOption::from_head(get.options_and_payload()).is_err());

  // ---- SCTP: chunks padded to 4 bytes; the checksum is there, but not checked.
  let sctp: &Sctp = try_pun_ref(&SCTP[..]).unwrap();
  assert_eq!((sctp.source_port.get(), sctp.checksum.get()), (5000, 0x1249_0994));
  let (chunk, rest) = from_head::<Chunk>(&sctp.chunks).unwrap();
  assert_eq!((chunk.kind, chunk.value.len(), rest.len()), (1, 16, 0));
  let (chunk, rest) = from_head::<Chunk>(&[0, 3, 0, 5, 9, 0, 0, 0, 0xee]).unwrap();
  assert_eq!((chunk.value.len(), rest), (4, &[0xee][..]));
  assert_eq!(from_head::<Chunk>(&[0, 3, 0, 3, 9, 0, 0, 0]).err(), Some(Error::Validity));
  let mut bytes = SCTP;
  bytes[20] ^= 1;
  assert!(try_pun_ref::<_, Sctp>(&bytes[..]).is_ok());

  println!("ok");
}
