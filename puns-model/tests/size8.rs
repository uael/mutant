//! Every ordered pair of the types of 8 bytes of the model.

use puns_model::types::*;

#[test]
fn size8() {
  puns_model::all_pairs!(usize::MAX;
    u64, [u8; 8], [bool; 8], [u32; 2], (u32, u32), CTagged, CTaggedB, [Shape; 2], [Drawing; 2],
    (u16, u8, bool, u32), OneOfTwo,
  );
  puns_model::finish("8 bytes");
}
