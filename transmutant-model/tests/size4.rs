//! Every ordered pair of the types of 4 bytes of the model.

use core::num::NonZero;

use transmutant::num::*;
use transmutant_model::types::*;

#[test]
#[ignore = "full model run, a minute in debug: CI runs it with `--profile model`"]
fn size4() {
  transmutant_model::all_pairs!(usize::MAX;
    u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
    u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag, CEnum,
    Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2],
  );
  transmutant_model::finish("4 bytes");
}
