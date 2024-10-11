use crate::Literal;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Sized;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Unsized;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MaybeUnsized {
  Sized,
  Unsized,
}

pub trait Sizing: Literal<MaybeUnsized> + sealed::Sizing {}

mod sealed {
  pub trait Sizing {}
}

// impl
const _: () = {
  use crate::Default;

  impl sealed::Sizing for Sized {}
  impl sealed::Sizing for Unsized {}

  impl Sizing for Sized {}
  impl Sizing for Unsized {}

  impl Literal<MaybeUnsized> for Sized {
    const LITERAL: MaybeUnsized = MaybeUnsized::Sized;
  }

  impl Literal<MaybeUnsized> for Unsized {
    const LITERAL: MaybeUnsized = MaybeUnsized::Unsized;
  }

  impl Default for MaybeUnsized {
    const DEFAULT: Self = Sized::LITERAL;
  }

  impl MaybeUnsized {
    #[inline]
    pub const fn is_sized(&self) -> bool {
      matches!(self, MaybeUnsized::Sized)
    }

    #[inline]
    pub const fn is_unsized(&self) -> bool {
      matches!(self, MaybeUnsized::Unsized)
    }
  }
};
