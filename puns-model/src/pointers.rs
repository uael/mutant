//! Model check of self-relative pointers: what `puns::view_ref` and the loaders of
//! `puns::nearest` accept, against an oracle that does not use the analysis.
//!
//! A type of the model is described by hand ([`Desc`]): its plain bytes and what they may be, its
//! fields and variants at the offsets that the compiler gives, its pointers and what they aim at.
//! The oracle is a reference validator ([`valid`]) that walks concrete bytes with such a
//! description, as a debugger would: every byte it reads must be in bounds and initialized,
//! every value valid, every pointer must lead to a place that is aligned for its target and
//! holds one, and so on behind it. It knows nothing of layouts, of pairs of types or of
//! instances of functions.
//!
//! - [`check_pair`]: every value of a corpus of regions of `S` ([`Model::corpus`]: every shape up
//!   to a small depth, every value of a small alphabet), with the bytes that its type writes
//!   marked as initialized ([`mark`]), is read as a `D` by the oracle. If `view_ref::<S, D>`
//!   compiles (`hook::views`, which asks the code of the library that decides it), every one must
//!   be a valid `D`: else the verdict is **unsound**. Then the view is made and read through, for
//!   Miri. `try_view_ref` must accept the values that are a `D`, and only them. A pair that is
//!   refused while every value of the corpus is a `D` is **imprecise**, and counted.
//! - [`check_load`]: every image of the corpus of `T`, and its corruptions, are given to the
//!   loader of `puns` and to the one of `nearest`, which must agree; and what `puns` accepts must
//!   be a valid `T` for the oracle.
//!
//! [`sound`] tells what the oracle alone says of a pair, for the pairs whose answer is known: an
//! oracle that would accept everything would check nothing.

use std::{collections::BTreeMap, fmt::Debug, sync::Mutex};

use nearest::{Flat, Region};
use puns::{Pun, hook};

/// How many pointers `hook::views` follows: more than the pairs of types that a pair of the
/// model can meet.
pub const FUEL: usize = 6;

/// What a pointer aims at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
  /// One value.
  One,
  /// A length, then that many elements; a capacity precedes the length.
  List,
  /// A length, then that many bytes of UTF-8.
  Str,
}

/// What a type is, written by hand.
#[derive(Clone)]
pub enum Desc {
  /// Bytes that are a value if `valid` says so.
  Plain { size: usize, align: usize, valid: fn(&[u8]) -> bool },
  /// Fields at their offsets: the bytes that none covers are padding.
  Struct { size: usize, align: usize, fields: Vec<(usize, Desc)> },
  /// A tag, a byte at 0, and for each tag the fields of its variant at their offsets.
  Enum { size: usize, align: usize, variants: Vec<(u8, Vec<(usize, Desc)>)> },
  /// An offset of 4 bytes, of no alignment, in a byte order: the distance from itself to what
  /// `target` describes (an element, if it aims at a list). Zero is no pointer.
  Pointer { big: bool, nullable: bool, aim: Aim, target: fn() -> Desc },
}

impl Desc {
  #[must_use]
  pub const fn size(&self) -> usize {
    match self {
      Self::Plain { size, .. } | Self::Struct { size, .. } | Self::Enum { size, .. } => *size,
      Self::Pointer { .. } => 4,
    }
  }

  #[must_use]
  pub const fn align(&self) -> usize {
    match self {
      Self::Plain { align, .. } | Self::Struct { align, .. } | Self::Enum { align, .. } => *align,
      Self::Pointer { .. } => 1,
    }
  }
}

/// The bytes of a buffer whose first byte is at a multiple of 8, and which of them a value was
/// written to: the others are padding, or gaps between values.
pub struct Image {
  pub bytes: Vec<u8>,
  pub init: Vec<bool>,
}

impl Image {
  /// The `size` bytes at `at`, which must be in bounds and written to.
  fn read(&self, at: usize, size: usize) -> Result<&[u8], String> {
    let end = at.checked_add(size).filter(|end| *end <= self.bytes.len());
    let end = end.ok_or_else(|| format!("{size} bytes at {at} are out of bounds"))?;
    if self.init[at..end].iter().all(|init| *init) {
      Ok(&self.bytes[at..end])
    } else {
      Err(format!("a byte at {at}..{end} is not initialized"))
    }
  }

  /// The integer of 4 bytes at `at`, in a byte order.
  fn int(&self, at: usize, big: bool) -> Result<i64, String> {
    let bytes: [u8; 4] = self.read(at, 4)?.try_into().map_err(|_| "4 bytes".to_owned())?;
    Ok(i64::from(if big { i32::from_be_bytes(bytes) } else { i32::from_le_bytes(bytes) }))
  }
}

/// Where the pointer at `at` aims, `None` if it is zero.
fn aim(image: &Image, at: usize, big: bool) -> Result<Option<usize>, String> {
  let offset = image.int(at, big)?;
  if offset == 0 {
    return Ok(None);
  }
  let to = i64::try_from(at).ok().and_then(|at| at.checked_add(offset));
  let to = to.and_then(|to| usize::try_from(to).ok()).filter(|to| *to <= image.bytes.len());
  to.map(Some).ok_or_else(|| format!("the pointer at {at} aims out of the buffer"))
}

/// The length at `at`, which is not negative.
fn length(image: &Image, at: usize, big: bool) -> Result<usize, String> {
  usize::try_from(image.int(at, big)?).map_err(|_| format!("a negative length at {at}"))
}

/// Whatever a value that `desc` describes can be read at `at`: the reference validator.
///
/// # Errors
/// What is wrong, if something is.
pub fn valid(desc: &Desc, image: &Image, at: usize) -> Result<(), String> {
  if !at.is_multiple_of(desc.align()) {
    return Err(format!("{at} is not aligned to {}", desc.align()));
  }
  match desc {
    Desc::Plain { size, valid, .. } => {
      if valid(image.read(at, *size)?) {
        Ok(())
      } else {
        Err(format!("an invalid value at {at}"))
      }
    }
    Desc::Struct { fields, .. } => {
      fields.iter().try_for_each(|(offset, field)| valid(field, image, at + offset))
    }
    Desc::Enum { variants, .. } => {
      let tag = image.read(at, 1)?[0];
      let (_, fields) = (variants.iter().find(|(of, _)| *of == tag))
        .ok_or_else(|| format!("{tag} at {at} is not a tag"))?;
      fields.iter().try_for_each(|(offset, field)| valid(field, image, at + offset))
    }
    Desc::Pointer { big, nullable, aim: what, target } => {
      let Some(to) = aim(image, at, *big)? else {
        return if *nullable { Ok(()) } else { Err(format!("a null pointer at {at}")) };
      };
      let target = target();
      match what {
        Aim::One => valid(&target, image, to),
        Aim::List => {
          let len = length(image, to, *big)?;
          (0..len).try_for_each(|i| valid(&target, image, to + 4 + i * target.size()))
        }
        Aim::Str => {
          let len = length(image, to, *big)?;
          match std::str::from_utf8(image.read(to + 4, len)?) {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("text at {} that is not UTF-8", to + 4)),
          }
        }
      }
    }
  }
}

/// Marks as initialized the bytes that the value at `at`, which `desc` describes, was written
/// to, and the ones of everything it points to: what is left is padding.
///
/// # Panics
/// If the value is not one: the description, or the corpus, is wrong.
pub fn mark(desc: &Desc, image: &mut Image, at: usize) {
  match desc {
    Desc::Plain { size, .. } => image.init[at..at + size].fill(true),
    Desc::Struct { fields, .. } => {
      for (offset, field) in fields {
        mark(field, image, at + offset);
      }
    }
    Desc::Enum { variants, .. } => {
      image.init[at] = true;
      let tag = image.bytes[at];
      let (_, fields) = variants.iter().find(|(of, _)| *of == tag).expect("a tag");
      for (offset, field) in fields {
        mark(field, image, at + offset);
      }
    }
    Desc::Pointer { big, aim: what, target, .. } => {
      image.init[at..at + 4].fill(true);
      let Some(to) = aim(image, at, *big).expect("a pointer") else { return };
      let target = target();
      match what {
        Aim::One => mark(&target, image, to),
        Aim::List => {
          // The capacity and the length.
          image.init[to - 4..to + 4].fill(true);
          let len = length(image, to, *big).expect("a length");
          for i in 0..len {
            mark(&target, image, to + 4 + i * target.size());
          }
        }
        Aim::Str => {
          image.init[to..to + 4].fill(true);
          let len = length(image, to, *big).expect("a length");
          image.init[to + 4..to + 4 + len].fill(true);
        }
      }
    }
  }
}

/// A type of the model: its description, and values of it.
pub trait Model: Flat + Pun + Debug + PartialEq + 'static {
  fn desc() -> Desc;

  /// Regions of values of the type: every shape up to a small depth, with every value of a
  /// small alphabet.
  fn corpus() -> Vec<Region<Self>>;
}

fn name<T>() -> &'static str {
  core::any::type_name::<T>().rsplit("::").next().unwrap_or_default()
}

/// The image of `region`, with what a `T` writes marked as initialized.
///
/// # Panics
/// If the oracle does not find the value valid: the description, or the corpus, is wrong.
#[must_use]
pub fn image<T: Model>(region: &Region<T>) -> Image {
  let bytes = region.as_bytes().expect("a compact region").to_vec();
  let mut image = Image { init: vec![false; bytes.len()], bytes };
  mark(&T::desc(), &mut image, 0);
  if let Err(why) = valid(&T::desc(), &image, 0) {
    panic!("the oracle refuses a {} of the corpus: {why}", name::<T>());
  }
  image
}

/// What the oracle alone says of reading an `S` as a `D`: whatever every value of the corpus of
/// `S` is a valid `D`.
#[must_use]
pub fn sound<S: Model, D: Model>() -> bool {
  let target = D::desc();
  S::corpus().iter().all(|region| valid(&target, &image(region), 0).is_ok())
}

/// The verdicts that are imprecise, by cause.
pub static IMPRECISE: Mutex<BTreeMap<&'static str, Vec<String>>> = Mutex::new(BTreeMap::new());

fn imprecise(cause: &'static str, pair: &str) {
  IMPRECISE.lock().unwrap().entry(cause).or_default().push(pair.to_owned());
}

/// Checks what the library does of reading an `S` as a `D` against the oracle, on every value of
/// the corpus of `S`.
///
/// # Panics
/// If a verdict is unsound (a value that is not a `D` is read as one) or incorrect (a value that
/// is a `D` is refused at run time).
pub fn check_pair<S: Model, D: Model>() {
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (views, tries) = const { (hook::views::<S, D>(FUEL), hook::tries_view::<S, D>(FUEL)) };
  let target = D::desc();
  let (mut all, mut some) = (true, false);
  for region in S::corpus() {
    let verdict = valid(&target, &image(&region), 0);
    (all, some) = (all && verdict.is_ok(), some || verdict.is_ok());
    if let Some(view) = hook::view_ref::<S, D, FUEL>(&*region) {
      if let Err(why) = &verdict {
        panic!("UNSOUND: {pair}: `view_ref` reads {:?} as a target, but {why}", *region);
      }
      // All of it, pointers followed: what Miri is there to see.
      let _ = format!("{view:?}");
    }
    match (hook::try_view_ref::<S, D, FUEL>(&*region), &verdict) {
      (Some(Ok(_)), Err(why)) => {
        panic!("UNSOUND: {pair}: `try_view_ref` accepts {:?}, but {why}", *region)
      }
      (Some(Err(error)), Ok(())) => {
        panic!("INCORRECT: {pair}: `try_view_ref` refuses {:?}, a valid target: {error}", *region)
      }
      (Some(Ok(view)), Ok(())) => {
        let _ = format!("{view:?}");
      }
      (Some(Err(_)), Err(_)) | (None, _) => {}
    }
  }
  assert!(tries || !views, "{pair}: what is proven can be tried");
  if all && !views {
    imprecise("`view_ref` refused, while every value of the corpus is a target", &pair);
  }
  if some && !tries {
    imprecise("`try_view_ref` refused, while some values of the corpus are targets", &pair);
  }
}

/// Checks the loader of `puns` against the one of `nearest` and against the oracle: on every
/// image of the corpus of `T`, with each byte changed by each of `masks`, and cut anywhere.
///
/// # Panics
/// If the loaders disagree, or if `puns` accepts bytes that the oracle does not find to be a
/// valid `T`.
pub fn check_load<T: Model>(masks: &[u8]) {
  // At an address that is a multiple of 8, wherever the buffer of a region is.
  let aligned = |bytes: &[u8]| {
    let mut buffer = vec![0; bytes.len() + 8];
    let start = buffer.as_ptr().addr().wrapping_neg() % 8;
    buffer[start..start + bytes.len()].copy_from_slice(bytes);
    (buffer, start)
  };
  let desc = T::desc();
  let both = |bytes: &[u8]| {
    let (ours, theirs) = (puns::nearest::view::<T>(bytes), nearest::view::<T>(bytes));
    assert_eq!(ours, theirs, "{}: the loaders disagree on {bytes:?}", name::<T>());
    if ours.is_ok() {
      // Every byte of what comes from outside is initialized.
      let image = Image { bytes: bytes.to_vec(), init: vec![true; bytes.len()] };
      if let Err(why) = valid(&desc, &image, 0) {
        panic!("UNSOUND: {}: loaded from {bytes:?}, but {why}", name::<T>());
      }
    }
  };
  for region in T::corpus() {
    let (mut buffer, start) = aligned(region.as_bytes().expect("a compact region"));
    let len = buffer.len() - 8;
    both(&buffer[start..start + len]);
    for at in 0..len {
      for mask in masks {
        buffer[start + at] ^= mask;
        both(&buffer[start..start + len]);
        buffer[start + at] ^= mask;
      }
      both(&buffer[start..start + at]);
    }
  }
}

/// Prints the verdicts that were found imprecise.
///
/// # Panics
/// If a test panicked while it held the report.
pub fn report() {
  for (cause, pairs) in IMPRECISE.lock().unwrap().iter() {
    println!("   imprecise, {cause}: {}", pairs.len());
    for pair in pairs {
      println!("      {pair}");
    }
  }
}

/// `$check` for every pair of the types, in both orders and with themselves.
#[macro_export]
macro_rules! pointer_pairs {
  ($check:ident; $($ty:ty),+) => {
    $crate::pointer_pairs!(@ $check; ($($ty),+); $($ty),+);
  };
  (@ $check:ident; $targets:tt; $($source:ty),+) => {
    $($crate::pointer_pairs!(@@ $check; $source; $targets);)+
  };
  (@@ $check:ident; $source:ty; ($($target:ty),+)) => {
    $($crate::pointers::$check::<$source, $target>();)+
  };
}

/// The types of the model.
pub mod types {
  use core::mem::offset_of;

  use nearest::{Emit, Near, Region, flat};
  use puns::{
    Pun,
    num::{i32be, i32le, u16le},
  };

  use super::{Aim, Desc, Model};

  /// The offset of the pointers of the model, but for one type.
  pub type O = i32le;

  const fn any(_: &[u8]) -> bool {
    true
  }

  const fn zero_or_one(bytes: &[u8]) -> bool {
    bytes[0] <= 1
  }

  const fn bytes(size: usize) -> Desc {
    Desc::Plain { size, align: 1, valid: any }
  }

  const fn flag() -> Desc {
    Desc::Plain { size: 1, align: 1, valid: zero_or_one }
  }

  const fn pointer(nullable: bool, aim: Aim, target: fn() -> Desc) -> Desc {
    Desc::Pointer { big: false, nullable, aim, target }
  }

  const fn structure<T>(fields: Vec<(usize, Desc)>) -> Desc {
    Desc::Struct { size: size_of::<T>(), align: align_of::<T>(), fields }
  }

  /// A chain of values, each made by `link` and held by the `next` of the one before.
  macro_rules! chains {
    ($ty:ident: $alphabet:expr) => {
      fn corpus() -> Vec<Region<Self>> {
        let alphabet = $alphabet;
        let mut corpus = vec![];
        for first in alphabet {
          corpus.push(Region::new($ty::new(first, None)));
          for second in alphabet {
            let mut region = Region::new($ty::new(first, None));
            region.edit().next().insert($ty::new(second, None));
            corpus.push(Region::new(&*region));
          }
        }
        let mut long = Region::new($ty::new(alphabet[0], None));
        let last = alphabet[alphabet.len() - 1];
        long.edit().next().insert($ty::new(last, None)).next().insert($ty::new(last, None));
        corpus.push(Region::new(&*long));
        corpus
      }
    };
  }

  /// A recursive type: a `bool`, and maybe one more of the same.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct Flags {
    pub flag: bool,
    pub next: Option<Near<Flags, O>>,
  }

  impl Model for Flags {
    fn desc() -> Desc {
      structure::<Self>(vec![
        (offset_of!(Self, flag), flag()),
        (offset_of!(Self, next), pointer(true, Aim::One, Self::desc)),
      ])
    }

    chains!(Flags: [false, true]);
  }

  /// The same with a `u8`.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct Bytes {
    pub flag: u8,
    pub next: Option<Near<Bytes, O>>,
  }

  impl Model for Bytes {
    fn desc() -> Desc {
      structure::<Self>(vec![
        (offset_of!(Self, flag), bytes(1)),
        (offset_of!(Self, next), pointer(true, Aim::One, Self::desc)),
      ])
    }

    chains!(Bytes: [0, 1, 2, 255]);
  }

  /// Lists of values of the same type, one level or two.
  macro_rules! lists {
    ($ty:ident: $alphabet:expr) => {
      fn corpus() -> Vec<Region<Self>> {
        let alphabet = $alphabet;
        let leaf = |value| $ty::new(value, ());
        let mut corpus = vec![];
        for first in alphabet {
          corpus.push(Region::new(leaf(first)));
          for second in alphabet {
            corpus.push(Region::new($ty::new(first, [leaf(second)])));
            corpus.push(Region::new($ty::new(first, [leaf(second), leaf(first)])));
            let deep = $ty::new(second, (leaf(first), $ty::new(first, [leaf(second)])));
            corpus.push(Region::new($ty::new(first, (leaf(second), deep))));
          }
        }
        corpus
      }
    };
  }

  /// A recursive type through a list: a `bool`, and more of the same.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct FlagList {
    pub flag: bool,
    pub rest: Near<[FlagList], O>,
  }

  impl Model for FlagList {
    fn desc() -> Desc {
      structure::<Self>(vec![
        (offset_of!(Self, flag), flag()),
        (offset_of!(Self, rest), pointer(false, Aim::List, Self::desc)),
      ])
    }

    lists!(FlagList: [false, true]);
  }

  /// The same with a `u8`.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ByteList {
    pub flag: u8,
    pub rest: Near<[ByteList], O>,
  }

  impl Model for ByteList {
    fn desc() -> Desc {
      structure::<Self>(vec![
        (offset_of!(Self, flag), bytes(1)),
        (offset_of!(Self, rest), pointer(false, Aim::List, Self::desc)),
      ])
    }

    lists!(ByteList: [0, 1, 2, 255]);
  }

  /// A struct that is one pointer: its description, and a region for each of the `$values`
  /// that the pointer aims at.
  macro_rules! holder {
    ($ty:ident: $desc:expr; $build:expr; $values:expr) => {
      impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![(offset_of!(Self, to), $desc)])
        }

        fn corpus() -> Vec<Region<Self>> {
          $values.into_iter().map($build).collect()
        }
      }
    };
  }

  fn region<T: Model>(builder: impl Emit<T>) -> Region<T> {
    Region::new(builder)
  }

  /// A pointer to a `bool`.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToFlag {
    pub to: Near<bool, O>,
  }

  /// A pointer to a `u8`.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToByte {
    pub to: Near<u8, O>,
  }

  /// A pointer to a `u8`, or none.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct MaybeByte {
    pub to: Option<Near<u8, O>>,
  }

  /// A pointer to a `u8`, whose offset is big-endian.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToByteBe {
    pub to: Near<u8, i32be>,
  }

  /// A pointer to two bytes that are a number.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToWord {
    pub to: Near<u16le, O>,
  }

  /// A pointer to two bytes.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToPair {
    pub to: Near<[u8; 2], O>,
  }

  /// A pointer to a list of bytes.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToBytes {
    pub to: Near<[u8], O>,
  }

  /// A pointer to a list of `bool`s.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToFlags {
    pub to: Near<[bool], O>,
  }

  /// A pointer to text.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub struct ToText {
    pub to: Near<str, O>,
  }

  holder!(ToFlag: pointer(false, Aim::One, flag); |value| region(ToFlag::new(value)); [false, true]);
  holder! {
    ToByte: pointer(false, Aim::One, || bytes(1));
    |value| region(ToByte::new(value)); [0u8, 1, 2, 255]
  }
  holder! {
    MaybeByte: pointer(true, Aim::One, || bytes(1));
    |value: Option<u8>| value.map_or_else(
      || region(MaybeByte::new(None)),
      |value| region(MaybeByte::new(Near::new(value))),
    );
    [None, Some(0u8), Some(1), Some(2), Some(255)]
  }
  holder! {
    ToByteBe: Desc::Pointer { big: true, nullable: false, aim: Aim::One, target: || bytes(1) };
    |value| region(ToByteBe::new(value)); [0u8, 1, 2, 255]
  }
  holder! {
    ToWord: pointer(false, Aim::One, || bytes(2));
    |value| region(ToWord::new(u16le::new(value))); [0u16, 1, 0x0100, 0x0201, 0xffff]
  }
  holder! {
    ToPair: pointer(false, Aim::One, || bytes(2));
    |value| region(ToPair::new(value)); [[0u8, 0], [1, 0], [0, 1], [2, 1], [255, 255]]
  }
  holder! {
    ToBytes: pointer(false, Aim::List, || bytes(1));
    |value: &[u8]| region(ToBytes::new(value));
    [&[][..], &[0][..], &[1, 2][..], &[255, 0, 1][..], &[97][..]]
  }
  holder! {
    ToFlags: pointer(false, Aim::List, flag);
    |value: &[bool]| region(ToFlags::new(value)); [&[][..], &[true][..], &[false, true][..]]
  }
  holder! {
    ToText: pointer(false, Aim::Str, || bytes(1));
    |value| region(ToText::new(value)); ["", "a", "\u{e9}", "ab"]
  }

  /// Four bytes: what a pointer is made of, and is not.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
  pub struct Raw {
    pub raw: [u8; 4],
  }

  impl Model for Raw {
    fn desc() -> Desc {
      structure::<Self>(vec![(offset_of!(Self, raw), bytes(4))])
    }

    fn corpus() -> Vec<Region<Self>> {
      [[0; 4], [1, 0, 0, 0], [4, 0, 0, 0], [0, 0, 0, 4], [0xff; 4], [8, 0, 0, 0]]
        .into_iter()
        .map(|raw| Region::new(Raw { raw }))
        .collect()
    }
  }

  /// An enum with fields: a `bool`, or a pointer to one more of the same.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub enum Tree {
    Leaf(bool),
    Node(Near<Tree, O>),
  }

  impl Model for Tree {
    fn desc() -> Desc {
      Desc::Enum {
        size: size_of::<Self>(),
        align: align_of::<Self>(),
        variants: vec![
          (0, vec![(1, flag())]),
          (1, vec![(1, pointer(false, Aim::One, Self::desc))]),
        ],
      }
    }

    fn corpus() -> Vec<Region<Self>> {
      let mut corpus = vec![];
      for value in [false, true] {
        corpus.push(Region::new(Tree::Leaf(value)));
        corpus.push(Region::new(Tree::node(Tree::Leaf(value))));
        corpus.push(Region::new(Tree::node(Tree::node(Tree::Leaf(value)))));
      }
      corpus
    }
  }

  /// The same with a `u8`, and one more variant: a list.
  #[flat]
  #[derive(Pun, Debug, PartialEq, Eq)]
  pub enum Wood {
    Leaf(u8),
    Node(Near<Wood, O>),
    Pair(Near<[Wood], O>),
  }

  impl Model for Wood {
    fn desc() -> Desc {
      Desc::Enum {
        size: size_of::<Self>(),
        align: align_of::<Self>(),
        variants: vec![
          (0, vec![(1, bytes(1))]),
          (1, vec![(1, pointer(false, Aim::One, Self::desc))]),
          (2, vec![(1, pointer(false, Aim::List, Self::desc))]),
        ],
      }
    }

    fn corpus() -> Vec<Region<Self>> {
      let mut corpus = vec![];
      for value in [0, 1, 2, 255] {
        corpus.push(Region::new(Wood::Leaf(value)));
        corpus.push(Region::new(Wood::node(Wood::Leaf(value))));
        corpus.push(Region::new(Wood::node(Wood::node(Wood::Leaf(value)))));
        corpus.push(Region::new(Wood::pair([Wood::Leaf(value), Wood::Leaf(1)])));
        corpus.push(Region::new(Wood::pair((Wood::node(Wood::Leaf(value)), Wood::pair(())))));
      }
      corpus
    }
  }
}
