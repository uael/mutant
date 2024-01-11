use crate::DataType;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering {
  Lsb = 1, Msb
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct IntegerSpec {
  pub ordering: Option<Ordering>,
  pub bitsmask: Option<u64>,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct u16le([u8; 2]);

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct u32le([u8; 4]);

pub trait Integer: Copy + DataType {
  const SPEC: IntegerSpec;
}

impl u16le {
  pub const fn from_le_bytes(bytes: [u8; 2]) -> Self {
    Self(bytes)
  }
}

impl u32le {
  pub const fn from_le_bytes(bytes: [u8; 4]) -> Self {
    Self(bytes)
  }
}

impl IntegerSpec {
  #[inline(always)]
  pub const fn new() -> Self {
    Self { ordering: None, bitsmask: None }
  }

  #[inline(always)]
  pub const fn with_ordering(self, ordering: Ordering) -> Self {
    Self { ordering: Some(ordering), bitsmask: self.bitsmask }
  }

  #[inline(always)]
  pub const fn with_bitsmask(self, bitsmask: u64) -> Self {
    Self { ordering: self.ordering, bitsmask: Some(bitsmask) }
  }

  #[inline(always)]
  pub const fn ordering(self) -> Ordering {
    match self.ordering {
      Some(ordering) => ordering,
      #[cfg(target_endian = "little")]
      _ => Ordering::Lsb,
      #[cfg(target_endian = "big")]
      _ => Ordering::Msb,
    }
  }

  #[inline(always)]
  pub const fn load(self, bytes: &[u8]) -> u64 {
    let (mut bit, mut uint) = (0, 0);
    assert!(bytes.len() <= 8);
    while bit < bytes.len() {
      let byte = bytes[bit];
      uint |= (byte as u64) << 8 * match self.ordering() {
        Ordering::Lsb => bit,
        Ordering::Msb => bytes.len() - bit,
      };
      bit += 1;
    }
    uint
  }
}

impl Integer for u8 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u16 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u32 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u64 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

impl Integer for u16le {
  const SPEC: IntegerSpec = IntegerSpec::new().with_ordering(Ordering::Lsb);
}

impl Integer for u32le {
  const SPEC: IntegerSpec = IntegerSpec::new().with_ordering(Ordering::Lsb);
}
