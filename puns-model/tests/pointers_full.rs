//! The exhaustive model check of self-relative pointers: every pair of every type of the model,
//! and every value of every byte of their images given to the loaders. Behind the features
//! `nearest` and `full`: `cargo test --profile model -p puns-model --features nearest,full`.

#![cfg(all(feature = "nearest", feature = "full"))]
#![cfg(not(miri))]

use puns_model::pointers::{check_load, report, types::*};

#[test]
fn every_pair() {
  puns_model::pointer_pairs!(check_pair;
    Flags, Bytes, FlagList, ByteList, ToFlag, ToByte, MaybeByte, ToByteBe, ToWord, ToPair, ToBytes,
    ToFlags, ToText, Raw, Tree, Wood
  );
  report();
}

#[test]
fn every_byte() {
  let masks: Vec<u8> = (1..=255).collect();
  check_load::<Flags>(&masks);
  check_load::<Bytes>(&masks);
  check_load::<FlagList>(&masks);
  check_load::<ByteList>(&masks);
  check_load::<ToFlag>(&masks);
  check_load::<ToByte>(&masks);
  check_load::<MaybeByte>(&masks);
  check_load::<ToByteBe>(&masks);
  check_load::<ToWord>(&masks);
  check_load::<ToPair>(&masks);
  check_load::<ToBytes>(&masks);
  check_load::<ToFlags>(&masks);
  check_load::<ToText>(&masks);
  check_load::<Raw>(&masks);
  check_load::<Tree>(&masks);
  check_load::<Wood>(&masks);
}
