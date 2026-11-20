// A byte that a target requires, of which a source knows nothing: not a `u8`.
use core::mem::MaybeUninit;

use puns::layout::{Endian, Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Zero(MaybeUninit<u8>);

// SAFETY: A `Zero` may hold any byte, as `Spec::maybe_uninit` allows, and 0 is a valid one.
unsafe impl puns::Scalar for Zero {
  const SPEC: Spec = Spec::ranges::<u8>(Endian::Little, &[(0, 0)]).maybe_uninit();
}

fn main() {
  let _ = puns::try_pun::<Zero, bool>(Zero(MaybeUninit::uninit()));
}
