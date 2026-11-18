// A byte that a target requires, of which a source knows nothing: not a `u8`.
use core::mem::MaybeUninit;

use puns::layout::{Endian, Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Zero(MaybeUninit<u8>);

// SAFETY: 0 is a value of a `MaybeUninit<u8>`, which is known to be anything.
unsafe impl puns::Scalar for Zero {
  const SPEC: Spec = Spec::ranges::<u8>(Endian::Little, &[(0, 0)]).maybe_uninit();
}

fn main() {
  let _ = puns::try_pun::<Zero, bool>(Zero(MaybeUninit::uninit()));
}
