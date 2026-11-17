//! A sample of the model check that Miri can afford: some pairs of each size, a few values each,
//! with the values made by the oracle (`make`) checked by Miri as well.

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
fn sample() {
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
