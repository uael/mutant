use core::fmt::Debug;
use core::mem::ManuallyDrop;
use mutant::num::*;
use mutant::*;

#[repr(C, packed)]
#[derive(Data, DebugPacked, Copy, Clone, PartialEq, Eq)]
pub struct EthernetFrame<T: ?Sized + EthernetPacket> {
  pub dst_mac: [u8; 6],
  pub src_mac: [u8; 6],
  pub ether_type: T::Type,
  pub payload: ManuallyDrop<T>,
}

pub trait EthernetPacket: Data {
  type Type: Integer<Bits = u16>;
}

impl EthernetPacket for [u8] {
  type Type = u16;
}

pub mod ipv4 {
  use super::*;

  #[repr(transparent)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Ipv4(pub [u8; 4]);

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, DebugPacked, Copy, Clone, PartialEq, Eq)]
  pub struct Ipv4Packet<T: ?Sized> {
    // Version (4 bits) + IHL (4 bits) + TOS (8 bits) + Total Length (16 bits).
    version_ihl: u8, tos: u8, total_length: Size<u16>,
    // Identification (16 bits) + Flags (3 bits) + Fragment Offset (13 bits).
    identification: SetDefault<u16, 1234>, flags_fragment_offset: u16,
    // TTL (8 bits) + Protocol (8 bits) + Header Checksum (16 bits).
    ttl: u8, protocol: u8, header_checksum: u16,
    // Source IP Address (32 bits).
    src_ip: Ipv4, dst_ip: Ipv4,
    // Options (`ihl - 20` bytes) and payload (`total_length` - `ihl` bytes).
    options_payload: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data> EthernetPacket for Ipv4Packet<T> {
    type Type = u16c<0x0800>;
  }

  impl<T: ?Sized + Data> Ipv4Packet<T> {
    pub const fn version(&self) -> u8 {
      self.version_ihl >> 4
    }

    pub const fn ihl(&self) -> usize {
      ((self.version_ihl & 0x0F) * 4) as usize
    }

    pub const fn options_payload(&self) -> (&[u8], &[u8]) {
      transmute_ref::<_, [u8]>(unaligned_ref!(self.options_payload)).split_at(self.ihl() - 20)
    }
  }

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone, Eq)]
  pub struct Ipv4Option<T: ?Sized + OptionData> {
    // Option Type (8 bits) + Option Length (8 bits).
    option_type: T::Type, option_length: Size<T::Length>,
    // Option Data.
    option_data: ManuallyDrop<T>,
  }

  pub trait OptionData: Data {
    type Type: Integer<Bits = u8>;
    type Length: Integer;
  }

  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct NoOperationOption;

  impl OptionData for NoOperationOption {
    type Type = u8c<0b00000001>;
    // TODO: type Type = FixedWith<u8, 0b00000001>;
    // TODO: type Type = DefaultWith<u8, 0b00000001>;
    // TODO: type Type = Literal<u8, 0b00000001>;
    type Length = ();
  }

  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct EndOfOptions;

  impl OptionData for EndOfOptions {
    type Type = u8c<0b00000000>;
    type Length = ();
  }

  #[repr(C, packed)]
  #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone, Eq)]
  pub struct RecordRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for RecordRouteOption<T> {
    type Type = u8c<0b00000111>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone, Eq)]
  pub struct StrictSourceRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for StrictSourceRouteOption<T> {
    type Type = u8c<0b10001001>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone, Eq)]
  pub struct LooseSourceRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for LooseSourceRouteOption<T> {
    type Type = u8c<0b10000011>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Timestamp {
    ip_address: Ipv4,
    timestamp: u32,
  }

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, DebugPacked, PartialEqPacked, Copy, Clone, Eq)]
  pub struct TimestampOption<T: ?Sized + Data<Item = Timestamp> = [Timestamp]> {
    // Pointer (8 bits) + O-Flow (4 bits) + Flags (4 bits).
    pointer: u8, o_flow_flags: u8,
    // IP Addresses.
    timestamps: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Timestamp>> OptionData for TimestampOption<T> {
    type Type = u8c<0b01000100>;
    type Length = u8;
  }

  #[test]
  fn test() {
    // Pack 3 options.
    let ipv4_options = pack!(
      Ipv4Option::<NoOperationOption> { ..default() },
      Ipv4Option {
        option_data: ManuallyDrop::new(RecordRouteOption {
          pointer: 2,
          ip_addresses: ManuallyDrop::new([Ipv4([192, 168, 1, 3]), Ipv4([192, 168, 1, 4])]),
        }),
        ..default()
      },
      Ipv4Option {
        option_data: ManuallyDrop::new(TimestampOption {
          pointer: 2,
          o_flow_flags: 0,
          timestamps: ManuallyDrop::new([
            Timestamp { ip_address: Ipv4([192, 168, 1, 3]), timestamp: 123234234 },
            Timestamp { ip_address: Ipv4([192, 168, 1, 4]), timestamp: 125243563 }
          ]),
        }),
        ..default()
      }
    );

    // Convert the options to bytes.
    let bytes: &[u8] = transmute_ref(&ipv4_options);

    // Unpack and assert `NoOperationOption`.
    let (option, bytes) = unpack::<Ipv4Option<NoOperationOption>>(bytes).unwrap();
    assert_eq!(option, &ipv4_options.0);

    // Unpack and assert `RecordRouteOption`.
    let option1: &Ipv4Option<RecordRouteOption> = transmute_ref(&ipv4_options.1);
    let (option, bytes) = unpack::<Ipv4Option<RecordRouteOption>>(bytes).unwrap();
    assert_eq!(option, option1);

    // Unpack and assert `TimestampOption`.
    let option2: &Ipv4Option<TimestampOption> = transmute_ref(&ipv4_options.2);
    let (option, bytes) = unpack::<Ipv4Option<TimestampOption>>(bytes).unwrap();
    assert_eq!(option, option2);

    // `bytes` must be empty at this point.
    assert!(bytes.is_empty());

    // Create an Ethernet frame with IPv4 packet and options.
    let ipv4_ethernet_frame = EthernetFrame {
      dst_mac: [1, 2, 3, 4, 5, 6],
      src_mac: [6, 5, 4, 3, 2, 1],
      payload: ManuallyDrop::new(Ipv4Packet {
        version_ihl: 5 + 8,
        src_ip: Ipv4([192, 168, 1, 2]),
        dst_ip: Ipv4([192, 168, 1, 1]),
        options_payload: ManuallyDrop::new(pack![ipv4_options, 0xcafeu16]),
        ..default()
      }),
      ..default()
    };

    // Convert the frame to bytes.
    let bytes: &[u8] = transmute_ref(&ipv4_ethernet_frame);

    // Unpack bytes back to frame.
    let (frame, bytes): (&EthernetFrame<Ipv4Packet<[u8]>>, _) = unpack(bytes).unwrap();
    assert!(bytes.is_empty());

    // Unpack and check all options.
    let (mut options_bytes, payload_bytes) = unaligned_ref!(frame.payload).options_payload();
    while !options_bytes.is_empty() {
      (_, options_bytes) = unpack!(options_bytes {
        | Ipv4Option<NoOperationOption> => |o| assert_eq!(o, &ipv4_options.0),
        | Ipv4Option<RecordRouteOption> => |o| assert_eq!(o, option1),
        | Ipv4Option<TimestampOption> => |o| assert_eq!(o, option2),
        | Ipv4Option<EndOfOptions>
        | Ipv4Option<StrictSourceRouteOption>
        | Ipv4Option<LooseSourceRouteOption> => |o| println!("option: {:#?}", o),
        _ => panic!("unexpected option")
      });
    }

    // `options_bytes` must be empty at this point.
    assert!(options_bytes.is_empty());

    // Unpack and check payload.
    let (payload, payload_bytes) = unpack::<Unalign<u16>>(payload_bytes).unwrap();
    assert_eq!(payload.into_inner(), 0xcafe);

    // `payload_bytes` must be empty at this point.
    assert!(payload_bytes.is_empty());
  }
}

fn main() {}
