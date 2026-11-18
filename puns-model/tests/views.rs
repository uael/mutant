//! View enums against trying each of their variants in turn (`puns_model::views`): every pair of a
//! few types of 1 and 2 bytes, chains of bases, and unsized types for what `from_head` takes. The
//! full run (`views_full.rs`) has every pair of the types of 1 and 2 bytes of the model.

use core::num::NonZero;

use puns::{Pun, num::*};
use puns_model::{
  types::*,
  views::{Three, Two, check_view},
};

/// A slice of `bool`s after a `bool`: as many as there are, as `[u8]` takes.
#[derive(Pun)]
#[repr(C)]
pub struct TailBool {
  a: bool,
  t: [bool],
}

/// A number of elements that are not zero, and the elements.
#[derive(Pun)]
#[repr(C)]
pub struct Counted {
  n: u8,
  #[pun(len = n)]
  t: [NonZero<u8>],
}

/// A number of elements, and the elements: the same rule as `Counted`.
#[derive(Pun)]
#[repr(C)]
pub struct CountedAny {
  n: u8,
  #[pun(len = n)]
  t: [u8],
}

/// A size, and what follows.
#[derive(Pun)]
#[repr(C)]
#[pun(size = size)]
pub struct Sized1 {
  size: u8,
  t: [u8],
}

/// The same, aligned to 2: `from_head` takes its trailing padding, which `Sized1` does not.
#[derive(Pun)]
#[repr(C, align(2))]
#[pun(size = size)]
pub struct Sized2 {
  size: u8,
  t: [u8],
}

#[test]
#[cfg(not(miri))]
fn views() {
  // Every pair of a few types of 1 byte: inclusions (`u8c<7>` in `u8`, `Third` in `ThirdSoft`
  // and back: the same rule), none (`Fifth`), requirements (`SoftBit`), keys apart (`u8c<7>`
  // and `bool`).
  puns_model::view_pairs!(&[0, 1, 2], true;
    u8, bool, NonZero<u8>, u8c<7>, SoftBit, Third, ThirdSoft, Fifth,
  );
  // Of 2 bytes: a base less aligned than its variant (the address is checked from the base), an
  // enum, a rule over the bytes.
  puns_model::view_pairs!(&[1, 2, 3], true; u16, [u8; 2], [bool; 2], Aligned, E16, NotOneOne);
  // Chains: a base that is the base of another, a base shared by two variants.
  check_view::<Three<u8c<1>, NonZero<u8>, u8>>(&[0, 1, 2], true);
  check_view::<Three<u8c<1>, u8c<2>, NonZero<u8>>>(&[0, 1, 2], true);
  check_view::<Three<Third, Fifth, u8>>(&[0, 1, 2], true);
  check_view::<Three<u16bec<0x1234>, NonZero<u16>, [u8; 2]>>(&[1, 2, 3], true);
  // Unsized types, whose extents `from_head` takes from a rule, or as many elements as fit.
  let lengths = &[0, 1, 2, 3, 4, 5];
  check_view::<Two<[bool], [u8]>>(lengths, true);
  check_view::<Two<TailBool, [u8]>>(lengths, true);
  check_view::<Two<[u16], [u8]>>(lengths, true);
  check_view::<Two<Counted, CountedAny>>(lengths, true);
  check_view::<Two<Counted, [u8]>>(lengths, true);
  check_view::<Two<Sized2, Sized1>>(lengths, true);
  check_view::<Three<Counted, CountedAny, [u8]>>(lengths, true);
  check_view::<Three<[u8; 2], [u16], [u8]>>(lengths, true);
  puns_model::finish("views");
}

/// Under Miri, which is there for undefined behaviour (the native run has the verdicts): a view
/// for each unsafe path of `puns::view`, over the small alphabets of Miri. A variant decided from
/// its base (`Third` from `u8`), and the address checked from a base less aligned (`u16` from
/// `[u8; 2]`); a variant rebuilt from its state, unsized (`[u8]` after `[bool]`, its base, failed)
/// and sized (`u8` in a chain); `from_head` from a base of the same extent, with and without
/// trailing padding (`Counted`, `Sized2`).
#[test]
#[cfg(miri)]
fn views() {
  check_view::<Two<Third, u8>>(&[1], true);
  check_view::<Two<u16, [u8; 2]>>(&[2], true);
  check_view::<Two<[bool], [u8]>>(&[0, 1, 2], true);
  check_view::<Three<u8c<1>, NonZero<u8>, u8>>(&[1], true);
  check_view::<Two<Counted, CountedAny>>(&[1, 2, 3], true);
  check_view::<Two<Sized2, Sized1>>(&[2, 3], true);
  puns_model::finish("views");
}
