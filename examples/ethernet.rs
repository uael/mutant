use core::mem::ManuallyDrop;
use mutant::num::*;
use mutant::*;

#[repr(C, packed)]
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
pub struct EthernetFrame<T: ?Sized + EthernetPacket> {
  pub dst_mac: [u8; 6],
  pub src_mac: [u8; 6],
  pub ether_type: T::Type,
  pub payload: ManuallyDrop<T>,
}

pub trait EthernetPacket: Data {
  type Type: Integer<Bits = u16>;
}

impl<T: EthernetPacket> EthernetFrame<T> {
  pub const fn new(dst_mac: [u8; 6], src_mac: [u8; 6], payload: T) -> Self {
    Self { dst_mac, src_mac, ether_type: default(), payload: ManuallyDrop::new(payload) }
  }
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Ipv4Packet<T: ?Sized> {
    // Version (4 bits) + IHL (4 bits) + TOS (8 bits) + Total Length (16 bits).
    version_ihl: u8, tos: u8, total_length: u16,
    // Identification (16 bits) + Flags (3 bits) + Fragment Offset (13 bits).
    identification: u16, flags_fragment_offset: u16,
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

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Option<T: ?Sized + OptionData> {
    // Option Type (8 bits) + Option Length (8 bits).
    option_type: T::Type, option_length: T::Length,
    // Option Data.
    option_data: ManuallyDrop<T>,
  }

  impl<T: OptionData> Option<T> {
    pub const fn new(option_data: T) -> Self {
      Self { option_type: default(), option_length: default(), option_data: ManuallyDrop::new(option_data) }
    }
  }

  pub trait OptionData: Data {
    type Type: Integer<Bits = u8>;
    type Length: Integer;
  }

  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct NoOperationOption;

  impl OptionData for NoOperationOption {
    type Type = u8c<0b00000001>;
    type Length = ();
  }

  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct EndOfOptionsListOption;

  impl OptionData for EndOfOptionsListOption {
    type Type = u8c<0b00000000>;
    type Length = ();
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct RecordRouteOption<T: ?Sized + Many<Item = Ipv4>> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Many<Item = Ipv4>> OptionData for RecordRouteOption<T> {
    type Type = u8c<0b00000111>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct StrictSourceRouteOption<T: ?Sized + Many<Item = Ipv4>> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Many<Item = Ipv4>> OptionData for StrictSourceRouteOption<T> {
    type Type = u8c<0b10001001>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct LooseSourceRouteOption<T: ?Sized + Many<Item = Ipv4>> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Many<Item = Ipv4>> OptionData for LooseSourceRouteOption<T> {
    type Type = u8c<0b10000011>;
    type Length = u8;
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Timestamp {
    ip_address: Ipv4,
    timestamp: u32,
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct TimestampOption<T: ?Sized + Many<Item = Timestamp>> {
    // Pointer (8 bits) + O-Flow (4 bits) + Flags (4 bits).
    pointer: u8,
    o_flow_flags: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Many<Item = Timestamp>> OptionData for TimestampOption<T> {
    type Type = u8c<0b01000100>;
    type Length = u8;
  }

  #[test]
  fn test() {
    let packet = EthernetFrame::new(
      [1, 2, 3, 4, 5, 6],
      [6, 5, 4, 3, 2, 1],
      Ipv4Packet {
        version_ihl: 0x45,
        tos: 0,
        total_length: 0x0014,
        identification: 0,
        flags_fragment_offset: 0,
        ttl: 0x40,
        protocol: 0x06,
        header_checksum: 0,
        src_ip: Ipv4([192, 168, 1, 2]),
        dst_ip: Ipv4([192, 168, 1, 1]),
        options_payload: ManuallyDrop::new(pack![
          Option::new(NoOperationOption),
          Option::new(RecordRouteOption {
            pointer: 2,
            ip_addresses: ManuallyDrop::new([Ipv4([192, 168, 1, 3]), Ipv4([192, 168, 1, 4])]),
          }),
          Option::new(EndOfOptionsListOption),
        ]),
      },
    );

    println!("{:#?}", packet);
  }
}

fn main() {}
