use crate::Literal;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Le;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Be;

#[cfg(target_endian = "little")]
pub type Ne = Le;
#[cfg(target_endian = "big")]
pub type Ne = Be;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Endian {
  Le,
  Be,
}

pub trait Endianness: Literal<Endian> + sealed::Endianness {}

mod sealed {
  pub trait Endianness {}
}

// impl
const _: () = {
  use crate::Default;

  impl sealed::Endianness for Le {}
  impl sealed::Endianness for Be {}

  impl Endianness for Le {}
  impl Endianness for Be {}

  impl Literal<Endian> for Le {
    const LITERAL: Endian = Endian::Le;
  }

  impl Literal<Endian> for Be {
    const LITERAL: Endian = Endian::Be;
  }

  impl Default for Endian {
    const DEFAULT: Self = Ne::LITERAL;
  }

  impl Endian {
    #[inline]
    pub const fn is_native(&self) -> bool {
      match self {
        Endian::Le => cfg!(target_endian = "little"),
        Endian::Be => cfg!(target_endian = "big"),
      }
    }

    #[inline]
    pub const fn is_little(&self) -> bool {
      matches!(self, Endian::Le)
    }

    #[inline]
    pub const fn is_big(&self) -> bool {
      matches!(self, Endian::Be)
    }
  }
};
