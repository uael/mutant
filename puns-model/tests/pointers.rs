//! The sample of the model check of self-relative pointers that runs on every push (with the
//! feature `nearest`): see `puns_model::pointers` for what is checked, and `pointers_full.rs` for
//! every pair of every type.

#![cfg(feature = "nearest")]

use puns_model::pointers::{check_load, report, sound, types::*};

/// What the oracle alone says of pairs whose answer is known: it finds the values that are not
/// a target, in corpora that have them.
#[test]
fn the_oracle_tells() {
  // More values, a pointer that may be null, more variants.
  assert!(sound::<Flags, Bytes>() && sound::<ToFlag, ToByte>() && sound::<ToByte, MaybeByte>());
  assert!(sound::<Tree, Wood>() && sound::<FlagList, ByteList>() && sound::<ToFlags, ToBytes>());
  // The first bytes of what a pointer aims at, and a pointer as its bytes.
  assert!(sound::<ToWord, ToByte>() && sound::<ToWord, ToPair>() && sound::<ToByte, Raw>());
  // Fewer values, a null pointer, another byte order, a larger target, another kind of target,
  // bytes as a pointer, fewer variants, text that is not.
  assert!(!sound::<Bytes, Flags>() && !sound::<ToByte, ToFlag>() && !sound::<MaybeByte, ToByte>());
  assert!(!sound::<ToByte, ToByteBe>() && !sound::<ToByte, ToWord>() && !sound::<Raw, ToByte>());
  assert!(!sound::<ToByte, ToBytes>() && !sound::<Wood, Tree>() && !sound::<ByteList, FlagList>());
  // What the library refuses and the oracle does not: the length of a list is initialized
  // bytes, so its first byte can be read. Refused all the same: a list is read as a list.
  assert!(sound::<ToBytes, ToByte>());
  assert!(
    !sound::<ToBytes, ToText>() && !sound::<Flags, FlagList>() && !sound::<ToBytes, ToFlags>()
  );
}

/// What the library accepts is what the functions themselves compile: `hook::views` asks the
/// code that decides it, here are the functions, for pairs that it accepts.
#[test]
fn the_functions_themselves() {
  fn view<S: Model + 'static, D: Model>() {
    assert!(const { puns::hook::views::<S, D>(FUEL) });
    for region in S::corpus() {
      let view: &D = puns::view_ref(&*region);
      assert_eq!(
        format!("{view:?}"),
        format!("{:?}", puns::try_view_ref::<S, D>(&region).unwrap())
      );
    }
  }
  use puns_model::pointers::{FUEL, Model};
  view::<Flags, Bytes>();
  view::<Tree, Wood>();
  view::<FlagList, ByteList>();
  // Under Miri, a recursive struct, an enum and lists are what there is to see.
  if !cfg!(miri) {
    view::<ToFlags, ToBytes>();
    view::<ToWord, ToByte>();
    view::<ToByte, MaybeByte>();
    view::<ToText, ToText>();
    view::<ToByteBe, Raw>();
  }
}

/// Every pair of a selection of the types: a recursive struct, an enum, a pointer that may be
/// null, bytes. Under Miri, a few of them, for the views that are read.
#[test]
fn pairs() {
  #[cfg(not(miri))]
  puns_model::pointer_pairs!(check_pair; Flags, Bytes, ToFlag, ToByte, MaybeByte, Raw, Tree, Wood);
  #[cfg(miri)]
  puns_model::pointer_pairs!(check_pair; MaybeByte, Tree, Wood);
  report();
}

/// The loaders, on the images of the same types and on their corruptions.
#[test]
fn loaders() {
  let masks: &[u8] = if cfg!(miri) { &[0xff] } else { &[0x01, 0x04, 0x80, 0xff] };
  check_load::<MaybeByte>(masks);
  check_load::<Tree>(masks);
  if !cfg!(miri) {
    check_load::<Flags>(masks);
    check_load::<Bytes>(masks);
    check_load::<ToFlag>(masks);
    check_load::<Wood>(masks);
    check_load::<ByteList>(masks);
    check_load::<ToText>(masks);
    check_load::<ToByteBe>(masks);
  }
}
