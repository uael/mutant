//! A sample of the model check, natively a product of some pairs of each size, and under Miri a
//! selection that runs what Miri is there to see.
//!
//! Under Miri the point is not the verdicts (the native run checks them all, exhaustively, see
//! `size*.rs` and `parts*.rs`) but undefined behaviour: the unsafe paths of the library that the
//! model reaches (`try_pun` and `try_pun_ref` here, with values made by the oracle, `make`), once
//! or a few times on each class of layout. Each pair is there for one of them, with the few values
//! of the alphabet of `alphabet` under Miri:
//!
//! | Path                                                   | Pair                               |
//! |--------------------------------------------------------|------------------------------------|
//! | nothing to check (a proven pun, through `try_*`)       | `bool -> u8`, `[bool; 2] -> u16`   |
//! | a scalar checked: accepted, rejected for validity      | `u8 -> bool`, `u16 -> (u8, bool)`  |
//! | a rule checked at run time                             | `u16 -> NotOneOne`                 |
//! | a soft restriction (`valid(..)`) checked               | `u8 -> SoftBit`                    |
//! | the address checked (aligned here, see `misaligned`)   | `[u8; 2] -> u16`                   |
//! | enums with fields: variant by variant, nothing checked | `Shape -> Drawing`                 |
//! | enums with fields checked: the tag, then the variant   | `Drawing -> Shape`, `u32 -> Shape` |
//! | a target with padding, read from initialized bytes     | `[u8; 4] -> PadMid`                |
//!
//! `misaligned` rejects an address for alignment (`try_pun_ref`, `from_tail`). The parts
//! (`head*`, `tail*`) are in `parts.rs`, slices and unsized tails in `dst.rs`. What the model does
//! not run, `pun`, `pun_mut`, `try_pun_mut`, `from_head_mut`, `from_tail_mut`, the regions and
//! the view enums, the tests of `puns` run under Miri.

use core::num::NonZero;

use puns::{Error, hook, num::*};
use puns_model::types::*;

#[test]
fn sample() {
  if cfg!(miri) {
    puns_model::pairs!(check_pair, 3;
      (bool, u8), ([bool; 2], u16), (u8, bool), (u16, (u8, bool)), (u16, NotOneOne), (u8, SoftBit),
      ([u8; 2], u16), (Shape, Drawing), (Drawing, Shape), (u32, Shape), ([u8; 4], PadMid),
    );
    puns_model::finish("sample, under Miri");
    return;
  }
  puns_model::all_pairs!(8;
    u8, bool, NonZero<u8>, u8in<0, 0x7f>, E37, Eneg, SoftBit, Third,
  );
  puns_model::all_pairs!(6;
    u16, [bool; 2], u16bec<0x1234>, (u8, bool), Aligned, Opt, OptFlag, OptSoftBit, Skewed, EitherC,
    NotOneOne,
  );
  puns_model::all_pairs!(4;
    [u8; 3], PackedBoolNz, Wide, Lit, Triple,
  );
  puns_model::all_pairs!(4;
    u32, char, PadMid, PadEnd, Shape, Drawing, CU8, Tag16, ReadsPad,
  );
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
