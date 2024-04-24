use core::fmt::Debug;
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
    version_ihl: u8, tos: u8, total_length: Size<u16>,
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

    pub const fn options(&self) -> &[u8] {
      self.options_payload().0
    }

    pub const fn payload(&self) -> &[u8] {
      self.options_payload().1
    }
  }

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, Copy, Clone, PartialEq, Eq)]
  pub struct Ipv4Option<T: ?Sized + OptionData> {
    // Option Type (8 bits) + Option Length (8 bits).
    option_type: T::Type, option_length: T::Length,
    // Option Data.
    option_data: ManuallyDrop<T>,
  }

  impl<T: OptionData<Length = ()>> Ipv4Option<T> {
    pub const fn new_sized(option_data: T) -> Self {
      Self {
        option_type: default(),
        option_length: (),
        option_data: ManuallyDrop::new(option_data),
      }
    }
  }

  impl<T: OptionData<Length = Size<u8>>> Ipv4Option<T> {
    pub const fn new(option_data: T) -> Self {
      Self {
        option_type: default(),
        option_length: transmute(2 + min_size_of::<T>() as u8),
        option_data: ManuallyDrop::new(option_data),
      }
    }
  }

  impl<T: ?Sized + OptionData + Debug> Debug for Ipv4Option<T>
  where
    T::Type: Debug,
    T::Length: Debug,
  {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
      f.debug_struct(core::any::type_name::<Self>())
        .field("option_type", &unaligned_ref!(self.option_type))
        .field("option_length", &unaligned_ref!(self.option_length))
        .field("option_data", &unaligned_ref!(self.option_data))
        .finish()
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
  pub struct EndOfOptions;

  impl OptionData for EndOfOptions {
    type Type = u8c<0b00000000>;
    type Length = ();
  }

  #[repr(C, packed)]
  #[derive(Data, Copy, Clone, PartialEq, Eq)]
  pub struct RecordRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for RecordRouteOption<T> {
    type Type = u8c<0b00000111>;
    type Length = Size<u8>;
  }

  impl<T: ?Sized + Data<Item = Ipv4> + Debug> Debug for RecordRouteOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
      f.debug_struct(core::any::type_name::<Self>())
        .field("pointer", &self.pointer)
        .field("ip_addresses", &unaligned_ref!(self.ip_addresses))
        .finish()
    }
  }

  #[repr(C, packed)]
  #[derive(Data, Copy, Clone, PartialEq, Eq)]
  pub struct StrictSourceRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4> + Debug> Debug for StrictSourceRouteOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
      f.debug_struct(core::any::type_name::<Self>())
        .field("pointer", &self.pointer)
        .field("ip_addresses", &unaligned_ref!(self.ip_addresses))
        .finish()
    }
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for StrictSourceRouteOption<T> {
    type Type = u8c<0b10001001>;
    type Length = Size<u8>;
  }

  #[repr(C, packed)]
  #[derive(Data, Copy, Clone, PartialEq, Eq)]
  pub struct LooseSourceRouteOption<T: ?Sized + Data<Item = Ipv4> = [Ipv4]> {
    // Pointer (8 bits).
    pointer: u8,
    // IP Addresses.
    ip_addresses: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Ipv4> + Debug> Debug for LooseSourceRouteOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
      f.debug_struct(core::any::type_name::<Self>())
        .field("pointer", &self.pointer)
        .field("ip_addresses", &unaligned_ref!(self.ip_addresses))
        .finish()
    }
  }

  impl<T: ?Sized + Data<Item = Ipv4>> OptionData for LooseSourceRouteOption<T> {
    type Type = u8c<0b10000011>;
    type Length = Size<u8>;
  }

  #[repr(C, packed)]
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Timestamp {
    ip_address: Ipv4,
    timestamp: u32,
  }

  #[rustfmt::skip]
  #[repr(C, packed)]
  #[derive(Data, Copy, Clone, PartialEq, Eq)]
  pub struct TimestampOption<T: ?Sized + Data<Item = Timestamp> = [Timestamp]> {
    // Pointer (8 bits) + O-Flow (4 bits) + Flags (4 bits).
    pointer: u8, o_flow_flags: u8,
    // IP Addresses.
    timestamps: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Timestamp> + Debug> Debug for TimestampOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
      f.debug_struct(core::any::type_name::<Self>())
        .field("pointer", &self.pointer)
        .field("o_flow_flags", &self.o_flow_flags)
        .field("timestamps", &unaligned_ref!(self.timestamps))
        .finish()
    }
  }

  impl<T: ?Sized + Data<Item = Timestamp>> OptionData for TimestampOption<T> {
    type Type = u8c<0b01000100>;
    type Length = Size<u8>;
  }

  pub fn process(bytes: &[u8]) -> Result<&[u8], Error> {
    let (frame, bytes): (&EthernetFrame<Ipv4Packet<[u8]>>, _) = unpack(bytes)?;
    let ipv4 = unaligned_ref!(frame.payload);
    let mut options = ipv4.options();
    while !options.is_empty() {
      options = if let Ok((option, options)) = unpack::<Ipv4Option<NoOperationOption>>(options) {
        println!("option: {:#?}", option);
        options
      } else if let Ok((option, options)) = unpack::<Ipv4Option<EndOfOptions>>(options) {
        println!("option: {:#?}", option);
        options
      } else if let Ok((option, options)) = unpack::<Ipv4Option<RecordRouteOption>>(options) {
        println!("option: {:#?}", option);
        options
      } else if let Ok((option, options)) = unpack::<Ipv4Option<StrictSourceRouteOption>>(options) {
        println!("option: {:#?}", option);
        options
      } else if let Ok((option, options)) = unpack::<Ipv4Option<LooseSourceRouteOption>>(options) {
        println!("option: {:#?}", option);
        options
      } else if let Ok((option, options)) = unpack::<Ipv4Option<TimestampOption>>(options) {
        println!("option: {:#?}", option);
        options
      } else {
        return Err(Error::InvalidBitPattern);
      };
    }
    Ok(bytes)
  }

  #[test]
  fn option_test() {
    let options = pack!(
      Ipv4Option::new_sized(NoOperationOption),
      Ipv4Option::new(RecordRouteOption {
        pointer: 2,
        ip_addresses: ManuallyDrop::new([Ipv4([192, 168, 1, 3]), Ipv4([192, 168, 1, 4])]),
      }),
      Ipv4Option::new(TimestampOption {
        pointer: 1,
        o_flow_flags: 0,
        timestamps: ManuallyDrop::new([
          Timestamp { ip_address: Ipv4([192, 168, 1, 3]), timestamp: 123234234 },
          Timestamp { ip_address: Ipv4([192, 168, 1, 4]), timestamp: 125243563 }
        ])
      })
    );
    let bytes: &[u8] = transmute_ref(&options);
    let (option, bytes) = unpack::<Ipv4Option<NoOperationOption>>(bytes).unwrap();
    println!("option: {:#?}", option);
    let (option, bytes) = unpack::<Ipv4Option<RecordRouteOption>>(bytes).unwrap();
    println!("option: {:#?}", option);
    let (option, bytes) = unpack::<Ipv4Option<TimestampOption>>(bytes).unwrap();
    println!("option: {:#?}", option);
    println!("bytes: {:#?}", bytes);
  }

  #[test]
  fn test() {
    let packet = EthernetFrame::new(
      [1, 2, 3, 4, 5, 6],
      [6, 5, 4, 3, 2, 1],
      Ipv4Packet {
        version_ihl: 5 + 8,
        tos: 0,
        total_length: transmute(((5u16 + 8) * 4) + 0),
        identification: 0,
        flags_fragment_offset: 0,
        ttl: 0x40,
        protocol: 0x06,
        header_checksum: 0,
        src_ip: Ipv4([192, 168, 1, 2]),
        dst_ip: Ipv4([192, 168, 1, 1]),
        options_payload: ManuallyDrop::new(pack![
          Ipv4Option::new_sized(NoOperationOption),
          Ipv4Option::new(RecordRouteOption {
            pointer: 2,
            ip_addresses: ManuallyDrop::new([Ipv4([192, 168, 1, 3]), Ipv4([192, 168, 1, 4])]),
          }),
          Ipv4Option::new(TimestampOption {
            pointer: 1,
            o_flow_flags: 0,
            timestamps: ManuallyDrop::new([
              Timestamp { ip_address: Ipv4([192, 168, 1, 3]), timestamp: 123234234 },
              Timestamp { ip_address: Ipv4([192, 168, 1, 4]), timestamp: 125243563 }
            ])
          })
        ]),
      },
    );

    println!("{:#?}", packet);

    process(transmute_ref(&packet)).unwrap();
  }
}

fn main() {}
