//! Ethernet, in safe Rust, with nothing but `#[derive(Data)]` and its options.
//!
//! The first half of this file is all there is to Ethernet here: type declarations. There is no
//! `unsafe`, no `impl`, no parsing code and no size computation. The second half uses them:
//! frames are read in place, from bytes, by `unpack` and `try_transmute_ref`.
//!
//! What is specific to Ethernet, and what tells it:
//!
//! | Specificity                                          | Told by                                |
//! |------------------------------------------------------|----------------------------------------|
//! | big-endian fields                                    | `u16be`                                |
//! | the EtherType tells the type of the payload          | `tag(EtherType = u16bec<0x0800>)`      |
//! | an EtherType is at least `0x0600`                    | `valid(0x0600..)`                      |
//! | below `1500`, it is the length of the payload        | `valid(..=1500)`, `regions(..)`        |
//! | frames are 60 to 1514 bytes, padded if needed        | `assert(size_of_val(self) >= 60 ..)`   |
//! | the frame check sequence: 4 last bytes, a CRC-32     | `regions(..)`, `assert(crc32(..) ..)`  |
//! | the preamble and the start frame delimiter           | `[u8c<0x55>; 7]`, `u8c<0xd5>`          |
//! | LLC and SNAP headers                                 | `u8c<0xaa>`                            |
//! | 802.1Q tags, nested or not                           | a payload that has a payload           |
//! | bit fields (VLAN identifier, IPv4 header length)     | `get(..)`, `assert(..)`, `valid(..)`   |
//! | the size of an IPv4 packet is in its header          | `size = total_length`                  |
//! | IPv4 options, whose size is in the header too        | `regions(options = .., payload)`       |
//! | the checksum of the IPv4 header                      | `assert(sum16(..) == 0xffff)`          |
//! | a frame carries more bytes than its packet (padding) | `unpack` returns what remains          |
#![deny(unsafe_code)]

use mutant::num::{u8c, u16be, u16bec};
use mutant::{Data, Error, Tag, as_bytes, transmute_ref, try_transmute_ref, unpack};

// -------------------------------------------------------------------------------------------------
// Ethernet

/// A MAC address.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Mac(pub [u8; 6]);

/// An EtherType, when what it tells is not known: values up to 1500 are not one, but a length
/// (see [`Frame8023`]).
///
/// A payload tells the EtherType it comes with by `#[data(tag(EtherType = ..))]`.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct EtherType(#[data(valid(0x0600..))] pub u16be);

/// An Ethernet II frame, without preamble nor frame check sequence. Its payload is bytes, of any
/// EtherType, unless told: a `Frame<Ipv4>` is a frame whose EtherType is the one of IPv4.
#[derive(Data)]
#[repr(C)]
pub struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: P::Tag,
  pub payload: P,
}

/// A frame as it is on the wire: 60 to 1514 bytes, padded if needed.
#[derive(Data)]
#[repr(C)]
#[data(assert(size_of_val(self) >= 60 && size_of_val(self) <= 1514))]
pub struct WireFrame {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: EtherType,
  pub payload: [u8],
}

/// A frame with its frame check sequence: its 4 last bytes, which are the CRC-32 of what is
/// before them, least significant byte first.
#[derive(Data)]
#[repr(C)]
#[data(assert(size_of_val(self) >= 64 && size_of_val(self) <= 1518))]
#[data(assert(crc32(0, size_of_val(self) - 4) == le(size_of_val(self) - 4, 4)))]
pub struct CheckedFrame {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: EtherType,
  #[data(regions(payload = size_of_val(self) - 18, fcs))]
  pub data: [u8],
}

/// A frame as it is on the medium: a preamble, a start frame delimiter, the frame and its
/// check sequence.
#[derive(Data)]
#[repr(C)]
pub struct Packet {
  pub preamble: [u8c<0x55>; 7],
  pub start: u8c<0xd5>,
  pub frame: CheckedFrame,
}

/// An IEEE 802.3 frame: no EtherType, but the length of the payload, which is then padded.
#[derive(Data)]
#[repr(C)]
pub struct Frame8023 {
  pub destination: Mac,
  pub source: Mac,
  #[data(valid(..=1500))]
  pub length: u16be,
  #[data(regions(payload = length, padding))]
  pub data: [u8],
}

/// LLC and SNAP headers: an EtherType in an 802.3 frame.
#[derive(Data)]
#[repr(C)]
pub struct Snap<P: ?Sized + Tag<EtherType> = [u8]> {
  pub dsap: u8c<0xaa>,
  pub ssap: u8c<0xaa>,
  pub control: u8c<0x03>,
  pub organization: [u8; 3],
  pub ether_type: P::Tag,
  pub payload: P,
}

/// An IEEE 802.1Q tag, and what follows it: a payload that has a payload.
#[derive(Data)]
#[repr(C)]
#[data(tag(EtherType = u16bec<0x8100>))]
#[data(get(priority: u8 = tci >> 13, drop_eligible: bool = tci & 0x1000 != 0, id: u16 = tci & 0x0fff))]
#[data(assert((tci & 0x0fff) != 0x0fff))]
pub struct Vlan<P: ?Sized + Tag<EtherType> = [u8]> {
  /// Priority (3 bits), drop eligible (1 bit), VLAN identifier (12 bits, `0xfff` is reserved).
  pub tci: u16be,
  pub ether_type: P::Tag,
  pub payload: P,
}

// -------------------------------------------------------------------------------------------------
// ARP

#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum Operation {
  // The values an `u16` has in memory when it is big-endian on the wire.
  Request = 1u16.to_be(),
  Reply = 2u16.to_be(),
}

/// ARP, for IPv4 over Ethernet.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C, packed)]
#[data(tag(EtherType = u16bec<0x0806>))]
pub struct Arp {
  pub hardware_type: u16bec<1>,
  pub protocol_type: u16bec<0x0800>,
  pub hardware_length: u8c<6>,
  pub protocol_length: u8c<4>,
  pub operation: Operation,
  pub sender: (Mac, [u8; 4]),
  pub target: (Mac, [u8; 4]),
}

// -------------------------------------------------------------------------------------------------
// IPv4

/// An IPv4 packet: its size is in its header, and so is the size of its options.
#[derive(Data)]
#[repr(C)]
#[data(tag(EtherType = u16bec<0x0800>))]
#[data(size = total_length)]
#[data(assert(total_length >= (version_ihl & 0x0f) * 4))]
#[data(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))]
#[data(get(
  version: u8 = version_ihl >> 4,
  header_length: usize = (version_ihl & 0x0f) * 4,
  more_fragments: bool = flags_fragment_offset & 0x2000 != 0,
  fragment_offset: u16 = flags_fragment_offset & 0x1fff,
))]
pub struct Ipv4<P: ?Sized + Data = [u8]> {
  /// Version (4 bits, 4), then the length of the header in 32 bits words (4 bits, at least 5).
  #[data(valid(0x45..=0x4f))]
  pub version_ihl: u8,
  pub type_of_service: u8,
  pub total_length: u16be,
  pub identification: u16be,
  pub flags_fragment_offset: u16be,
  pub time_to_live: u8,
  pub protocol: u8,
  pub checksum: u16be,
  pub source: [u8; 4],
  pub destination: [u8; 4],
  #[data(regions(options = (version_ihl & 0x0f) * 4 - 20, payload))]
  pub data: P,
}

/// An IPv4 option that has a length (all but "end of options" and "no operation").
#[derive(Data)]
#[repr(C)]
#[data(size = length)]
pub struct Ipv4Option {
  #[data(valid(2..))]
  pub kind: u8,
  pub length: u8,
  pub data: [u8],
}

/// A UDP datagram.
#[derive(Data)]
#[repr(C)]
#[data(size = length)]
pub struct Udp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub length: u16be,
  pub checksum: u16be,
  pub payload: [u8],
}

// -------------------------------------------------------------------------------------------------
// Use

const A: Mac = Mac([0x02, 0, 0, 0, 0, 0x0a]);
const B: Mac = Mac([0x02, 0, 0, 0, 0, 0x0b]);

/// An ARP request, as bytes.
const ARP: [u8; 28] = [
  0, 1, 0x08, 0, 6, 4, 0, 1, 0x02, 0, 0, 0, 0, 0x0a, 10, 0, 0, 1, 0, 0, 0, 0, 0, 0, 10, 0, 0, 2,
];

/// A frame from `A` to `B`, as bytes: an EtherType or a length, then a payload.
fn frame(ether_type: u16, payload: &[u8]) -> Vec<u8> {
  [&B.0[..], &A.0[..], &ether_type.to_be_bytes()[..], payload].concat()
}

/// An IPv4 packet, as bytes: an option that has a length, two that do not, a UDP datagram.
fn ipv4() -> Vec<u8> {
  let mut header = vec![0x47, 0, 0, 28 + 13, 0, 1, 0x20, 0x07, 64, 17, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2];
  header.extend([7, 6, 4, 10, 0, 0, 1, 1]);
  let checksum = !mutant::sum16(&header);
  header[10..12].copy_from_slice(&checksum.to_be_bytes());
  [&header[..], &[0x30, 0x39, 0, 53, 0, 8 + 5, 0, 0][..], b"hello"].concat()
}

pub fn main() {
  // ---- A frame, of any payload, then of the payload that its EtherType tells.
  let bytes = frame(0x0800, &ipv4());
  let any: &Frame = try_transmute_ref(&bytes[..]).unwrap();
  assert_eq!((any.destination, any.source, any.ether_type.0.get()), (B, A, 0x0800));
  assert_eq!(any.payload.len(), 41);

  let ip: &Frame<Ipv4> = try_transmute_ref(&bytes[..]).unwrap();
  assert_eq!(ip.payload.total_length.get(), 41);
  assert_eq!(ip.payload.protocol, 17);
  // Bit fields.
  assert_eq!((ip.payload.version(), ip.payload.header_length()), (4, 28));
  assert_eq!((ip.payload.more_fragments(), ip.payload.fragment_offset()), (true, 7));
  // The regions of the packet: their sizes come from the header.
  assert_eq!(ip.payload.options(), [7, 6, 4, 10, 0, 0, 1, 1]);
  let (udp, rest) = unpack::<Udp>(ip.payload.payload()).unwrap();
  assert_eq!((udp.destination_port.get(), &udp.payload, rest.len()), (53, &b"hello"[..], 0));
  // The options: one that has a length, then two that are a single byte.
  let (option, rest) = unpack::<Ipv4Option>(ip.payload.options()).unwrap();
  assert_eq!((option.kind, option.length, &option.data), (7, 6, &[4, 10, 0, 0][..]));
  assert_eq!(unpack::<[u8c<1>; 2]>(rest).unwrap().1.len(), 0);
  assert_eq!(unpack::<Ipv4Option>(rest).err(), Some(Error::Validity));

  // ---- It is not something else.
  assert_eq!(try_transmute_ref::<_, Frame<Arp>>(&bytes[..]).err(), Some(Error::Size));
  assert_eq!(try_transmute_ref::<_, Frame<Vlan>>(&bytes[..]).err(), Some(Error::Validity));
  // Not an EtherType.
  assert_eq!(try_transmute_ref::<_, Frame>(&frame(100, &[0; 100])[..]).err(), Some(Error::Validity));
  // Version 6, a header of 16 bytes, a total length that is not the one of the packet, and a
  // time to live that the checksum does not agree with.
  for (at, value) in [(14, 0x67), (14, 0x44), (17, 40), (22, 63)] {
    let mut bytes = bytes.clone();
    bytes[at] = value;
    assert!(try_transmute_ref::<_, Frame>(&bytes[..]).is_ok());
    assert_eq!(try_transmute_ref::<_, Frame<Ipv4>>(&bytes[..]).err(), Some(Error::Validity));
  }
  // Options that are longer than the packet.
  let mut short = frame(0x0800, &ipv4()[..24]);
  short[17] = 24;
  assert_eq!(try_transmute_ref::<_, Frame<Ipv4>>(&short[..]).err(), Some(Error::Validity));
  // Too short for the header.
  assert_eq!(try_transmute_ref::<_, Frame<Ipv4>>(&bytes[..20]).err(), Some(Error::Size));

  // ---- On the wire, a small frame is padded to 60 bytes: the packet tells where it ends.
  let mut wire = bytes.clone();
  wire.resize(60, 0);
  assert!(try_transmute_ref::<_, WireFrame>(&wire[..]).is_ok());
  assert_eq!(try_transmute_ref::<_, WireFrame>(&bytes[..]).err(), Some(Error::Validity));
  assert_eq!(try_transmute_ref::<_, Frame<Ipv4>>(&wire[..]).err(), Some(Error::Validity));
  let (ip, padding) = unpack::<Frame<Ipv4>>(&wire).unwrap();
  assert_eq!((size_of_val(ip), padding), (55, &[0; 5][..]));
  assert!(try_transmute_ref::<_, WireFrame>(&[0xff; 1515][..]).is_err());

  // ---- The frame check sequence, when it is there: the CRC-32 of the frame.
  wire.extend(mutant::crc32(&wire).to_le_bytes());
  let checked: &CheckedFrame = try_transmute_ref(&wire[..]).unwrap();
  assert_eq!((checked.payload().len(), checked.fcs().len()), (46, 4));
  // As anyone would check it: the CRC of a frame with its sequence is a constant.
  assert_eq!(mutant::crc32(&wire), 0x2144df1c);
  for at in [0, 30, 59, 60, 63] {
    let mut wire = wire.clone();
    wire[at] ^= 1;
    assert_eq!(try_transmute_ref::<_, CheckedFrame>(&wire[..]).err(), Some(Error::Validity));
  }
  assert!(try_transmute_ref::<_, CheckedFrame>(&wire[..63]).is_err());
  // On the medium, after its preamble.
  let mut medium = [&[0x55; 7][..], &[0xd5], &wire].concat();
  let packet: &Packet = try_transmute_ref(&medium[..]).unwrap();
  assert_eq!((packet.frame.source, packet.frame.payload().len()), (A, 46));
  medium[7] = 0x55;
  assert_eq!(try_transmute_ref::<_, Packet>(&medium[..]).err(), Some(Error::Validity));

  // ---- 802.1Q: a tag, then a frame; twice for "Q-in-Q".
  let tagged = frame(0x8100, &[&[0xb0, 0x2a, 0x08, 0x00][..], &ipv4()].concat());
  let vlan: &Frame<Vlan<Ipv4>> = try_transmute_ref(&tagged[..]).unwrap();
  let tag = &vlan.payload;
  assert_eq!((tag.priority(), tag.drop_eligible(), tag.id()), (5, true, 42));
  assert_eq!(tag.payload.time_to_live, 64);
  assert_eq!(try_transmute_ref::<_, Frame<Ipv4>>(&tagged[..]).err(), Some(Error::Validity));
  let reserved = frame(0x8100, &[&[0x2f, 0xff, 0x08, 0x00][..], &ipv4()].concat());
  assert_eq!(try_transmute_ref::<_, Frame<Vlan<Ipv4>>>(&reserved[..]).err(), Some(Error::Validity));
  let double = frame(0x8100, &[&[0, 1, 0x81, 0x00, 0, 2, 0x08, 0x06][..], &ARP[..]].concat());
  let qinq: &Frame<Vlan<Vlan<Arp>>> = try_transmute_ref(&double[..]).unwrap();
  assert_eq!((qinq.payload.id(), qinq.payload.payload.id()), (1, 2));
  assert!(try_transmute_ref::<_, Frame<Vlan<Vlan<Ipv4>>>>(&double[..]).is_err());

  // ---- 802.3: a length instead of an EtherType, and padding after the payload.
  let llc = frame(3, &[0xaa, 0xaa, 0x03, 0, 0, 0, 0]);
  let frame8023: &Frame8023 = try_transmute_ref(&llc[..]).unwrap();
  assert_eq!((frame8023.payload(), frame8023.padding()), (&[0xaa, 0xaa, 0x03][..], &[0; 4][..]));
  assert!(try_transmute_ref::<_, Frame>(&llc[..]).is_err());
  assert!(try_transmute_ref::<_, Frame8023>(&frame(9, &[0; 7])[..]).is_err());
  assert!(try_transmute_ref::<_, Frame8023>(&bytes[..]).is_err());
  // LLC and SNAP headers, then what the EtherType of the SNAP header tells.
  let snap = frame(8 + 28, &[&[0xaa, 0xaa, 0x03, 0, 0, 0, 0x08, 0x06][..], &ARP[..], &[0; 10]].concat());
  let frame8023: &Frame8023 = try_transmute_ref(&snap[..]).unwrap();
  let (arp, rest) = unpack::<Snap<Arp>>(frame8023.payload()).unwrap();
  assert_eq!((arp.payload.operation, rest.len(), frame8023.padding().len()), (Operation::Request, 0, 10));
  assert!(unpack::<Snap<Ipv4>>(frame8023.payload()).is_err());

  // ---- ARP: a payload of a fixed size, made in safe Rust.
  let request = Frame {
    destination: Mac([0xff; 6]),
    source: A,
    ether_type: Default::default(),
    payload: Arp {
      hardware_type: Default::default(),
      protocol_type: Default::default(),
      hardware_length: Default::default(),
      protocol_length: Default::default(),
      operation: Operation::Request,
      sender: (A, [10, 0, 0, 1]),
      target: (Mac([0; 6]), [10, 0, 0, 2]),
    },
  };
  // Nothing to check to see it as bytes, or as a frame of any payload: this is all proven.
  let bytes: &[u8; 42] = transmute_ref(&request);
  assert_eq!(bytes[12..14], [0x08, 0x06]);
  assert_eq!(bytes[14..], ARP);
  let any: &Frame = transmute_ref(&request);
  assert_eq!((any.ether_type.0.get(), any.payload.len()), (0x0806, 28));
  // And back: only what the bytes do not tell is checked.
  let arp: &Frame<Arp> = try_transmute_ref(bytes).unwrap();
  assert_eq!(arp.payload, request.payload);
  let mut reply = *bytes;
  reply[21] = 2;
  // (A field of a packed struct is read by value.)
  let operation = try_transmute_ref::<_, Frame<Arp>>(&reply).unwrap().payload.operation;
  assert_eq!(operation, Operation::Reply);
  reply[21] = 3;
  assert_eq!(try_transmute_ref::<_, Frame<Arp>>(&reply).err(), Some(Error::Validity));

  // ---- A packet made in safe Rust, of a payload whose size is known...
  let mut packet = Ipv4 {
    version_ihl: 0x45,
    type_of_service: 0,
    total_length: u16be::new(20 + 4),
    identification: u16be::new(1),
    flags_fragment_offset: u16be::new(0),
    time_to_live: 64,
    protocol: 253,
    checksum: u16be::new(0),
    source: [10, 0, 0, 1],
    destination: [10, 0, 0, 2],
    data: *b"ping",
  };
  packet.checksum = u16be::new(!mutant::sum16(&as_bytes(&packet)[..20]));
  // ... is a packet of any payload.
  let packet: &Ipv4 = &packet;
  assert_eq!((packet.options(), packet.payload()), (&[][..], &b"ping"[..]));
  let (parsed, rest) = unpack::<Ipv4>(as_bytes(packet)).unwrap();
  assert_eq!((parsed.payload(), parsed.header_length(), rest.len()), (&b"ping"[..], 20, 0));

  println!("ok");
}
