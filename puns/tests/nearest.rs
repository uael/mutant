//! The feature `nearest`: offsets of a given byte order, regions checked from the layouts (and
//! compared with the check of `nearest` on valid images and on corruptions of them), values of a
//! region read as other types, rules checked when a region is loaded.

use nearest::{Edit, Error, Flat, Near, Offset, Pending, Region, flat};
use puns::{
  Pun,
  layout::Endian,
  nearest::Order,
  num::{i16be, i16le, i32be, i32le, i64be, i64le, u8c, u8in, u16bec, u16le, u32le, u64lec},
};

// -------------------------------------------------------------------------------------------------
// A schema, generic over the offset

/// An enum without fields: a scalar.
#[flat]
#[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
  File,
  Dir,
  Link,
}

#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
pub struct Item<O: Order> {
  pub id: u32,
  pub hidden: bool,
  pub kind: Kind,
  pub name: Near<str, O>,
  pub tags: Near<[u16], O>,
  pub next: Option<Near<Item<O>, O>>,
}

#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
pub struct Dir<O: Order> {
  pub size: u64,
  pub flags: [bool; 3],
  pub entries: Near<[Item<O>], O>,
  pub parent: Option<Near<Dir<O>, O>>,
}

#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
pub enum Expr<O: Order> {
  Lit(i64),
  Neg(Near<Expr<O>, O>),
  Add(Near<Expr<O>, O>, Near<Expr<O>, O>),
  Var(Near<str, O>),
  Call(Near<str, O>, Near<[Expr<O>], O>),
  Cond(Option<Near<Expr<O>, O>>, char),
}

/// As the `Tree` of the tests of `nearest` (`tests/width.rs`).
#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
struct Tree<O: Order> {
  /// 8-aligned, so that lists of trees are 8-aligned behind 2-byte headers too.
  id: u64,
  name: Near<str, O>,
  kids: Near<[Tree<O>], O>,
  next: Option<Near<Tree<O>, O>>,
}

fn leaf<O: Order>(id: u64, name: &str) -> impl nearest::Emit<Tree<O>> {
  Tree::new(id, name, (), None)
}

fn tree<O: Order>() -> Region<Tree<O>> {
  Region::new(Tree::new(
    1,
    "root",
    (leaf(2, "a"), Tree::new(3, "b", [leaf(4, "c")], None)),
    Near::new(leaf(5, "next")),
  ))
}

fn ids<O: Order>(t: &Tree<O>) -> Vec<u64> {
  let mut out = vec![t.id];
  for k in &t.kids {
    out.extend(ids(k));
  }
  out.extend(t.next.as_deref().map_or_default(ids));
  out
}

// -------------------------------------------------------------------------------------------------
// Offsets: what `nearest` requires of an `Offset`, and what `Order` says of its bytes

/// The integer that `bytes` are in two's complement, in the byte order `endian`.
fn decode(bytes: &[u8], endian: Endian) -> i64 {
  let mut wide = [if bytes[if endian == Endian::Little { bytes.len() - 1 } else { 0 }] & 0x80 == 0 {
    0
  } else {
    0xff
  }; 8];
  for (i, byte) in bytes.iter().enumerate() {
    let digit = if endian == Endian::Little { i } else { bytes.len() - 1 - i };
    wide[digit] = *byte;
  }
  i64::from_le_bytes(wide)
}

/// The `O` at `at`: read as the layout of the pointers says, not with `to_isize`.
fn int<O: Order>(bytes: &[u8], at: usize) -> i64 {
  decode(&bytes[at..at + size_of::<O>()], O::ENDIAN)
}

/// Writes `v` as the `O` at `at`.
fn put<O: Order + Pun>(bytes: &mut [u8], at: usize, v: i64) {
  let o = O::from_isize(isize::try_from(v).unwrap());
  bytes[at..at + size_of::<O>()].copy_from_slice(puns::as_bytes(&o));
}

/// A pointer, an optional pointer, a list and a string are each one `O`, aligned as it is.
fn sizes<O: Order>() {
  let w = size_of::<O>();
  assert!(matches!(w, 2 | 4 | 8));
  assert_eq!((size_of::<Near<u8, O>>(), align_of::<Near<u8, O>>()), (w, align_of::<O>()));
  assert_eq!(size_of::<Option<Near<u8, O>>>(), w);
  assert_eq!(size_of::<Near<[u8], O>>(), w);
  assert_eq!(size_of::<Near<str, O>>(), w);
  assert_eq!((size_of::<O::NonZero>(), align_of::<O::NonZero>()), (w, align_of::<O>()));
  assert_eq!(size_of::<Option<O::NonZero>>(), w);
  assert!(O::LIMIT.is_multiple_of(8) && O::LIMIT == (1 << (8 * w - 1)) - 8);
}

/// The contract of `Offset`, and of `Order`: a round trip, zero is the zero bytes and nothing
/// else is, and the bytes are the integer in the byte order that the type says.
fn contract<O: Order + Pun>() {
  let w = size_of::<O>();
  let limit = isize::try_from(O::LIMIT).unwrap_or(isize::MAX);
  // Every bit pattern is an integer of the size of `O`: also the ones a region never holds.
  let (least, most) = (i64::MIN >> (64 - 8 * w), i64::MAX >> (64 - 8 * w));
  let mut samples = vec![0, 1, -1, 2, 127, 128, 255, 256, -255, -256, 0x1234, -0x1234, least, most];
  samples.extend([limit as i64, -(limit as i64), limit as i64 - 1, most - 1, least + 1]);
  samples.extend((0..63).map(|bit| (1i64 << bit) & most));
  for n in samples {
    let Ok(wanted) = isize::try_from(n) else { continue };
    let o = O::from_isize(wanted);
    assert_eq!(o.to_isize(), wanted, "{n} does not round trip");
    let bytes = puns::as_bytes(&o);
    assert_eq!(decode(bytes, O::ENDIAN), n, "{n} is {bytes:?}");
    assert_eq!(bytes.iter().all(|byte| *byte == 0), n == 0);
    // The `NonZero` is the same bytes, and `None` of it is zero.
    let stored: Option<O::NonZero> = puns_try_non_zero::<O>(bytes);
    assert_eq!(stored.is_some(), n != 0);
    if let Some(stored) = stored {
      let back: O = stored.into();
      assert_eq!(puns::as_bytes(&back), bytes);
    }
  }
}

/// The `Option<O::NonZero>` whose bytes are `bytes`: any `size_of::<O>()` bytes are one.
fn puns_try_non_zero<O: Offset>(bytes: &[u8]) -> Option<O::NonZero> {
  assert_eq!((bytes.len(), size_of::<Option<O::NonZero>>()), (size_of::<O>(), size_of::<O>()));
  // SAFETY: an `Option` of the `NonZero` of an offset has the size of the offset, and every
  // bit pattern of that size is one (`None` is zero): the contract of `Offset`.
  unsafe { bytes.as_ptr().cast::<Option<O::NonZero>>().read_unaligned() }
}

fn build_and_read<O: Order>() {
  let r = tree::<O>();
  assert_eq!(ids(&r), [1, 2, 3, 4, 5]);
  assert_eq!((&*r.name, &*r.kids[1].kids[0].name), ("root", "c"));
  assert_eq!(r.next.as_ref().map(|n| &*n.name), Some("next"));
  assert!(r.kids.iter().all(|k| (&raw const *k).addr().is_multiple_of(8)));
  assert_eq!(r.clone(), r);
  assert_eq!(Region::new(&*r), r);
  let e = Region::new(Expr::<O>::add(Expr::neg(Expr::Lit(1)), Expr::Lit(2)));
  assert!(matches!(&*e, Expr::Add(a, b) if matches!(&**a, Expr::Neg(_)) && **b == Expr::Lit(2)));
}

/// Every handle, and moves inside the region and copies out of it.
fn edit<O: Order>() {
  let mut r = tree::<O>();
  let t = r.edit();
  *t.id() = 10;
  t.name().set("renamed");
  let old = t.kids().at(1).name().replace("b2");
  t.kids().at(0).name().set(old); // a move
  t.kids().push(leaf(6, "d"));
  for i in 0..20 {
    t.kids().at(1).kids().push(leaf(100 + i, "x")); // grows, moves the block
  }
  t.kids().at(1).kids().retain(|k| k.id % 2 == 0);
  t.kids().at(1).kids().truncate(5);
  let c = t.kids().at(1).kids().remove(0);
  t.kids().insert(0, c);
  t.kids().swap(0, 3);
  let next = t.next().take().unwrap();
  t.kids().at(0).next().set(Near::new(next));
  t.kids().at(0).update(|old| Tree::new(7, "wrapped", [old], None));
  t.next().insert(leaf(8, "new")).name().update(|s| s);
  assert_eq!(ids(&r), [10, 7, 6, 5, 2, 3, 100, 102, 104, 106, 4, 8]);
  let names = r.kids.iter().map(|k| &*k.name).collect::<Vec<_>>();
  assert_eq!((&*r.name, names), ("renamed", vec!["wrapped", "b", "b2", "c"]));
  // The edited region is not compact; its copy is, and is the same value for both checks.
  let compact = Region::new(&*r);
  assert!(compact.byte_len() < r.byte_len() && r.as_bytes().is_none());
  let image = compact.as_bytes().unwrap();
  assert_eq!(puns::nearest::view::<Tree<O>>(image), Ok(&*r));
  assert_eq!(nearest::view::<Tree<O>>(image), Ok(&*r));
}

/// A helper that takes the handle of a generic type, and returns one.
fn first<'a, 'e, O: Order>(t: &'a mut Edit<'e, Tree<O>>) -> &'a mut Edit<'e, Tree<O>> {
  t.kids().at(0)
}

/// The layout: an `O` pointer, list headers of two `O`s before elements aligned for their type,
/// string lengths of one `O`, all in the byte order of `O`.
fn layout<O: Order>() {
  let w = size_of::<O>();
  let list = Region::<Near<[u64], O>>::new(Near::from_iter([7u64]));
  let bytes = list.as_bytes().unwrap();
  let elems = (3 * w).next_multiple_of(8);
  assert_eq!(bytes.len(), elems + 8);
  assert_eq!(int::<O>(bytes, 0), i64::try_from(elems - w).unwrap()); // to the length
  assert_eq!((int::<O>(bytes, elems - 2 * w), int::<O>(bytes, elems - w)), (1, 1));
  assert_eq!(bytes[elems..], 7u64.to_ne_bytes());
  let text = Region::<Near<str, O>>::new("hi");
  let bytes = text.as_bytes().unwrap();
  assert_eq!(bytes.len(), 2 * w + 2);
  assert_eq!((int::<O>(bytes, 0), int::<O>(bytes, w)), (i64::try_from(w).unwrap(), 2));
  assert_eq!(&bytes[2 * w..], b"hi");
}

/// Bytes: saved, checked, loaded and viewed, by this crate as by `nearest`; a bad image is the
/// same error for both.
fn bytes<O: Order + Pun>() {
  let mut r = tree::<O>();
  *first(r.edit()).id() = 20;
  r.edit().kids().at(0).kids().push(leaf(9, "late"));
  let compact = Region::new(&*r);
  let image = compact.as_bytes().unwrap();
  assert_eq!(puns::nearest::from_bytes::<Tree<O>>(image), Ok(Region::new(&*r)));
  assert_eq!(puns::nearest::view::<Tree<O>>(image), Ok(&*r));
  let mut scratch = vec![puns::nearest::Pending::default(); image.len() / size_of::<O>().min(4)];
  assert_eq!(puns::nearest::view_with::<Tree<O>>(image, &mut scratch), Ok(&*r));
  assert_eq!(
    puns::nearest::from_bytes_with::<Tree<O>>(image, &mut []).err(),
    Some(Error::ScratchFull)
  );
  // Cut, padded, or a pointer one byte off.
  let same = |bytes: &[u8]| {
    let (ours, theirs) =
      (puns::nearest::from_bytes::<Tree<O>>(bytes), Region::<Tree<O>>::from_bytes(bytes));
    assert_eq!(ours, theirs);
    ours.err()
  };
  assert!(matches!(same(&image[..image.len() - 1]), Some(Error::Invalid(_))));
  let mut long = image.to_vec();
  long.push(0);
  assert_eq!(same(&long), Some(Error::Invalid(image.len())));
  let name = std::mem::offset_of!(Tree<O>, name);
  for n in [1, -1] {
    let mut bad = image.to_vec();
    put::<O>(&mut bad, name, int::<O>(image, name) + n);
    assert_eq!(same(&bad), Some(Error::Invalid(name)));
  }
  // A negative length is as bad as one too long.
  let text = Region::<Near<str, O>>::new("hi");
  let mut bad = text.as_bytes().unwrap().to_vec();
  let w = size_of::<O>();
  put::<O>(&mut bad, w, -1);
  assert_eq!(puns::nearest::from_bytes::<Near<str, O>>(&bad).err(), Some(Error::Invalid(w)));
  assert_eq!(Region::<Near<str, O>>::from_bytes(&bad).err(), Some(Error::Invalid(w)));
}

macro_rules! offsets {
  ($($test:ident),*) => {
    mod w16 { $(#[test] fn $test() { super::$test::<i16>() })* }
    mod w32 { $(#[test] fn $test() { super::$test::<i32>() })* }
    mod w64 { $(#[test] fn $test() { super::$test::<i64>() })* }
    mod le16 { $(#[test] fn $test() { super::$test::<super::i16le>() })* }
    mod le32 { $(#[test] fn $test() { super::$test::<super::i32le>() })* }
    mod le64 { $(#[test] fn $test() { super::$test::<super::i64le>() })* }
    mod be16 { $(#[test] fn $test() { super::$test::<super::i16be>() })* }
    mod be32 { $(#[test] fn $test() { super::$test::<super::i32be>() })* }
    mod be64 { $(#[test] fn $test() { super::$test::<super::i64be>() })* }
  };
}
offsets!(sizes, contract, build_and_read, edit, layout, bytes);

/// The bytes of a region are the same on every target with offsets of a given byte order, and
/// such offsets have no alignment requirement.
#[test]
fn byte_orders() {
  let le = Region::<Near<str, i32le>>::new("hi");
  assert_eq!(le.as_bytes().unwrap(), [4, 0, 0, 0, 2, 0, 0, 0, b'h', b'i']);
  let be = Region::<Near<str, i32be>>::new("hi");
  assert_eq!(be.as_bytes().unwrap(), [0, 0, 0, 4, 0, 0, 0, 2, b'h', b'i']);
  let be = Region::<Near<[u8], i16be>>::new(Near::from_iter([7u8]));
  // A pointer to the length, the capacity and the length, the element.
  assert_eq!(be.as_bytes().unwrap(), [0, 4, 0, 1, 0, 1, 7]);
  let le = Region::<Near<u8, i64le>>::new(Near::new(7));
  assert_eq!(le.as_bytes().unwrap(), [8, 0, 0, 0, 0, 0, 0, 0, 7]);
  // Loaded with the byte order it was written with, and only that one.
  let image = Region::<Near<str, i32be>>::new("hello").as_bytes().unwrap().to_vec();
  assert_eq!(&**puns::nearest::from_bytes::<Near<str, i32be>>(&image).unwrap(), "hello");
  assert!(puns::nearest::from_bytes::<Near<str, i32le>>(&image).is_err());
  // A byte array: a pointer may be anywhere.
  assert_eq!((align_of::<i32le>(), align_of::<Near<u64, i64be>>()), (1, 1));
  assert_eq!((size_of::<Table>(), align_of::<Table>()), (9, 1));
}

/// Numbers of a given byte order are stored as their bytes are, and edited in place.
#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
struct Table {
  version: u8,
  count: u32le,
  rows: Near<[u16le], i32le>,
}

#[test]
fn numbers_of_a_byte_order_in_a_region() {
  let mut r = Region::new(Table::new(1, u32le::new(2), [u16le::new(0x0102), u16le::new(3)]));
  // The version, the count, the distance to the length of the rows; the capacity and the length,
  // so placed that the rows are aligned to the width of the offset; the rows.
  let image = [1, 2, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 1, 3, 0];
  assert_eq!(r.as_bytes().unwrap(), image);
  assert_eq!(puns::nearest::view::<Table>(r.as_bytes().unwrap()), Ok(&*r));
  // Plain data: a handle derefs to the number itself.
  let t = r.edit();
  *t.count() = u32le::new(3);
  t.rows().push(u16le::new(0xffff));
  let sum = r.rows.iter().map(|row| u32::from(row.get())).sum::<u32>();
  assert_eq!((r.count.get(), sum), (3, 0x0102 + 3 + 0xffff));
  const { assert!(<u32le as Flat>::PLAIN && <u32le as Flat>::ANY && <i64be as Flat>::ANY) };
}

/// A 2-byte offset: a region of at most 32 KiB, as with `i16`.
#[test]
fn the_limit_of_a_narrow_offset() {
  type Small = Near<[u32], i16le>;
  let limit = i16::MAX as usize & !7;
  assert_eq!(<i16le as Offset>::LIMIT, limit as u64);
  let r = Region::<Small>::new(Near::from_iter([1u32, 2]));
  let mut long = r.as_bytes().unwrap().to_vec();
  assert_eq!(puns::nearest::from_bytes::<Small>(&long).map(|r| r.to_vec()), Ok(vec![1, 2]));
  long.resize(limit + 8, 0);
  assert_eq!(puns::nearest::from_bytes::<Small>(&long).err(), Some(Error::Invalid(0)));
  assert_eq!(Region::<Small>::from_bytes(&long).err(), Some(Error::Invalid(0)));
  let full = std::panic::catch_unwind(|| Region::<Small>::new(Near::from_iter(0..10_000u32)));
  assert!(full.is_err());
}

// -------------------------------------------------------------------------------------------------
// The check of this crate against the one of `nearest`

/// How much of the corpus runs: Miri is slow.
const SCALE: usize = if cfg!(miri) { 1 } else { 6 };

#[derive(Default, Debug)]
struct Stats {
  images: usize,
  compared: usize,
  accepted: usize,
  rejected: usize,
}

/// `bytes` in words: an 8-aligned copy.
fn aligned(bytes: &[u8]) -> Vec<u64> {
  let mut words = vec![0u64; bytes.len().div_ceil(8) + 1];
  puns::pun_mut::<[u64], [u8]>(&mut words)[..bytes.len()].copy_from_slice(bytes);
  words
}

/// Both checks on the same bytes: the same result, and an accepted image is the same value.
fn same<T: Flat + Pun + PartialEq + core::fmt::Debug>(bytes: &[u8], stats: &mut Stats) {
  let (theirs, ours) = (nearest::view::<T>(bytes), puns::nearest::view::<T>(bytes));
  match (theirs, ours) {
    (Ok(a), Ok(b)) => {
      assert!(core::ptr::eq(a, b));
      stats.accepted += 1;
    }
    (Err(a), Err(b)) => {
      assert_eq!(a, b, "different errors on {bytes:?}");
      stats.rejected += 1;
    }
    (a, b) => panic!("nearest: {a:?}, puns: {b:?}, on {bytes:?}"),
  }
  stats.compared += 1;
}

/// The same through `from_bytes`, at an odd address.
fn same_loaded<T: Flat + Pun + PartialEq + core::fmt::Debug>(bytes: &[u8]) {
  let mut odd = vec![0u8; bytes.len() + 1];
  odd[1..].copy_from_slice(bytes);
  let (theirs, ours) = (Region::<T>::from_bytes(&odd[1..]), puns::nearest::from_bytes(&odd[1..]));
  assert_eq!(theirs, ours);
}

/// The same with every size of scratch up to one that suffices: both run out at the same sizes.
fn same_scratch<T: Flat + Pun + PartialEq + core::fmt::Debug>(bytes: &[u8]) {
  for size in 0.. {
    let mut theirs = vec![Pending::default(); size];
    let mut ours = vec![puns::nearest::Pending::default(); size];
    let (theirs, ours) = (
      nearest::view_with::<T>(bytes, &mut theirs),
      puns::nearest::view_with::<T>(bytes, &mut ours),
    );
    assert_eq!(theirs, ours, "with {size} entries");
    if theirs != Err(Error::ScratchFull) {
      break;
    }
  }
}

/// The image, then every byte changed in a few ways, every truncation and some extensions.
fn corrupt<T: Flat + Pun + PartialEq + core::fmt::Debug>(image: &[u8], stats: &mut Stats) {
  stats.images += 1;
  let (mut words, len) = (aligned(image), image.len());
  let ok = puns::nearest::view::<T>(&puns::as_bytes(&words[..])[..len]);
  assert!(ok.is_ok(), "a valid image is rejected: {ok:?}");
  same::<T>(&puns::as_bytes(&words[..])[..len], stats);
  same_loaded::<T>(image);
  same_scratch::<T>(&puns::as_bytes(&words[..])[..len]);
  let masks: &[u8] = if cfg!(miri) { &[0xff] } else { &[0x01, 0x02, 0x04, 0x10, 0x80, 0xff] };
  let step = if cfg!(miri) { 3 } else { 1 };
  for at in (0..len).step_by(step) {
    for &mask in masks {
      puns::pun_mut::<[u64], [u8]>(&mut words)[at] ^= mask;
      same::<T>(&puns::as_bytes(&words[..])[..len], stats);
      if !cfg!(miri) {
        same_loaded::<T>(&puns::as_bytes(&words[..])[..len]);
      }
      puns::pun_mut::<[u64], [u8]>(&mut words)[at] ^= mask;
    }
  }
  for cut in (0..len).step_by(step) {
    same::<T>(&puns::as_bytes(&words[..])[..cut], stats);
  }
  for extra in 1..=7 {
    let words = aligned(&[image, &vec![0; extra][..]].concat());
    same::<T>(&puns::as_bytes(&words[..])[..len + extra], stats);
  }
  // Not 8-aligned.
  let words = aligned(&[&[0u8][..], image].concat());
  same::<T>(&puns::as_bytes(&words[..])[1..=len], stats);
}

/// A small deterministic generator, for the corpus.
struct Rng(u64);

impl Rng {
  const fn next(&mut self) -> u64 {
    // xorshift64*
    self.0 ^= self.0 >> 12;
    self.0 ^= self.0 << 25;
    self.0 ^= self.0 >> 27;
    self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
  }

  const fn below(&mut self, n: u64) -> usize {
    (self.next() % n) as usize
  }
}

const fn name(rng: &mut Rng) -> &'static str {
  ["a", "bé", "ç∂", "dir", "", "😀x", "long-name-of-a-file"][rng.below(7)]
}

fn item_chain<O: Order>(rng: &mut Rng, len: usize) -> Region<Item<O>> {
  let kinds = [Kind::File, Kind::Dir, Kind::Link];
  let tags: Vec<u16> = (0..rng.below(4)).map(|_| rng.next() as u16).collect();
  let mut r =
    Region::new(Item::new(rng.next() as u32, false, Kind::Dir, name(rng), &tags[..], None));
  let mut cur = r.edit();
  for _ in 0..len {
    let tags: Vec<u16> = (0..rng.below(3)).map(|_| rng.next() as u16).collect();
    let (id, hidden, kind) = (rng.next() as u32, rng.below(2) == 1, kinds[rng.below(3)]);
    cur = cur.next().insert(Item::new(id, hidden, kind, name(rng), &tags[..], None));
  }
  Region::new(&*r)
}

fn expr<O: Order>(rng: &mut Rng, depth: usize) -> Region<Expr<O>> {
  if depth == 0 {
    return match rng.below(3) {
      0 => Region::new(Expr::Lit(rng.next().cast_signed())),
      1 => Region::new(Expr::var(name(rng))),
      _ => Region::new(Expr::cond(None, char::from_u32(rng.next() as u32 >> 12).unwrap_or('?'))),
    };
  }
  match rng.below(5) {
    0 => Region::new(Expr::neg(&*expr(rng, depth - 1))),
    1 => {
      let (a, b) = (expr(rng, depth - 1), expr(rng, depth - 1));
      Region::new(Expr::add(&*a, &*b))
    }
    2 => {
      let args: Vec<_> = (0..rng.below(4)).map(|_| expr(rng, depth - 1)).collect();
      Region::new(Expr::call(name(rng), Near::from_iter(args.iter().map(|a| &**a))))
    }
    3 => Region::new(Expr::cond(Near::new(&*expr(rng, depth - 1)), 'x')),
    _ => expr(rng, 0),
  }
}

fn dir<O: Order>(rng: &mut Rng, depth: usize) -> Region<Dir<O>> {
  let entries: Vec<_> = (0..rng.below(3))
    .map(|_| {
      let len = rng.below(2);
      item_chain::<O>(rng, len)
    })
    .collect();
  let parent = (depth > 0).then(|| dir::<O>(rng, depth - 1));
  let (size, flags) = (rng.next(), [rng.below(2) == 1, true, false]);
  let entries = || Near::from_iter(entries.iter().map(|e| &**e));
  parent.as_ref().map_or_else(
    || Region::new(Dir::new(size, flags, entries(), None)),
    |parent| Region::new(Dir::new(size, flags, entries(), Near::new(&**parent))),
  )
}

/// Valid images of every type of the schema, and their corruptions.
fn corpus<O: Order>(scale: usize) -> Stats {
  let mut stats = Stats::default();
  let mut rng = Rng(0x5eed_1234_abcd_0001);
  for i in 0..scale {
    let r = item_chain::<O>(&mut rng, i % 4);
    corrupt::<Item<O>>(r.as_bytes().unwrap(), &mut stats);
    let r = expr::<O>(&mut rng, 1 + i % 3);
    corrupt::<Expr<O>>(r.as_bytes().unwrap(), &mut stats);
    let r = dir::<O>(&mut rng, i % 2);
    corrupt::<Dir<O>>(r.as_bytes().unwrap(), &mut stats);
  }
  assert!(stats.accepted > 0 && stats.rejected > stats.accepted, "{stats:?}");
  stats
}

/// The same accepts, the same rejects and the same errors as `nearest`, with offsets of the
/// target and of each byte order, of each width.
#[test]
fn the_check_is_the_one_of_nearest() {
  let mut total = corpus::<i32le>(SCALE);
  if !cfg!(miri) {
    for stats in [corpus::<i32>(SCALE), corpus::<i16be>(SCALE), corpus::<i64le>(SCALE)] {
      total.images += stats.images;
      total.compared += stats.compared;
      total.accepted += stats.accepted;
      total.rejected += stats.rejected;
    }
    let mut stats = Stats::default();
    // Images that are only plain data, and an enum without fields.
    corrupt::<Kind>(Region::new(Kind::Link).as_bytes().unwrap(), &mut stats);
    corrupt::<[bool; 3]>(
      Region::<[bool; 3]>::new([true, false, true]).as_bytes().unwrap(),
      &mut stats,
    );
    corrupt::<Table>(
      Region::new(Table::new(1, u32le::new(2), [u16le::new(3)])).as_bytes().unwrap(),
      &mut stats,
    );
    // Numbers that are one value, or some: `nearest` checks them as this crate does.
    let constants = Region::new(Constants::new(
      u16bec::new(),
      u8c::new(),
      u8in::new(3).unwrap(),
      [u64lec::new(), u64lec::new()],
    ));
    corrupt::<Constants>(constants.as_bytes().unwrap(), &mut stats);
    // And both refuse any other value: here the last byte, of the second mark.
    let mut bad = constants.as_bytes().unwrap().to_vec();
    *bad.last_mut().unwrap() = 1;
    let at = bad.len() - 8;
    assert_eq!(Region::<Constants>::from_bytes(&bad).err(), Some(Error::Invalid(at)));
    assert_eq!(puns::nearest::from_bytes::<Constants>(&bad).err(), Some(Error::Invalid(at)));
    total.compared += stats.compared;
  }
  println!("{total:?}");
  assert!(total.compared > if cfg!(miri) { 100 } else { 10_000 });
}

/// An edited region is not compact: both reject its buffer, both accept its compact copy.
#[test]
fn an_edited_region() {
  let mut r = Region::new(Expr::<i32be>::add(Expr::Lit(1), Expr::Lit(2)));
  r.edit().update(Expr::neg);
  assert!(r.as_bytes().is_none());
  let compact = Region::new(&*r);
  corrupt::<Expr<i32be>>(compact.as_bytes().unwrap(), &mut Stats::default());
}

/// What `nearest` says of a type (`PLAIN`: copied as bytes; `ANY`: never checked; `REACH`: the
/// largest region) is what its layout says: the loaders refuse to compile for a type of which
/// `nearest` says more than the layout does.
#[test]
fn what_nearest_says_of_a_type() {
  fn says<T: Flat + Pun>() -> (bool, bool, u64) {
    // Compiles: the layout agrees.
    let _ = puns::nearest::view::<T>(&[]);
    (T::PLAIN, T::ANY, T::REACH)
  }
  assert_eq!(says::<u64>(), (true, true, u64::MAX));
  assert_eq!(says::<bool>(), (true, false, u64::MAX));
  assert_eq!(says::<[u16le; 3]>(), (true, true, u64::MAX));
  assert_eq!(says::<Kind>(), (true, false, u64::MAX));
  assert_eq!(says::<Table>().2, i32::MAX as u64 & !7);
  assert_eq!(says::<Near<u32, i16be>>(), (false, false, i16::MAX as u64 & !7));
  assert_eq!(says::<Option<Near<u32, i64le>>>(), (false, false, i64::MAX as u64 & !7));
  assert_eq!(says::<Item<i16>>(), (false, false, i16::MAX as u64 & !7));
  assert_eq!(says::<Expr<i64>>(), (false, false, i64::MAX as u64 & !7));
  // Padding is not initialized (not `PLAIN`), but nothing in it can be invalid (`ANY`).
  assert_eq!(says::<Padded>(), (false, true, u64::MAX));
}

/// Numbers of a given value, and of given values.
#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
struct Constants {
  magic: u16bec<0x0102>,
  version: u8c<3>,
  level: u8in<1, 5>,
  marks: Near<[u64lec<7>], i32le>,
}

#[flat]
#[derive(Pun, Clone, Copy)]
struct Padded {
  a: u8,
  b: u32,
}

// -------------------------------------------------------------------------------------------------
// A value of a region read as another type

/// The next version of the schema, which reads every image of the first as it is: a variant
/// added at the end of each enum, a `bool` read as a `u8`, a `char` as a `u32`, `[u16]` as
/// `[[u8; 2]]`.
mod v2 {
  use nearest::{Near, flat};
  use puns::{Pun, num::i32le};

  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
  pub enum Kind {
    File,
    Dir,
    Link,
    /// New.
    Socket,
  }

  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct Item {
    pub id: u32,
    /// Was a `bool`: there are more flags now.
    pub hidden: u8,
    pub kind: Kind,
    pub name: Near<str, i32le>,
    /// Was `[u16]`: read as bytes.
    pub tags: Near<[[u8; 2]], i32le>,
    pub next: Option<Near<Item, i32le>>,
  }

  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct Dir {
    pub size: u64,
    pub flags: [u8; 3],
    pub entries: Near<[Item], i32le>,
    pub parent: Option<Near<Dir, i32le>>,
  }

  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub enum Expr {
    Lit(i64),
    Neg(Near<Expr, i32le>),
    Add(Near<Expr, i32le>, Near<Expr, i32le>),
    Var(Near<str, i32le>),
    Call(Near<str, i32le>, Near<[Expr], i32le>),
    /// Was a `char`.
    Cond(Option<Near<Expr, i32le>>, u32),
    /// New.
    Mul(Near<Expr, i32le>, Near<Expr, i32le>),
  }

  /// Evaluates an expression; variables are 0, calls sum their arguments.
  pub fn eval(e: &Expr) -> i64 {
    match e {
      Expr::Lit(n) => *n,
      Expr::Neg(a) => eval(a).wrapping_neg(),
      Expr::Add(a, b) => eval(a).wrapping_add(eval(b)),
      Expr::Mul(a, b) => eval(a).wrapping_mul(eval(b)),
      Expr::Var(_) => 0,
      Expr::Call(_, args) => args.iter().map(eval).fold(0, i64::wrapping_add),
      Expr::Cond(c, k) => c.as_deref().map_or_else(|| i64::from(*k), eval),
    }
  }
}

type V1 = i32le;

#[test]
fn an_expression_as_its_next_version() {
  let r = Region::new(Expr::<V1>::call(
    "f",
    (
      Expr::add(Expr::Lit(2), Expr::neg(Expr::Lit(5))),
      Expr::var("x"),
      Expr::cond(Near::new(Expr::Lit(40)), 'z'),
      Expr::cond(None, 'A'),
    ),
  ));
  let new: &v2::Expr = puns::view_ref(&*r);
  // Nothing is copied: the same bytes.
  assert!(core::ptr::addr_eq(new, &raw const *r));
  assert_eq!(v2::eval(new), 2 - 5 + 40 + 65);
  let v2::Expr::Call(name, args) = new else { panic!() };
  assert_eq!(&**name, "f");
  assert!(matches!(&args[3], v2::Expr::Cond(None, 65)));
  // A deep copy of the view is a region of the new type, which both check as one.
  let copy = Region::<v2::Expr>::new(new);
  assert_eq!(nearest::view::<v2::Expr>(copy.as_bytes().unwrap()), Ok(new));
  assert_eq!(puns::nearest::view::<v2::Expr>(copy.as_bytes().unwrap()), Ok(new));
  // The image of the old one is the image of the new one here: the same sizes everywhere.
  assert_eq!(copy.as_bytes(), r.as_bytes());
}

#[test]
fn a_directory_as_its_next_version() {
  let entry = Region::new(Item::<V1>::new(
    7,
    true,
    Kind::Link,
    "a",
    &[0x0102u16, 0x0304][..],
    Near::new(Item::new(8, false, Kind::File, "b", (), None)),
  ));
  let r = Region::new(Dir::<V1>::new(
    1 << 40,
    [true, false, true],
    Near::from_iter([&*entry, &*entry]),
    Near::new(Dir::new(3, [false; 3], (), None)),
  ));
  let new: &v2::Dir = puns::view_ref(&*r);
  assert_eq!((new.size, new.flags, new.entries.len()), (1 << 40, [1, 0, 1], 2));
  let e = &new.entries[1];
  assert_eq!((e.id, e.hidden, e.kind, &*e.name), (7, 1, v2::Kind::Link, "a"));
  assert_eq!(&e.tags[..], &[0x0102u16.to_ne_bytes(), 0x0304u16.to_ne_bytes()][..]);
  let next = e.next.as_deref().unwrap();
  assert_eq!((next.id, next.kind, &*next.name, next.tags.len()), (8, v2::Kind::File, "b", 0));
  assert!(next.next.is_none());
  assert_eq!(new.parent.as_ref().unwrap().size, 3);
  // The elements of a list, as a slice of the new ones.
  let entries: &[v2::Item] = puns::view_ref(&r.entries[..]);
  assert_eq!(entries.len(), 2);
  assert!(core::ptr::addr_eq(entries, &raw const r.entries[..]));
  assert_eq!(&*entries[0].next.as_ref().unwrap().name, "b");
}

/// Bytes checked when loaded, then read as the next version.
#[test]
fn loaded_then_read_as_another_type() {
  let r = Region::new(Expr::<V1>::neg(Expr::add(Expr::Lit(1), Expr::var("y"))));
  let old: &Expr<V1> = puns::nearest::view(r.as_bytes().unwrap()).unwrap();
  assert_eq!(v2::eval(puns::view_ref(old)), -1);
}

/// What only some values are: checked, for the value itself.
#[test]
fn checked_at_run_time() {
  #[flat]
  #[derive(Pun)]
  struct Maybe {
    level: u8,
    owner: Option<Near<u32, i32le>>,
    more: Near<[Maybe], i32le>,
  }

  #[flat]
  #[derive(Pun)]
  struct Named {
    #[pun(valid(..=3))]
    level: u8,
    owner: Near<u32, i32le>,
    more: Near<[Maybe], i32le>,
  }

  let owned = Region::new(Maybe::new(1, Near::new(7), [Maybe::new(9, None, ())]));
  let view = puns::try_view_ref::<Maybe, Named>(&owned).unwrap();
  assert_eq!((view.level, *view.owner, view.more[0].level), (1, 7, 9));
  let anonymous = Region::new(Maybe::new(2, None, ()));
  assert_eq!(puns::try_view_ref::<Maybe, Named>(&anonymous).err(), Some(puns::Error::Validity));
  let high = Region::new(Maybe::new(4, Near::new(7), ()));
  assert_eq!(puns::try_view_ref::<Maybe, Named>(&high).err(), Some(puns::Error::Validity));
  // A variant that the target does not have.
  let lit = Region::new(Expr::<V1>::Lit(3));
  assert!(puns::try_view_ref::<Expr<V1>, Small>(&lit).is_ok_and(|small| *small == Small::Lit(3)));
  let var = Region::new(Expr::<V1>::var("x"));
  assert!(puns::try_view_ref::<Expr<V1>, Small>(&var).is_err());
}

/// The first variants of `Expr`.
#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
enum Small {
  Lit(i64),
  Neg(Near<Expr<V1>, V1>),
}

/// A value that holds pointers is read as plain data: an offset is initialized bytes.
#[test]
fn pointers_are_read_as_bytes() {
  let r = Region::new(Table::new(1, u32le::new(2), [u16le::new(3)]));
  assert_eq!(puns::as_bytes(&*r), [1, 2, 0, 0, 0, 11, 0, 0, 0]);
  assert_eq!(puns::head_ref::<Table, u8>(&r), &1);
  assert_eq!(puns::tail_ref::<Table, u32le>(&r).get(), 11);
  assert_eq!(puns::try_pun_ref::<Table, (bool, [u8; 8])>(&r).map(|pair| pair.0), Ok(true));
}

// -------------------------------------------------------------------------------------------------
// What `#[derive(Pun)]` is told of a type, checked when a region is loaded

#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
#[pun(assert(lo <= hi))]
struct Span {
  lo: u16le,
  hi: u16le,
}

#[flat]
#[derive(Pun, Debug, PartialEq, Eq)]
#[pun(assert(count == 2 * pairs))]
struct Checked {
  #[pun(valid(1..=3))]
  level: u8,
  count: u8,
  pairs: u8,
  spans: Near<[Span], i32le>,
  next: Option<Near<Checked, i32le>>,
}

#[test]
fn rules_are_checked_at_load() {
  let span = |lo, hi| Span::new(u16le::new(lo), u16le::new(hi));
  let r = Region::new(Checked::new(
    1,
    4,
    2,
    [span(1, 2), span(3, 3)],
    Near::new(Checked::new(3, 0, 0, (), None)),
  ));
  let image = r.as_bytes().unwrap().to_vec();
  assert_eq!(puns::nearest::from_bytes::<Checked>(&image).as_deref(), Ok(&*r));
  // Where the spans and the next one are in the image.
  let base = r.as_bytes().unwrap().as_ptr().addr();
  let spans = (&raw const r.spans[0]).addr() - base;
  let next = (&raw const **r.next.as_ref().unwrap()).addr() - base;
  // What `nearest` accepts, since it knows nothing of the rules, is refused where a rule fails.
  for (at, value, error) in [
    (0, 4, 0),             // `level`, out of its valid values
    (1, 5, 0),             // `count == 2 * pairs`, of the root
    (spans + 2, 0, spans), // `lo <= hi`, of the first span
    (spans + 4, 4, spans + 4),
    (next, 0, next),     // `level`, behind a pointer
    (next + 1, 1, next), // `count == 2 * pairs`, behind a pointer
  ] {
    let mut bad = image.clone();
    bad[at] = value;
    assert!(Region::<Checked>::from_bytes(&bad).is_ok(), "at {at}");
    assert_eq!(puns::nearest::from_bytes::<Checked>(&bad).err(), Some(Error::Invalid(error)));
  }
}
