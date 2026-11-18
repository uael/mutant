//! The sample of the model check that runs on every push: a selection of pairs that runs what
//! Miri is there to see, and natively every pair of a few types of each size as well.
//!
//! Under Miri the point is not the verdicts (the full native run checks them all, exhaustively,
//! see `size*.rs` and `parts*.rs`, behind the `full` feature) but undefined behaviour: the unsafe
//! paths of the library that the model reaches (`try_pun` and `try_pun_ref` here, with values made
//! by the oracle, `make`), once or a few times on each class of layout. Each pair is there for one
//! of them, with the few values of the alphabet of `alphabet` under Miri:
//!
//! | Path                                                   | Pair                               |
//! |--------------------------------------------------------|------------------------------------|
//! | nothing to check (a proven pun, through `try_*`)       | `bool -> u8`, `[bool; 2] -> u16`   |
//! | a scalar checked: accepted, rejected for validity      | `u8 -> bool`, `u16 -> (u8, bool)`  |
//! | a rule checked at run time                             | `u16 -> NotOneOne`                 |
//! | a soft restriction (`valid(..)`) checked               | `u8 -> SoftBit`                    |
//! | the address checked (aligned here, see `misaligned`)   | `[u8; 2] -> u16`                   |
//! | an enum with fields checked: the tag, then the variant | `u32 -> Shape`                     |
//! | a target with padding, read from initialized bytes     | `[u8; 4] -> PadMid`                |
//!
//! A pair is not there for what the analysis proves of it: an enum proven variant by variant
//! (`Shape -> Drawing`) runs the same code as `bool -> u8`, nothing, and an enum checked from an
//! enum (`Drawing -> Shape`) the same as from `u32`. Those are verdicts: the native run has them.
//!
//! `misaligned` rejects an address for alignment (`try_pun_ref`, `from_tail`). The parts
//! (`head*`, `tail*`) are in `parts.rs`, slices and unsized tails in `dst.rs`. What the model does
//! not run, `pun`, `pun_mut`, `try_pun_mut`, `from_head_mut`, `from_tail_mut`, the regions and
//! the view enums, the tests of `puns` run under Miri.

#[cfg(not(miri))]
use core::num::NonZero;

use puns::{Error, hook};
use puns_model::types::*;

#[test]
fn sample() {
  // The selection of the table above: natively as well, so that it is checked on every push
  // with the full alphabets.
  puns_model::pairs!(check_pair, 3;
    (bool, u8), ([bool; 2], u16), (u8, bool), (u16, (u8, bool)), (u16, NotOneOne), (u8, SoftBit),
    ([u8; 2], u16), (u32, Shape), ([u8; 4], PadMid),
  );
  // Natively, every pair of a few types of each size as well: about a hundred pairs, which is
  // what builds in seconds (each pair is an instance of the analysis). Not compiled under Miri.
  #[cfg(not(miri))]
  {
    puns_model::all_pairs!(8; u8, bool, NonZero<u8>, E37, SoftBit, Zero, core::mem::MaybeUninit<u8>);
    puns_model::all_pairs!(6; u16, [bool; 2], (u8, bool), Aligned, Opt, NotOneOne, ByteZero);
    puns_model::all_pairs!(4; [u8; 3], Wide, Triple);
    puns_model::all_pairs!(4; u32, char, PadMid, Shape, Drawing);
  }
  puns_model::finish("sample");
}

/// An address that is not aligned for the target, whatever where the allocator put the bytes.
#[test]
fn misaligned() {
  let words = [0x0102_0304u32, 0x0506_0708];
  let bytes: &[u8; 8] = puns::pun_ref(&words);
  let got = hook::try_pun_ref::<[u8], u32>(&bytes[1..5]).expect("compiles");
  assert_eq!(got, Err(Error::Alignment));
  let got = hook::from_tail::<u32>(&bytes[..7]).expect("compiles").map(|(_, w)| *w);
  assert_eq!(got, Err(Error::Alignment));
  let got = hook::from_tail::<u32>(&bytes[..]).expect("compiles").map(|(rest, w)| (rest.len(), *w));
  assert_eq!(got, Ok((4, 0x0506_0708)));
}
