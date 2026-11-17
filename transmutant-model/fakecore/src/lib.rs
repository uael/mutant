//! `core`, but for two items, for a crate that says `extern crate fakecore as core;` (safe code):
//! - `mem::offset_of!` swaps the fields named `a` and `b`;
//! - `marker::PhantomData` is aligned to 16 bytes.
//!
//! Nothing that `#[derive(Data)]` generates may depend on them.
#![no_std]

#[doc(hidden)]
pub use core as real;
pub use core::*;

pub mod mem {
  pub use core::mem::*;

  #[macro_export]
  macro_rules! offset_of {
    (Self, a) => { $crate::real::mem::offset_of!(Self, b) };
    (Self, b) => { $crate::real::mem::offset_of!(Self, a) };
    ($($t:tt)*) => { $crate::real::mem::offset_of!($($t)*) };
  }

  pub use offset_of;
}

pub mod marker {
  pub use core::marker::*;

  #[repr(align(16))]
  pub struct PhantomData<T: ?Sized>(pub core::marker::PhantomData<T>);
}
