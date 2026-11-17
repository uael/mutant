//! Parts of values (`head`, `tail`, `head_mut`, `tail_mut`...): a sample that Miri can afford,
//! of each size of source, with targets of every size below it. Every pair of the model is run by
//! `parts2`, `parts3`, `parts4` and `parts8`.

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
fn parts_sample() {
  puns_model::part_pairs!(4;
    [u16, [bool; 2], (u8, bool), NotOneOne, Opt, OptFlag, Skewed, Aligned, PackedU16, OptSoftBit];
    [u8, bool, NonZero<u8>, u8in<0, 0x7f>, Third, SoftBit]
  );
  puns_model::part_pairs!(4;
    [[u8; 3], Wide, Triple, PackedBoolNz, PackedU16Bool];
    [u8, bool, [u8; 2], u16, (u8, bool), Opt, NotOneOne]
  );
  puns_model::part_pairs!(2;
    [u32, PadMid, PadEnd, Shape, Drawing, ReadsPad, Tag16, CU8];
    [u8, bool, u16, [u8; 2], [u8; 3], Opt, Triple]
  );
  puns_model::part_pairs!(2; [CTagged, [Shape; 2], (u16, u8, bool, u32)]; [u32, Shape, bool]);
  puns_model::finish("parts, sample");
}
