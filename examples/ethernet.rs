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

  fn validate(&self) -> Result<usize, Error>;
}

impl<T: EthernetPacket> EthernetFrame<T> {
  pub const fn new(dst_mac: [u8; 6], src_mac: [u8; 6], payload: T) -> Self {
    Self { dst_mac, src_mac, ether_type: default(), payload: ManuallyDrop::new(payload) }
  }
}

impl<T: ?Sized + EthernetPacket> EthernetFrame<T> {
  pub fn try_split(&self) -> Result<(&Self, &[u8]), Error> {
    let mid = min_size_of::<EthernetFrame<[u8]>>() + unaligned_ref!(self.payload).validate()?;
    match try_split_at(self, mid) {
      Some((frame, rest)) => Ok((frame, transmute_ref(rest))),
      None => Err(Error::InvalidBitPattern),
    }
  }
}

impl EthernetPacket for [u8] {
  type Type = u16;

  fn validate(&self) -> Result<usize, Error> {
    Ok(self.len())
  }
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

    fn validate(&self) -> Result<usize, Error> {
      if self.ihl() < 20 || self.ihl() > 60 {
        return Err(Error::InvalidBitPattern);
      }
      if self.total_length < self.ihl() as u16 {
        return Err(Error::InvalidBitPattern);
      }
      Ok(self.total_length as usize)
    }
  }

  impl<T: ?Sized + Data> Ipv4Packet<T> {
    pub const fn version(&self) -> u8 {
      self.version_ihl >> 4
    }

    pub const fn ihl(&self) -> u8 {
      (self.version_ihl & 0x0F) * 4
    }

    pub const fn options_payload(&self) -> (&[u8], &[u8]) {
      transmute_ref::<_, [u8]>(unaligned_ref!(self.options_payload))
        .split_at(self.ihl() as usize - 20)
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct Ipv4Option<T: ?Sized + OptionData> {
    // Option Type (8 bits) + Option Length (8 bits).
    option_type: T::Type, option_length: T::Length,
    // Option Data.
    option_data: ManuallyDrop<T>,
  }

  pub fn option_sized<T: OptionData<Length = ()>>(bytes: &[u8]) -> Option<(&T, &[u8])> {
    match try_transmute_ref::<_, Ipv4Option<T>>(&bytes[..1]) {
      Ok(option) => Some((unaligned_ref!(option.option_data), &bytes[1..])),
      Err(_) => None,
    }
  }

  pub fn option<T: ?Sized + OptionData<Length = u8>>(
    bytes: &[u8],
  ) -> Result<Option<(&T, &[u8])>, Error> {
    match try_transmute_ref::<_, Ipv4Option<T>>(bytes) {
      Ok(option) => {
        let (option, bytes) = option.try_split()?;
        Ok(Some((unaligned_ref!(option.option_data), bytes)))
      }
      Err(_) => Ok(None),
    }
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

  impl<T: OptionData<Length = u8>> Ipv4Option<T> {
    pub const fn new(option_data: T) -> Self {
      Self {
        option_type: default(),
        option_length: 2 + min_size_of::<T>() as u8,
        option_data: ManuallyDrop::new(option_data),
      }
    }
  }

  impl<T: ?Sized + OptionData<Length = u8>> Ipv4Option<T> {
    pub const fn try_split(&self) -> Result<(&Self, &[u8]), Error> {
      if self.option_length < 2 {
        return Err(Error::InvalidBitPattern);
      }
      if self.option_length as usize > size_of_val(self) {
        return Err(Error::InvalidBitPattern);
      }
      match try_split_at(self, self.option_length as usize) {
        Some((option, rest)) => Ok((option, transmute_ref(rest))),
        None => Err(Error::InvalidBitPattern),
      }
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct RecordRouteOption<T: ?Sized + Data<Item = Ipv4>> {
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct StrictSourceRouteOption<T: ?Sized + Data<Item = Ipv4>> {
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct LooseSourceRouteOption<T: ?Sized + Data<Item = Ipv4>> {
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
  #[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
  pub struct TimestampOption<T: ?Sized + Data<Item = Timestamp>> {
    // Pointer (8 bits) + O-Flow (4 bits) + Flags (4 bits).
    pointer: u8, o_flow_flags: u8,
    // IP Addresses.
    timestamps: ManuallyDrop<T>,
  }

  impl<T: ?Sized + Data<Item = Timestamp>> OptionData for TimestampOption<T> {
    type Type = u8c<0b01000100>;
    type Length = u8;
  }

  pub fn process(frame: &[u8]) -> Result<&[u8], Error> {
    let frame: &EthernetFrame<Ipv4Packet<[u8]>> = try_transmute_ref(frame)?;
    let (frame, rest) = frame.try_split()?;
    let ipv4 = unaligned_ref!(frame.payload);
    let mut options = ipv4.options();
    while !options.is_empty() {
      options = if let Some((opt, rest)) = option_sized::<NoOperationOption>(options) {
        println!("option: {:#?}", opt);
        rest
      } else if let Some((opt, rest)) = option_sized::<EndOfOptions>(options) {
        println!("option: {:#?}", opt);
        rest
      } else if let Some((opt, rest)) = option::<RecordRouteOption<[Ipv4]>>(options)? {
        println!("option: {:#?}", core::any::type_name_of_val(opt));
        rest
      } else if let Some((opt, rest)) = option::<StrictSourceRouteOption<[Ipv4]>>(options)? {
        println!("option: {:#?}", core::any::type_name_of_val(opt));
        rest
      } else if let Some((opt, rest)) = option::<LooseSourceRouteOption<[Ipv4]>>(options)? {
        println!("option: {:#?}", core::any::type_name_of_val(opt));
        rest
      } else if let Some((opt, rest)) = option::<TimestampOption<[Timestamp]>>(options)? {
        println!("option: {:#?}", core::any::type_name_of_val(opt));
        rest
      } else {
        return Err(Error::InvalidBitPattern);
      };
    }
    Ok(rest)
  }

  #[test]
  fn test() {
    let packet = EthernetFrame::new(
      [1, 2, 3, 4, 5, 6],
      [6, 5, 4, 3, 2, 1],
      Ipv4Packet {
        version_ihl: 5 + 8,
        tos: 0,
        total_length: ((5 + 8) * 4) + 0,
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
