//! The exhaustive model check of self-relative pointers: every pair of every type of the model.
//! Behind the feature `full`: `cargo test --profile model -p puns-model --features full`.

#![cfg(feature = "full")]
#![cfg(not(miri))]

use puns_model::pointers::{report, types::*};

#[test]
fn every_pair() {
  puns_model::pointer_pairs!(check_pair;
    Flags, Bytes, FlagList, ByteList, ToFlag, ToByte, MaybeByte, ToByteBe, ToWord, ToPair, ToBytes,
    ToFlags, ToText, Raw, Tree, Wood
  );
  report();
}

/// `puns::validate_in` against the oracle, on every type of the model, every single-byte change
/// by every mask of one bit and of all bits.
#[test]
fn validate_in_is_the_oracle() {
  use puns_model::pointers::check_validate;
  let masks = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0xff];
  let compared = check_validate::<Flags>(&masks)
    + check_validate::<Bytes>(&masks)
    + check_validate::<FlagList>(&masks)
    + check_validate::<ByteList>(&masks)
    + check_validate::<ToFlag>(&masks)
    + check_validate::<ToByte>(&masks)
    + check_validate::<MaybeByte>(&masks)
    + check_validate::<ToByteBe>(&masks)
    + check_validate::<ToWord>(&masks)
    + check_validate::<ToPair>(&masks)
    + check_validate::<ToBytes>(&masks)
    + check_validate::<ToFlags>(&masks)
    + check_validate::<ToText>(&masks)
    + check_validate::<Raw>(&masks)
    + check_validate::<Tree>(&masks)
    + check_validate::<Wood>(&masks);
  println!("{compared} images compared");
}
