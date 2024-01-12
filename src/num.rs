use crate::Data;
use core::fmt::{Debug, Display, Formatter};
use core::num::{NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Ordering {
  Lsb = 1,
  Msb,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct IntegerSpec {
  pub ordering: Option<Ordering>,
  pub layout: Option<u64>,
}

pub unsafe trait Integer: Copy + Data {
  const SPEC: IntegerSpec;
}

macro_rules! impl_num {
  ($ty:ident $raw:ident $bytes:literal $ordering:ident $a:ident $b:ident $c:ident) => {
    #[derive(Eq, PartialEq, Copy, Clone, Hash)]
    #[allow(non_camel_case_types)]
    #[repr(transparent)]
    pub struct $ty([u8; $bytes]);

    unsafe impl Integer for $ty {
      const SPEC: IntegerSpec = IntegerSpec::new().with_ordering(Ordering::$ordering);
    }

    impl $ty {
      pub const fn $a(bytes: [u8; $bytes]) -> Self {
        Self(bytes)
      }

      pub const fn $b(bytes: [u8; $bytes]) -> Self {
        Self::$a($raw::$b(bytes).$c())
      }

      pub const fn from_ne_bytes(bytes: [u8; $bytes]) -> Self {
        Self::$a($raw::from_ne_bytes(bytes).$c())
      }

      pub const fn from_raw(uint: $raw) -> Self {
        Self::$a(uint.$c())
      }

      pub const fn into_raw(self) -> $raw {
        $raw::$a(self.0)
      }
    }

    impl From<$raw> for $ty {
      fn from(value: $raw) -> Self {
        Self::from_raw(value)
      }
    }

    impl Into<$raw> for $ty {
      fn into(self) -> $raw {
        self.into_raw()
      }
    }

    impl Debug for $ty {
      fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        <$raw as Debug>::fmt(&self.into_raw(), f)
      }
    }

    impl Display for $ty {
      fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        <$raw as Display>::fmt(&self.into_raw(), f)
      }
    }
  };
}

impl_num!(u16le u16 2 Lsb from_le_bytes from_be_bytes to_le_bytes);
impl_num!(u32le u32 4 Lsb from_le_bytes from_be_bytes to_le_bytes);
impl_num!(u64le u64 8 Lsb from_le_bytes from_be_bytes to_le_bytes);
impl_num!(u16be u16 2 Msb from_be_bytes from_le_bytes to_be_bytes);
impl_num!(u32be u32 4 Msb from_be_bytes from_le_bytes to_be_bytes);
impl_num!(u64be u64 8 Msb from_be_bytes from_le_bytes to_be_bytes);

impl IntegerSpec {
  #[inline(always)]
  pub const fn new() -> Self {
    Self { ordering: None, layout: None }
  }

  #[inline(always)]
  pub const fn with_ordering(self, ordering: Ordering) -> Self {
    Self { ordering: Some(ordering), layout: self.layout }
  }

  #[inline(always)]
  pub const fn with_layout(self, layout: u64) -> Self {
    Self { ordering: self.ordering, layout: Some(layout) }
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
      uint |= (byte as u64)
        << 8
          * match self.ordering() {
            Ordering::Lsb => bit,
            Ordering::Msb => bytes.len() - bit,
          };
      bit += 1;
    }
    uint
  }
}

unsafe impl Integer for bool {
  const SPEC: IntegerSpec = IntegerSpec::new().with_layout(0b1);
}

unsafe impl Integer for u8 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for u16 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for u32 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for u64 {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for usize {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for Option<NonZeroU8> {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for Option<NonZeroU16> {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for Option<NonZeroU32> {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for Option<NonZeroU64> {
  const SPEC: IntegerSpec = IntegerSpec::new();
}

unsafe impl Integer for Option<NonZeroUsize> {
  const SPEC: IntegerSpec = IntegerSpec::new();
}
