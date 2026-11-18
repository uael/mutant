//! Every ordered pair of the types of 1 byte of the model.

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
#[cfg_attr(miri, ignore = "too slow under Miri, which runs `miri_sample`")]
fn size1() {
  puns_model::all_pairs!(usize::MAX;
    u8, i8, bool, NonZero<u8>, Option<NonZero<u8>>, u8c<0>, u8c<7>, u8c<255>, u8in<0, 0x7f>,
    u8in<1, 1>, u8in<0x80, 0xff>, u8in<2, 5>, E01, E37, Eff, Eneg, Erust, [u8; 1], [bool; 1],
    (bool,), Flag, Byte, Ascii, SoftBit, SoftTrue, Third, core::mem::ManuallyDrop<bool>, Zero,
    core::mem::MaybeUninit<u8>, MaybeFlag, NzOpt, ChoiceU,
  );
  puns_model::finish("1 bytes");
}
