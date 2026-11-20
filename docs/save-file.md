# A save file in two versions

This walkthrough reads the save file of a dungeon game with `puns`. Version 1 of the game saves
the hero's level, hit points and bag of items. Version 2 adds keys to the items, and must still
load the saves of version 1. The whole program is below, after the steps. It runs as a test of
the crate.

## The items

An item is a `repr(u8)` enum with fields: a tag, then the fields of its variant. `OldItem` is an
item of version 1, and `Item` adds a third variant, `Key`. A key's floor is a `NonZero<u8>`, and
its door is an `Option<NonZero<u8>>`, where 0 means `None`.

`as_bytes` views an item as its bytes and checks nothing, since every byte of an item is
initialized. `from_bytes` reads bytes as an item, and checks the tag, then the fields of its
variant. A key of floor 0 is rejected. A `u8c<1>` always holds 1, so `pun` turns it into a
`NonZero<u8>` without a check.

## The save

`Save` is generic over the type of its version and of its bag, so that one declaration describes
both versions. Its magic number is a `u32bec`, a big-endian `u32` that holds one value only. Its
level must be between 1 and 99 (`#[pun(valid(..))]`), and its hit points at most ten per level
(`#[pun(assert(..))]`). Its bag runs to the end of the file.

Version 1 writes its save by copying the bytes of a `Save` into a `Vec`. Then it raises the level
in the file, through a `&mut Save` that `from_bytes_mut` returns.

## Two versions

`SaveFile` is an enum whose variants hold references. The derive gives it a `from_bytes` that
returns the first variant that the bytes match. `load` reads either version, then converts it to
a `Save` with `pun_ref`. That conversion checks nothing, because every old item is an item and
every version is a `u8`.

Once the hero takes a key, the file is no longer a valid save of version 1. Writing 2 in its
version byte upgrades it. Reading the bag back as old items is checked, and fails because of the
key.

## Requirements are checked when bytes are read

`bool`, `NonZero`, enums and the types of `puns::num` guarantee the values that they hold. The
`valid` and `assert` attributes are requirements instead. Safe code can break them by assigning
a field, as the last step does with `hp`, and `puns` checks them again only when it reads bytes
as a `Save`.

## The program

```rust
use core::num::NonZero;

use puns::{
  Pun,
  num::{u8c, u16le, u32bec},
};

/// An item of version 1 of the game, saved as a tag and two bytes.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0, // little-endian, at any address
  Potion { heal: u8, cursed: bool } = 1,
}

/// An item of version 2, which adds keys. A key without a door opens every door of its floor.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: NonZero<u8>, door: Option<NonZero<u8>> } = 2,
}

// View an item as its bytes. Every item is three initialized bytes, so nothing is checked.
assert_eq!(puns::as_bytes(&OldItem::Sword { damage: 300.into() }), [0, 0x2c, 0x01]);
// Read three bytes as an item. The tag is checked, then the fields of its variant.
let key: &Item = puns::from_bytes(&[2, 1, 0]).unwrap();
// A `u8c<1>` is always 1, so it converts to a `NonZero<u8>` without a check. A door of 0 is `None`.
assert_eq!(key, &Item::Key { floor: puns::pun(u8c::<1>::new()), door: None });
// Floor 0 is not a `NonZero<u8>`, so these bytes are not an item.
assert_eq!(puns::from_bytes::<Item>(&[2, 0, 0]), Err(puns::Error::Validity));

/// A save file of version `V`, with a bag of type `B`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))] // at most 10 hp per level
struct Save<V = u8, B: ?Sized = [Item]> {
  magic: u32bec<0x5341_5645>, // always "SAVE"
  version: V,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: B, // to the end of the file
}

// Write a save of version 1. Every byte of a save is initialized, so it can be copied as bytes.
let bag = [OldItem::Sword { damage: 9.into() }, OldItem::Potion { heal: 25, cursed: false }];
let old = Save { magic: u32bec::new(), version: u8c::<1>::new(), level: 7, hp: 42.into(), bag };
let mut file = puns::as_bytes(&old).to_vec();
// The first four bytes of a save are its magic number.
assert_eq!(puns::head_ref::<_, [u8; 4]>(&old), b"SAVE");
// Raise the level in the file, through a `&mut Save` that points into its bytes.
puns::from_bytes_mut::<Save<u8c<1>, [OldItem]>>(&mut file).unwrap().level += 1;

/// A save file of either version.
#[derive(Pun)]
enum SaveFile<'a> {
  V1(&'a Save<u8c<1>, [OldItem]>),
  V2(&'a Save<u8c<2>>),
}

/// Loads a save of either version as a `Save`, whose version is a `u8`.
fn load(file: &[u8]) -> Result<&Save, puns::Error> {
  // Check the bytes once: the magic number, the version, the level, the rule and each item.
  Ok(match SaveFile::from_bytes(file)? {
    // Every old item is an item, and every version is a `u8`, so neither arm checks anything.
    SaveFile::V1(old) => puns::pun_ref(old),
    SaveFile::V2(new) => puns::pun_ref(new),
  })
}

let loaded = load(&file).unwrap();
assert_eq!((loaded.version, loaded.level), (1, 8));
// Add the key to the bag. Version 1 has no keys, so the file is no longer a valid save.
file.extend_from_slice(puns::as_bytes(key));
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
// Upgrade the file to version 2 by writing its version byte.
puns::from_bytes_mut::<Save>(&mut file).unwrap().version = 2;
let save = load(&file).unwrap();
assert_eq!(save.bag[0], Item::Sword { damage: 9.into() });
assert_eq!(&save.bag[2], key);
// Reading the bag as old items is checked. It fails here, because of the key.
assert_eq!(puns::try_pun_ref::<_, [OldItem]>(&save.bag).err(), Some(puns::Error::Validity));
// Give the hero 999 hp. The rule is checked when bytes are read as a `Save`, so the file no
// longer loads.
puns::from_bytes_mut::<Save>(&mut file).unwrap().hp = 999.into();
assert_eq!(load(&file).err(), Some(puns::Error::Validity));
```
