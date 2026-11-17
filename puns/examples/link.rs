//! Links and buses, in safe Rust, with nothing but `#[derive(Pun)]` and its options: IEEE
//! 802.11 frames, Bluetooth HCI packets and L2CAP, CAN and CAN FD frames (as Linux's SocketCAN
//! has them), Modbus RTU and Modbus TCP, and the byte-stuffed framings of SLIP and PPP.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads frames in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these links, and what tells it:
//!
//! | Specificity                                     | Told by                                   |
//! |-------------------------------------------------|-------------------------------------------|
//! | 802.11: a little-endian frame control           | `u16le`, `get(..)`, `assert(..)`          |
//! | 802.11: a fourth address when two bits are set  | `regions(address4 = if .. { 6 } ..)`      |
//! | 802.11: QoS and HT control, told by bits        | `regions(qos_control = .., ..)`           |
//! | 802.11: the frame check sequence, a CRC-32      | `assert(crc32(0, ..) == le(.., 4))`       |
//! | 802.11: information elements that fill a beacon | `assert(walk(36, ..) == ..)`              |
//! | HCI over UART: an indicator before a packet     | `u8c<0x01>`, an enum of references        |
//! | HCI: an opcode of two fields (OGF, OCF)         | `get(ogf: u8 = opcode >> 10, ..)`         |
//! | L2CAP in ACL data: both lengths agree           | `Acl<L2cap>`: the size of each is checked |
//! | CAN: flags in the identifier, of 11 or 29 bits  | `get(..)`, `assert(..)`                   |
//! | CAN FD: the 16 lengths that a DLC can tell      | `valid(0..=8, 12, 16, 20, 24, ..)`        |
//! | Modbus RTU: a CRC-16 of the frame               | a `walk`, tables in constants             |
//! | Modbus TCP: a protocol identifier of 0          | `u16bec<0>`, `size = length + 6`          |
//! | Modbus registers, counted in bytes              | `len = byte_count / 2` on `[u16be]`       |
//! | SLIP and PPP: escapes, where a frame ends       | `assert(walk(..) == ..)`                  |
//!
//! What is not covered, and why:
//!
//! - SLIP and PPP: what a frame holds is its bytes unescaped, which is a copy, not a view of the
//!   bytes. So the bytes are checked to be a well-formed escaped frame, but what the frame holds
//!   (the protocol of PPP, its FCS-16 over the unescaped bytes) is not a type, and is not checked.
//! - CAN on the wire is a stream of bits, stuffed, with a CRC of 15 bits: not bytes. The frames
//!   here are the ones of SocketCAN, in memory, little-endian.
//! - L2CAP frames fragmented over several ACL packets: reassembly is a copy, not a view.
//! - 802.11 bodies that are protected (CCMP, GCMP) are encrypted: not a view either.
#![deny(unsafe_code)]

use puns::{
  Error, Pun, from_head,
  num::{u8c, u16be, u16bec, u16le, u16lec, u32le, u64le},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// 802.11

/// An 802.11 data frame, with its frame check sequence.
#[derive(Pun)]
#[repr(C)]
// Protocol version 0, type 2 (data).
#[pun(assert(frame_control & 0x000f == 0x0008))]
#[pun(assert(crc32(0, size_of_val(self) - 4) == le(size_of_val(self) - 4, 4)))]
#[pun(get(
  subtype: u8 = (frame_control >> 4) & 0xf,
  to_ds: bool = frame_control & 0x0100 != 0,
  from_ds: bool = frame_control & 0x0200 != 0,
  retry: bool = frame_control & 0x0800 != 0,
  protected: bool = frame_control & 0x4000 != 0,
  qos: bool = frame_control & 0x0080 != 0,
  sequence: u16 = sequence_control >> 4,
  fragment: u8 = sequence_control & 0xf,
))]
pub struct DataFrame {
  pub frame_control: u16le,
  pub duration: u16le,
  pub address1: [u8; 6],
  pub address2: [u8; 6],
  pub address3: [u8; 6],
  pub sequence_control: u16le,
  #[pun(regions(
    // A fourth address when both "to DS" and "from DS" are set.
    address4 = if frame_control & 0x0300 == 0x0300 { 6 } else { 0 },
    // The QoS subtypes: bit 3 of the subtype.
    qos_control = if frame_control & 0x0080 != 0 { 2 } else { 0 },
    // HT control: the order bit, in a QoS frame.
    ht_control = if frame_control & 0x8080 == 0x8080 { 4 } else { 0 },
    body = size_of_val(self) - 24 - 4
      - (if frame_control & 0x0300 == 0x0300 { 6 } else { 0 })
      - (if frame_control & 0x0080 != 0 { 2 } else { 0 })
      - (if frame_control & 0x8080 == 0x8080 { 4 } else { 0 }),
    fcs,
  ))]
  pub data: [u8],
}

/// A beacon (management, subtype 8), with its frame check sequence: fixed fields, then
/// information elements that fill the frame.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(crc32(0, size_of_val(self) - 4) == le(size_of_val(self) - 4, 4)))]
#[pun(assert(walk(
  36,
  |at| at < size_of_val(self) - 4,
  |at| at + 2 + be(at + 1, 1),
) == size_of_val(self) - 4))]
#[pun(get(ess: bool = capability & 1 != 0, privacy: bool = capability & 0x10 != 0))]
pub struct Beacon {
  pub frame_control: u16lec<0x0080>,
  pub duration: u16le,
  pub destination: [u8; 6],
  pub source: [u8; 6],
  pub bssid: [u8; 6],
  pub sequence_control: u16le,
  pub timestamp: u64le,
  pub interval: u16le,
  pub capability: u16le,
  #[pun(regions(elements = size_of_val(self) - 36 - 4, fcs))]
  pub data: [u8],
}

/// An information element.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct Element {
  pub id: u8,
  pub length: u8,
  pub data: [u8],
}

/// The SSID element: up to 32 bytes.
#[derive(Pun)]
#[repr(C)]
pub struct Ssid {
  pub id: u8c<0>,
  #[pun(valid(..=32))]
  pub length: u8,
  #[pun(len = length)]
  pub name: [u8],
}

/// An ACK (control, subtype 13): a receiver address and the frame check sequence.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(crc32(0, 10) == fcs))]
pub struct Ack {
  pub frame_control: u16lec<0x00d4>,
  pub duration: u16le,
  pub receiver: [u8; 6],
  pub fcs: u32le,
}

// -------------------------------------------------------------------------------------------------
// Bluetooth

/// An HCI command, after its indicator on a UART (H4).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 4)]
#[pun(get(ogf: u8 = opcode >> 10, ocf: u16 = opcode & 0x3ff))]
pub struct Command {
  pub indicator: u8c<0x01>,
  pub opcode: u16le,
  pub length: u8,
  pub parameters: [u8],
}

/// HCI ACL data, of a payload `P`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 5)]
#[pun(get(handle: u16 = handle_flags & 0x0fff, boundary: u8 = (handle_flags >> 12) & 3))]
// The boundary flag 3 is reserved; so is the broadcast flag 3.
#[pun(assert((handle_flags >> 12) & 3 != 3 && handle_flags >> 14 != 3))]
pub struct Acl<P: ?Sized = [u8]> {
  pub indicator: u8c<0x02>,
  pub handle_flags: u16le,
  pub length: u16le,
  pub data: P,
}

/// An HCI event.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 3)]
pub struct Event {
  pub indicator: u8c<0x04>,
  pub code: u8,
  pub length: u8,
  pub parameters: [u8],
}

/// The Command Complete event: the number of packets allowed, the opcode, return parameters.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 3)]
#[pun(assert(length >= 3))]
pub struct CommandComplete {
  pub indicator: u8c<0x04>,
  pub code: u8c<0x0e>,
  pub length: u8,
  pub packets: u8,
  pub opcode: u16le,
  pub parameters: [u8],
}

/// An L2CAP basic frame, whole: its length and the one of the ACL data that holds it agree.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 4)]
pub struct L2cap {
  pub length: u16le,
  #[pun(valid(1..))]
  pub channel: u16le,
  pub payload: [u8],
}

/// A packet on an HCI UART, by its indicator.
#[derive(Pun)]
pub enum Packet<'a> {
  Complete(&'a CommandComplete),
  Command(&'a Command),
  Acl(&'a Acl<L2cap>),
  Event(&'a Event),
}

// -------------------------------------------------------------------------------------------------
// CAN

/// A classic CAN frame (`struct can_frame`, on a little-endian host): an identifier with flags,
/// 0 to 8 bytes of data.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
// A standard identifier is 11 bits.
#[pun(assert(can_id & 0x8000_0000 != 0 || can_id & 0x1fff_f800 == 0))]
// A DLC above 8 is only for a length of 8.
#[pun(assert(len8_dlc == 0 || len == 8))]
#[pun(get(
  extended: bool = can_id & 0x8000_0000 != 0,
  remote: bool = can_id & 0x4000_0000 != 0,
  error: bool = can_id & 0x2000_0000 != 0,
  id: u32 = can_id & 0x1fff_ffff,
))]
pub struct CanFrame {
  pub can_id: u32le,
  #[pun(valid(0..=8))]
  pub len: u8,
  pub pad: u8,
  pub res0: u8,
  /// The DLC of 9 to 15, for a length of 8.
  #[pun(valid(0, 9..=15))]
  pub len8_dlc: u8,
  pub data: [u8; 8],
}

/// A CAN FD frame: its length is one of the 16 that a DLC can tell; there are no remote frames.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(can_id & 0x8000_0000 != 0 || can_id & 0x1fff_f800 == 0))]
#[pun(assert(can_id & 0x4000_0000 == 0))]
#[pun(get(bit_rate_switch: bool = flags & 1 != 0))]
pub struct CanFdFrame {
  pub can_id: u32le,
  #[pun(valid(0..=8, 12, 16, 20, 24, 32, 48, 64))]
  pub len: u8,
  /// BRS, ESI, FDF.
  #[pun(valid(0..=7))]
  pub flags: u8,
  pub res0: u8c<0>,
  pub res1: u8c<0>,
  pub data: [u8; 64],
}

// -------------------------------------------------------------------------------------------------
// Modbus

/// `T[0]`, `T[1]`, `T[2]`, `T[3]` of the byte table of CRC-16/MODBUS, 16 bits each.
pub const T0: u128 = 0x0140_c181_c0c1_0000;
/// `T[0]`, `T[4]`, `T[8]`, `T[12]`.
pub const T1: u128 = 0x0500_c601_c301_0000;
/// `T[0]`, `T[16]`, `T[32]`, `T[48]`.
pub const T2: u128 = 0x1400_d801_cc01_0000;
/// `T[0]`, `T[64]`, `T[128]`, `T[192]`.
pub const T3: u128 = 0x5000_a001_f001_0000;

/// A Modbus RTU frame: an address, a function, data, and the CRC-16 of all of it, least
/// significant byte first; 4 to 256 bytes.
///
/// There is no CRC-16 in the expressions. A walk computes it, a byte per step, with a state of
/// `offset << 24 | crc`. A CRC is linear, so the table of a byte is the XOR of four tables of
/// four entries, `T[x] = T[x & 3] ^ T[x & 12] ^ T[x & 48] ^ T[x & 192]`, each packed in a
/// constant. The step is 49 operations, of the 56 a walk allows. This is the current way, until
/// a function for it is built in.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_val(self) >= 4 && size_of_val(self) <= 256))]
#[pun(assert({
  let state = walk(
    0xffff,
    |s| (s >> 24) < size_of_val(self) - 2,
    |s| {
      let x = s ^ be(s >> 24, 1);
      (((s >> 8) ^ (T0 >> ((x & 3) * 16)) ^ (T1 >> ((x & 12) * 4)) ^ (T2 >> (x & 48))
        ^ (T3 >> ((x & 192) / 4))) & 0xffff) | ((s | 0xff_ffff) + 1)
    },
  );
  state & 0xffff == le(size_of_val(self) - 2, 2)
}))]
#[pun(get(exception: bool = function & 0x80 != 0))]
pub struct RtuFrame {
  /// 0 is broadcast; 248 to 255 are reserved.
  #[pun(valid(0..=247))]
  pub address: u8,
  pub function: u8,
  #[pun(regions(data = size_of_val(self) - 4, crc))]
  pub rest: [u8],
}

/// The MBAP header of Modbus TCP, and its PDU.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 6)]
#[pun(assert(length >= 2 && length <= 254))]
pub struct TcpAdu {
  pub transaction: u16be,
  pub protocol: u16bec<0>,
  pub length: u16be,
  pub unit: u8,
  pub function: u8,
  pub data: [u8],
}

/// Function 3, "read holding registers": the request.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(count >= 1 && count <= 125))]
pub struct ReadHoldingRegisters {
  pub start: u16be,
  pub count: u16be,
}

/// Function 3: the response, a byte count, then registers.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(byte_count % 2 == 0))]
pub struct RegistersResponse {
  pub byte_count: u8,
  #[pun(len = byte_count / 2)]
  pub registers: [u16be],
}

// -------------------------------------------------------------------------------------------------
// Byte stuffing

/// A SLIP frame (RFC 1055): an END (`0xc0`) only at the end; an ESC (`0xdb`) only before
/// `ESC_END` (`0xdc`) or `ESC_ESC` (`0xdd`).
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 1 && be(size_of_val(self) - 1, 1) == 0xc0))]
#[pun(assert(walk(
  0,
  |at| at < size_of_val(self) - 1,
  |at| if be(at, 1) == 0xc0 {
    1 / 0
  } else if be(at, 1) == 0xdb {
    if be(at + 1, 1) == 0xdc || be(at + 1, 1) == 0xdd { at + 2 } else { 1 / 0 }
  } else {
    at + 1
  },
) == size_of_val(self) - 1))]
pub struct Slip(pub [u8]);

/// A PPP frame in HDLC-like framing (RFC 1662): flags (`0x7e`) around it, and escapes.
///
/// An escape is `0x7d`, then a byte XOR `0x20`. The address (`0xff`) and the control (`0x03`)
/// fields are escaped as well in general, as `0x7d 0x23`.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 6 && be(0, 1) == 0x7e && be(size_of_val(self) - 1, 1) == 0x7e))]
#[pun(assert(be(1, 1) == 0xff && be(2, 2) == 0x7d23))]
#[pun(assert(walk(
  1,
  |at| at < size_of_val(self) - 1,
  |at| if be(at, 1) == 0x7e {
    1 / 0
  } else if be(at, 1) == 0x7d {
    if be(at + 1, 1) == 0x7e { 1 / 0 } else { at + 2 }
  } else {
    at + 1
  },
) == size_of_val(self) - 1))]
pub struct Ppp(pub [u8]);

// -------------------------------------------------------------------------------------------------
// Use

/// An 802.11 QoS data frame to the distribution system, as bytes: an LLC/SNAP header of IPv4,
/// the frame check sequence.
const QOS_DATA: [u8; 38] = [
  0x88, 0x01, 0x2c, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
  0x00, 0x11, 0x22, 0x33, 0x44, 0x56, 0x30, 0x01, 0x05, 0x00, 0xaa, 0xaa, 0x03, 0x00, 0x00, 0x00,
  0x08, 0x00, 0x8f, 0x7a, 0x3a, 0x83,
];

/// The same, from and to the distribution system: four addresses.
const MESH_DATA: [u8; 44] = [
  0x88, 0x03, 0x2c, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
  0x00, 0x11, 0x22, 0x33, 0x44, 0x56, 0x30, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x05, 0x00,
  0xaa, 0xaa, 0x03, 0x00, 0x00, 0x00, 0x08, 0x00, 0x9a, 0xf7, 0x70, 0x9a,
];

/// An 802.11 beacon, as bytes: an SSID, supported rates, the frame check sequence.
const BEACON: [u8; 50] = [
  0x80, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01,
  0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
  0x64, 0x00, 0x11, 0x00, 0x00, 0x04, b't', b'e', b's', b't', 0x01, 0x02, 0x82, 0x84, 0xeb, 0xbe,
  0xbe, 0xaf,
];

/// An 802.11 ACK, as bytes.
const ACK: [u8; 14] =
  [0xd4, 0x00, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x71, 0xea, 0xf2, 0x4b];

/// HCI packets on a UART, as bytes: `HCI_Reset`, its Command Complete, then ACL data of an L2CAP
/// frame on channel 4 (ATT).
const HCI: [u8; 23] = [
  0x01, 0x03, 0x0c, 0x00, 0x04, 0x0e, 0x04, 0x01, 0x03, 0x0c, 0x00, 0x02, 0x40, 0x20, 0x07, 0x00,
  0x03, 0x00, 0x04, 0x00, 0x0a, 0x03, 0x00,
];

/// A CAN frame, as bytes: identifier `0x123`, 2 bytes of data.
const CAN: [u8; 16] = [0x23, 0x01, 0, 0, 2, 0, 0, 0, 0xde, 0xad, 0, 0, 0, 0, 0, 0];

/// A Modbus RTU request, as bytes: read 10 holding registers of slave 1, the CRC (`0xcdc5`).
const READ: [u8; 8] = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0a, 0xc5, 0xcd];

/// A Modbus RTU response, as bytes: three registers of slave 17, the CRC.
const REGISTERS: [u8; 11] = [0x11, 0x03, 0x06, 0x02, 0x2b, 0x00, 0x00, 0x00, 0x64, 0xc8, 0xba];

/// A Modbus TCP request, as bytes: read 3 holding registers of unit 17 from 107; a byte after it.
const ADU: [u8; 13] =
  [0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6b, 0x00, 0x03, 0xee];

/// A SLIP frame, as bytes: `0x45 0xc0 0x01 0xdb 0x02`, escaped.
const SLIP: [u8; 8] = [0x45, 0xdb, 0xdc, 0x01, 0xdb, 0xdd, 0x02, 0xc0];

/// A PPP frame in HDLC-like framing, as bytes: an LCP Configure-Request, escaped, with its FCS.
const LCP: [u8; 17] = [
  0x7e, 0xff, 0x7d, 0x23, 0xc0, 0x21, 0x7d, 0x21, 0x7d, 0x21, 0x7d, 0x20, 0x7d, 0x24, 0xd1, 0xb5,
  0x7e,
];

pub fn main() {
  // ---- 802.11: bit fields, optional fields, the frame check sequence.
  let frame: &DataFrame = try_pun_ref(&QOS_DATA[..]).unwrap();
  assert_eq!(
    (frame.subtype(), frame.to_ds(), frame.from_ds(), frame.qos()),
    (8, true, false, true)
  );
  assert_eq!(
    (frame.sequence(), frame.fragment(), frame.retry(), frame.protected()),
    (19, 0, false, false)
  );
  assert_eq!(
    (frame.address4().len(), frame.qos_control(), frame.ht_control().len()),
    (0, &[5, 0][..], 0)
  );
  assert_eq!((frame.body(), frame.fcs().len()), (&[0xaa, 0xaa, 0x03, 0, 0, 0, 0x08, 0x00][..], 4));
  let mesh: &DataFrame = try_pun_ref(&MESH_DATA[..]).unwrap();
  assert_eq!((mesh.address4(), mesh.body().len()), (&[2, 0, 0, 0, 0, 0x0c][..], 8));
  // The frame control, an address, the body, the frame check sequence: the CRC covers them all.
  for at in [0, 1, 10, 30, 43] {
    let mut bytes = MESH_DATA;
    bytes[at] ^= 0x04;
    assert_eq!(try_pun_ref::<_, DataFrame>(&bytes[..]).err(), Some(Error::Validity));
  }
  // Information elements, which fill a beacon.
  let beacon: &Beacon = try_pun_ref(&BEACON[..]).unwrap();
  assert_eq!((beacon.interval.get(), beacon.ess(), beacon.privacy()), (100, true, true));
  let (element, rest) = from_head::<Element>(beacon.elements()).unwrap();
  let ssid: &Ssid = try_pun_ref(&beacon.elements()[..6]).unwrap();
  assert_eq!((element.id, &ssid.name, rest), (0, &b"test"[..], &[1, 2, 0x82, 0x84][..]));
  // An element that runs into the frame check sequence (and a sequence that agrees with it).
  let mut bytes = BEACON[..46].to_vec();
  bytes[37] = 5;
  bytes.extend(puns::crc32(&bytes).to_le_bytes());
  assert_eq!(try_pun_ref::<_, Beacon>(&bytes[..]).err(), Some(Error::Validity));
  // An SSID of 33 bytes.
  let long = [&[0, 33][..], &[b'x'; 33]].concat();
  assert_eq!(try_pun_ref::<_, Ssid>(&long[..]).err(), Some(Error::Validity));
  assert!(try_pun_ref::<_, Ack>(&ACK[..]).is_ok());
  let mut bytes = ACK;
  bytes[5] = 0;
  assert_eq!(try_pun_ref::<_, Ack>(&bytes[..]).err(), Some(Error::Validity));

  // ---- Bluetooth: packets told by their indicator.
  let (packet, rest) = Packet::from_head(&HCI).unwrap();
  let Packet::Command(command) = packet else { panic!() };
  assert_eq!((command.ogf(), command.ocf(), command.parameters.len()), (3, 3, 0));
  let (packet, rest) = Packet::from_head(rest).unwrap();
  let Packet::Complete(complete) = packet else { panic!() };
  assert_eq!(
    (complete.packets, complete.opcode.get(), &complete.parameters),
    (1, 0x0c03, &[0][..])
  );
  let (packet, rest) = Packet::from_head(rest).unwrap();
  let Packet::Acl(acl) = packet else { panic!() };
  assert_eq!(
    (acl.handle(), acl.boundary(), acl.data.channel.get(), &acl.data.payload),
    (0x40, 2, 4, &[0x0a, 3, 0][..])
  );
  assert_eq!(rest.len(), 0);
  // Any event, of any code.
  let Ok(Packet::Event(event)) = Packet::from_bytes(&[0x04, 0x13, 0x01, 0x00]) else { panic!() };
  assert_eq!((event.code, &event.parameters), (0x13, &[0][..]));
  assert!(from_head::<Event>(&HCI[4..]).is_ok());
  // An L2CAP length that disagrees with the ACL length; a reserved boundary flag; channel 0; an
  // indicator that is not one.
  for (at, value) in [(16, 4), (13, 0x30), (18, 0)] {
    let mut bytes = HCI;
    bytes[at] = value;
    assert_eq!(Packet::from_bytes(&bytes[11..]).err(), Some(Error::Validity));
  }
  assert_eq!(Packet::from_bytes(&[0x05, 0, 0, 0]).err(), Some(Error::Validity));

  // ---- CAN: flags and an identifier in a word, a length of 0 to 8.
  let can: &CanFrame = try_pun_ref(&CAN).unwrap();
  assert_eq!((can.id(), can.extended(), can.remote(), can.error()), (0x123, false, false, false));
  assert_eq!((can.len, &can.data[..2]), (2, &[0xde, 0xad][..]));
  // A length of 9; an identifier of 12 bits that is not extended; a DLC of 9 for a length of 2.
  for (at, value) in [(4, 9), (1, 0x08), (7, 9)] {
    let mut bytes = CAN;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, CanFrame>(&bytes).err(), Some(Error::Validity));
  }
  // Extended: 29 bits.
  let mut extended = CAN;
  extended[1] = 0x08;
  extended[3] = 0x80;
  let can: &CanFrame = try_pun_ref(&extended).unwrap();
  assert_eq!((can.extended(), can.id()), (true, 0x823));
  // CAN FD: 48 bytes, with a bit rate switch; 40 bytes is not a length; no remote frames.
  let mut fd = [0u8; 72];
  fd[4] = 48;
  fd[5] = 1;
  let frame: &CanFdFrame = try_pun_ref(&fd).unwrap();
  assert_eq!((frame.len, frame.bit_rate_switch()), (48, true));
  fd[4] = 40;
  assert_eq!(try_pun_ref::<_, CanFdFrame>(&fd).err(), Some(Error::Validity));
  fd[4] = 48;
  fd[3] = 0x40;
  assert_eq!(try_pun_ref::<_, CanFdFrame>(&fd).err(), Some(Error::Validity));

  // ---- Modbus RTU: a CRC-16 over the frame.
  let frame: &RtuFrame = try_pun_ref(&READ[..]).unwrap();
  assert_eq!(
    (frame.address, frame.function, frame.exception(), frame.crc()),
    (1, 3, false, &[0xc5, 0xcd][..])
  );
  let request: &ReadHoldingRegisters = try_pun_ref(frame.data()).unwrap();
  assert_eq!((request.start.get(), request.count.get()), (0, 10));
  // Any bit of any byte.
  for at in 0..READ.len() {
    for bit in 0..8 {
      let mut bytes = READ;
      bytes[at] ^= 1 << bit;
      assert_eq!(try_pun_ref::<_, RtuFrame>(&bytes[..]).err(), Some(Error::Validity));
    }
  }
  let frame: &RtuFrame = try_pun_ref(&REGISTERS[..]).unwrap();
  let registers: &RegistersResponse = try_pun_ref(frame.data()).unwrap();
  let values: Vec<u16> = registers.registers.iter().map(|register| register.get()).collect();
  assert_eq!((frame.address, values), (17, vec![0x22b, 0, 0x64]));
  // The largest frame, and one byte more.
  let mut largest = vec![1u8; 254];
  largest.extend(0x454f_u16.to_le_bytes());
  assert!(try_pun_ref::<_, RtuFrame>(&largest[..]).is_ok());
  largest.insert(0, 1);
  assert_eq!(try_pun_ref::<_, RtuFrame>(&largest[..]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, RtuFrame>(&READ[..3]).err(), Some(Error::Validity));

  // ---- Modbus TCP: a header, then the same PDU.
  let (adu, rest) = from_head::<TcpAdu>(&ADU).unwrap();
  assert_eq!((adu.transaction.get(), adu.unit, adu.function, rest), (1, 0x11, 3, &[0xee][..]));
  let request: &ReadHoldingRegisters = try_pun_ref(&adu.data).unwrap();
  assert_eq!((request.start.get(), request.count.get()), (0x6b, 3));
  // A protocol identifier; a length below 2; a length beyond the bytes.
  for (at, value, error) in [(3, 1, Error::Validity), (5, 1, Error::Validity), (5, 9, Error::Size)]
  {
    let mut bytes = ADU;
    bytes[at] = value;
    assert_eq!(from_head::<TcpAdu>(&bytes).err(), Some(error));
  }
  // No registers to read.
  let mut bytes = ADU;
  bytes[11] = 0;
  let (adu, _) = from_head::<TcpAdu>(&bytes).unwrap();
  assert_eq!(try_pun_ref::<_, ReadHoldingRegisters>(&adu.data).err(), Some(Error::Validity));

  // ---- SLIP and PPP: escaped frames.
  assert!(try_pun_ref::<_, Slip>(&SLIP[..]).is_ok());
  // An END inside, an ESC before something else, no END at the end.
  for (at, value) in [(2, 0xc0), (2, 0x01), (7, 0x00)] {
    let mut bytes = SLIP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Slip>(&bytes[..]).err(), Some(Error::Validity));
  }
  assert_eq!(try_pun_ref::<_, Slip>(&[1u8, 0xdb, 0xc0][..]).err(), Some(Error::Validity));
  assert!(try_pun_ref::<_, Ppp>(&LCP[..]).is_ok());
  // An address; a flag inside.
  for (at, value) in [(1, 0xfe), (5, 0x7e)] {
    let mut bytes = LCP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Ppp>(&bytes[..]).err(), Some(Error::Validity));
  }
  // A corrupted FCS is not seen: the FCS is over the unescaped bytes.
  let mut bytes = LCP;
  bytes[15] ^= 0x01;
  assert!(try_pun_ref::<_, Ppp>(&bytes[..]).is_ok());

  println!("ok");
}
