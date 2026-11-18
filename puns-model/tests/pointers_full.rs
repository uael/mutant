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
