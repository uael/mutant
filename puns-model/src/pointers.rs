//! Model check of self-relative pointers: what `puns::view_ref` and `puns::try_view_ref` accept,
//! against an oracle that does not use the analysis.
//!
//! The pointers are the ones of the tests of `puns` ([`relative`](crate::relative): an offset of
//! a given byte order to one value, to a sequence whose length is before it, or to text whose
//! length is before it; one that may be null), declared through the public API only.
//!
//! A type of the model is described by hand ([`Desc`]): its plain bytes and what they may be, its
//! fields and variants at their offsets, its pointers and what they aim at. The oracle is a
//! reference validator ([`valid`]) that walks concrete bytes with such a description, as a
//! debugger would: every byte it reads must be in bounds and initialized, every value valid,
//! every pointer must lead to a place that is aligned for its target and holds one, and so on
//! behind it. It knows nothing of layouts, of pairs of types or of instances of functions.
//!
//! [`check_pair`]: every value of a corpus of `S` ([`Model::corpus`]: every shape up to a small
//! depth, every value of a small alphabet), with the bytes that its type writes marked as
//! initialized ([`mark`]), is read as a `D` by the oracle. If `view_ref::<S, D>` compiles
//! (`hook::views`, which asks the code of the library that decides it), every one must be a valid
//! `D`: else the verdict is **unsound**. Then the view is made and read through
//! ([`Model::show`]), for Miri. `try_view_ref` must accept the values that are a `D`, and only
//! them. A pair that is refused while every value of the corpus is a `D` is **imprecise**, and
//! counted.
//!
//! [`sound`] tells what the oracle alone says of a pair, for the pairs whose answer is known: an
//! oracle that would accept everything would check nothing.

use std::{collections::BTreeMap, mem::MaybeUninit, sync::Mutex};

use puns::{Pun, hook};

use crate::relative::{Fix, Region};

/// How many pointers `hook::views` follows: more than the pairs of types that a pair of the
/// model can meet.
pub const FUEL: usize = 6;

/// What a pointer aims at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
  /// One value.
  One,
  /// Values one after the other, the first at a multiple of the larger of their alignment and
  /// 4, their number right before it.
  Seq,
  /// Bytes of UTF-8, their number right before the first.
  Text,
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
  /// `target` describes (the first element of a sequence, the first byte of text). Zero is no
  /// pointer.
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

/// The bytes of a buffer, and which of them a value was written to: the others are padding, or
/// gaps between values, and are never read.
pub struct Image {
  bytes: Vec<MaybeUninit<u8>>,
  init: Vec<bool>,
}

impl Image {
  /// The `size` bytes at `at`, which must be in bounds and written to.
  fn read(&self, at: usize, size: usize) -> Result<Vec<u8>, String> {
    let end = at.checked_add(size).filter(|end| *end <= self.bytes.len());
    let end = end.ok_or_else(|| format!("{size} bytes at {at} are out of bounds"))?;
    if !self.init[at..end].iter().all(|init| *init) {
      return Err(format!("a byte at {at}..{end} is not initialized"));
    }
    // SAFETY: bytes that a value was written to, as the description of the type that wrote them
    // says (`mark`): a description that says so of a byte of padding is wrong, which Miri reports
    // here.
    Ok(self.bytes[at..end].iter().map(|byte| unsafe { byte.assume_init() }).collect())
  }

  /// Marks the `size` bytes at `at` as written to, and returns them.
  fn write(&mut self, at: usize, size: usize) -> Vec<u8> {
    self.init[at..at + size].fill(true);
    self.read(at, size).expect("written bytes")
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

/// The length right before `to`, which is not negative.
fn length(image: &Image, to: usize, big: bool) -> Result<usize, String> {
  let at = to.checked_sub(4).ok_or_else(|| format!("no length before {to}"))?;
  usize::try_from(image.int(at, big)?).map_err(|_| format!("a negative length at {at}"))
}

/// Whatever a value that `desc` describes can be read at `at`: the reference validator.
///
/// # Errors
/// What is wrong, if something is.
pub fn valid(desc: &Desc, image: &Image, at: usize) -> Result<(), String> {
  let mut fuel = usize::MAX;
  valid_with(desc, image, at, &mut fuel)
}

/// What the oracle says when it follows a pointer and has no fuel left: bytes whose pointers go
/// round in a circle, which a corruption can make, and of which it says nothing.
pub const NO_FUEL: &str = "no fuel";

/// [`valid`], following at most `fuel` pointers (each pointer followed takes one).
///
/// # Errors
/// What is wrong, if something is; [`NO_FUEL`] if it ran out of fuel first.
pub fn valid_with(desc: &Desc, image: &Image, at: usize, fuel: &mut usize) -> Result<(), String> {
  if !at.is_multiple_of(desc.align()) {
    return Err(format!("{at} is not aligned to {}", desc.align()));
  }
  match desc {
    Desc::Plain { size, valid, .. } => {
      if valid(&image.read(at, *size)?) {
        Ok(())
      } else {
        Err(format!("an invalid value at {at}"))
      }
    }
    Desc::Struct { fields, .. } => {
      fields.iter().try_for_each(|(offset, field)| valid_with(field, image, at + offset, fuel))
    }
    Desc::Enum { variants, .. } => {
      let tag = image.read(at, 1)?[0];
      let (_, fields) = (variants.iter().find(|(of, _)| *of == tag))
        .ok_or_else(|| format!("{tag} at {at} is not a tag"))?;
      fields.iter().try_for_each(|(offset, field)| valid_with(field, image, at + offset, fuel))
    }
    Desc::Pointer { big, nullable, aim: what, target } => {
      let Some(to) = aim(image, at, *big)? else {
        return if *nullable { Ok(()) } else { Err(format!("a null pointer at {at}")) };
      };
      *fuel = fuel.checked_sub(1).ok_or(NO_FUEL)?;
      let target = target();
      match what {
        Aim::One => valid_with(&target, image, to, fuel),
        Aim::Seq => {
          let first = target.align().max(4);
          if !to.is_multiple_of(first) {
            return Err(format!("a sequence at {to} that is not aligned to {first}"));
          }
          let len = length(image, to, *big)?;
          (0..len).try_for_each(|i| valid_with(&target, image, to + i * target.size(), fuel))
        }
        Aim::Text => {
          let len = length(image, to, *big)?;
          match std::str::from_utf8(&image.read(to, len)?) {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("text at {to} that is not UTF-8")),
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
    Desc::Plain { size, .. } => {
      image.write(at, *size);
    }
    Desc::Struct { fields, .. } => {
      for (offset, field) in fields {
        mark(field, image, at + offset);
      }
    }
    Desc::Enum { variants, .. } => {
      let tag = image.write(at, 1)[0];
      let (_, fields) = variants.iter().find(|(of, _)| *of == tag).expect("a tag");
      for (offset, field) in fields {
        mark(field, image, at + offset);
      }
    }
    Desc::Pointer { big, aim: what, target, .. } => {
      image.write(at, 4);
      let Some(to) = aim(image, at, *big).expect("a pointer") else { return };
      let target = target();
      match what {
        Aim::One => mark(&target, image, to),
        Aim::Seq => {
          image.write(to - 4, 4);
          let len = length(image, to, *big).expect("a length");
          for i in 0..len {
            mark(&target, image, to + i * target.size());
          }
        }
        Aim::Text => {
          image.write(to - 4, 4);
          let len = length(image, to, *big).expect("a length");
          image.write(to, len);
        }
      }
    }
  }
}

/// A type of the model: its description, values of it, and a way to read all of one.
pub trait Model: Pun + Fix + 'static {
  fn desc() -> Desc;

  /// Values of the type, each in a region: every shape up to a small depth, with every value of
  /// a small alphabet.
  fn corpus() -> Vec<Region<Self>>;

  /// All of the value, its pointers followed through `region`, which holds it: what Miri sees
  /// read.
  fn show<R>(&self, region: &Region<R>) -> String;
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
  let bytes = region.bytes().to_vec();
  let mut image = Image { init: vec![false; bytes.len()], bytes };
  mark(&T::desc(), &mut image, region.root());
  if let Err(why) = valid(&T::desc(), &image, region.root()) {
    panic!("the oracle refuses a {} of the corpus: {why}", name::<T>());
  }
  image
}

/// What the oracle alone says of reading an `S` as a `D`: whatever every value of the corpus of
/// `S` is a valid `D`.
#[must_use]
pub fn sound<S: Model, D: Model>() -> bool {
  let target = D::desc();
  S::corpus().iter().all(|region| valid(&target, &image(region), region.root()).is_ok())
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
    let verdict = valid(&target, &image(&region), region.root());
    (all, some) = (all && verdict.is_ok(), some || verdict.is_ok());
    let value = || region.show(&region);
    if let Some(view) = hook::view_ref::<S, D, FUEL>(&*region) {
      if let Err(why) = &verdict {
        panic!("UNSOUND: {pair}: `view_ref` reads {} as a target, but {why}", value());
      }
      // All of it, pointers followed: what Miri is there to see.
      let _ = view.show(&region);
    }
    match (hook::try_view_ref::<S, D, FUEL>(&*region), &verdict) {
      (Some(Ok(_)), Err(why)) => {
        panic!("UNSOUND: {pair}: `try_view_ref` accepts {}, but {why}", value())
      }
      (Some(Err(error)), Ok(())) => {
        panic!("INCORRECT: {pair}: `try_view_ref` refuses {}, a valid target: {error}", value())
      }
      (Some(Ok(view)), Ok(())) => {
        let _ = view.show(&region);
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

// -------------------------------------------------------------------------------------------------
// `puns::validate_in`, against the oracle

/// Why [`Follow`] stops.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
  /// Bytes that are not a value, at a position.
  Invalid(usize),
  /// A pointer of the bytes that does not lead to a value: where it is.
  Pointer(usize),
  /// More pointers followed than the fuel allows.
  NoFuel,
}

impl From<puns::Invalid> for Stop {
  fn from(invalid: puns::Invalid) -> Self {
    Self::Invalid(invalid.at)
  }
}

/// A visitor that follows the pointers of the tests (`relative`) through `bytes`.
///
/// It validates what they aim at with `puns` in turn: what a crate that has pointers does with
/// `validate_in`, written from the layouts of the pointers only (`as_pointer`, `Pointee`).
pub struct Follow<'b> {
  pub bytes: &'b [u8],
  pub fuel: usize,
}

impl Follow<'_> {
  /// The integer of `width` bytes at `at`, signed, in a byte order.
  fn int(&self, at: usize, width: usize, endian: puns::layout::Endian) -> Result<i64, Stop> {
    let bytes = self.bytes.get(at..).and_then(|rest| rest.get(..width)).ok_or(Stop::Pointer(at))?;
    let mut wide = [0u8; 8];
    for (i, byte) in bytes.iter().enumerate() {
      wide[if matches!(endian, puns::layout::Endian::Little) { i } else { width - 1 - i }] = *byte;
    }
    let shift = 64 - 8 * u32::try_from(width).unwrap_or(8);
    Ok(i64::from_le_bytes(wide) << shift >> shift)
  }

  /// The length of `width` bytes before `to`, not negative.
  fn length(&self, to: usize, width: usize, endian: puns::layout::Endian) -> Result<usize, Stop> {
    let at = to.checked_sub(width).ok_or(Stop::Pointer(to))?;
    usize::try_from(self.int(at, width, endian)?).map_err(|_| Stop::Pointer(at))
  }
}

/// What a pointer of the tests aims at, found by [`Follow::target`].
enum Target {
  /// One value at a position.
  One(usize),
  /// A sequence: its first element, its length.
  Sequence(usize, usize),
  /// Text, valid UTF-8.
  Text,
}

impl Follow<'_> {
  /// What the pointer `P` at `at` aims at, from its layout only (`as_pointer`), and checked as its
  /// scheme says: in the bytes, aligned, a length that is one; a text of UTF-8.
  fn target<P: ?Sized + Pun>(&mut self, at: usize) -> Result<Target, Stop> {
    let pointer = const { P::LAYOUT.as_pointer() }.expect("a pointer is handed over");
    assert_eq!(pointer.scheme(), crate::relative::SCHEME, "a pointer of the tests");
    let (width, endian) = (pointer.width(), pointer.endian());
    let offset = self.int(at, width, endian)?;
    // An offset of zero, which the type of the pointer allows (it is no `NonZero`): the oracle
    // reads it as null, so does this (the tests make one only for a target that zeros are).
    if offset == 0 {
      return Err(Stop::Pointer(at));
    }
    self.fuel = self.fuel.checked_sub(1).ok_or(Stop::NoFuel)?;
    let to = i64::try_from(at).ok().and_then(|at| at.checked_add(offset));
    let to = to.and_then(|to| usize::try_from(to).ok()).filter(|to| *to <= self.bytes.len());
    let to = to.ok_or(Stop::Pointer(at))?;
    match pointer.aim() {
      puns::layout::Aim::One => {
        if !to.is_multiple_of(pointer.align()) {
          return Err(Stop::Pointer(at));
        }
        Ok(Target::One(to))
      }
      puns::layout::Aim::Sequence { align } => {
        if !to.is_multiple_of(align) {
          return Err(Stop::Pointer(at));
        }
        Ok(Target::Sequence(to, self.length(to, width, endian)?))
      }
      puns::layout::Aim::Text => {
        let len = self.length(to, width, endian)?;
        let text =
          self.bytes.get(to..).and_then(|rest| rest.get(..len)).ok_or(Stop::Pointer(at))?;
        std::str::from_utf8(text).map(|_| Target::Text).map_err(|_| Stop::Pointer(at))
      }
    }
  }
}

impl puns::Visit for Follow<'_> {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    match self.target::<P>(at)? {
      Target::One(to) => puns::validate_in::<puns::Pointee<P>, Self>(self.bytes, to, self),
      Target::Sequence(to, len) => {
        puns::validate_elems_in::<puns::Pointee<P>, Self>(self.bytes, to, len, self)
      }
      Target::Text => Ok(()),
    }
  }
}

/// A visitor that follows the pointers of the tests as [`Follow`] does, and records each pointer
/// handed over (its position, its type), with `validate_in` (`trusted` false) or with
/// `pointers_in` (`trusted` true, for bytes that `validate_in` accepted, every pointer followed).
struct Trace<'b> {
  follow: Follow<'b>,
  trusted: bool,
  seen: Vec<(usize, &'static str)>,
}

impl puns::Visit for Trace<'_> {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    self.seen.push((at, core::any::type_name::<P>()));
    let target = self.follow.target::<P>(at)?;
    let (bytes, trusted) = (self.follow.bytes, self.trusted);
    match (target, trusted) {
      (Target::One(to), false) => puns::validate_in::<puns::Pointee<P>, Self>(bytes, to, self),
      (Target::Sequence(to, len), false) => {
        puns::validate_elems_in::<puns::Pointee<P>, Self>(bytes, to, len, self)
      }
      // SAFETY: the bytes that `validate_in` accepted, every pointer followed (`trace`): what
      // the pointer aims at is a valid value, in the bytes.
      (Target::One(to), true) => unsafe {
        puns::pointers_in::<puns::Pointee<P>, Self>(bytes.as_ptr(), to, self)
      },
      // SAFETY: as above, `len` valid values.
      (Target::Sequence(to, len), true) => unsafe {
        puns::pointers_elems_in::<puns::Pointee<P>, Self>(bytes.as_ptr(), to, len, self)
      },
      (Target::Text, _) => Ok(()),
    }
  }
}

/// How many pointers the oracle and [`Follow`] follow in a corrupted image: more than any value
/// of a corpus holds; past it, the pointers of the image go round in a circle.
const VALIDATE_FUEL: usize = 64;

/// The bytes of `image`, those that no value was written to (padding, gaps) as `fill`, and 16
/// more of it: a reference to a value covers all of its bytes, which `validate_in` wants in
/// bounds, where the oracle reads only those of the variant that it is (a value of a corrupted
/// image may start near the end).
fn initialized(image: &Image, fill: u8) -> Vec<u8> {
  (0..image.bytes.len() + 16)
    .map(|at| {
      if image.init.get(at) == Some(&true) { image.read(at, 1).expect("written")[0] } else { fill }
    })
    .collect()
}

/// The image of `bytes`, every one of them initialized.
fn whole(bytes: &[u8]) -> Image {
  Image {
    bytes: bytes.iter().map(|byte| MaybeUninit::new(*byte)).collect(),
    init: vec![true; bytes.len()],
  }
}

/// Whatever `puns::validate_in` with [`Follow`], and the oracle, agree on `bytes` (`image`, the
/// same bytes) as a `T` at `at`: `false` if one of them ran out of fuel (no answer).
///
/// # Panics
/// If they do not agree.
fn agree<T: Model>(bytes: &[u8], image: &Image, at: usize, what: &str) -> bool {
  let oracle = valid_with(&T::desc(), image, at, &mut { VALIDATE_FUEL });
  let mut follow = Follow { bytes, fuel: VALIDATE_FUEL };
  let ours = puns::validate_in::<T, Follow<'_>>(bytes, at, &mut follow);
  match (&oracle, &ours) {
    (Err(why), _) if why == NO_FUEL => false,
    (_, Err(Stop::NoFuel)) => false,
    (Ok(()), Ok(())) | (Err(_), Err(_)) => true,
    _ => panic!("{}: {what}: the oracle says {oracle:?}, `validate_in` {ours:?}", name::<T>()),
  }
}

/// `puns::validate_in`, with a visitor that follows the pointers, accepts what the oracle accepts.
///
/// On every value of the corpus of `T`, and on every copy of one with one byte changed by one of
/// `masks` (`xor`), padding and gaps included, and with the bytes cut short. Returns how many
/// images were compared.
///
/// # Panics
/// If they do not agree on one of them.
#[must_use]
pub fn check_validate<T: Model>(masks: &[u8]) -> usize {
  let mut compared = 0;
  for region in T::corpus() {
    let image = image(&region);
    for fill in [0, 0xa5] {
      let bytes = initialized(&image, fill);
      assert!(
        agree::<T>(&bytes, &whole(&bytes), region.root(), "the corpus"),
        "fuel for the corpus"
      );
      compared += 1;
      for at in 0..bytes.len() {
        for &mask in masks {
          let mut bad = bytes.clone();
          bad[at] ^= mask;
          let what = format!("{mask:#x} at {at}");
          compared += usize::from(agree::<T>(&bad, &whole(&bad), region.root(), &what));
        }
      }
      // Cut right after the root, which is all there: the same. Cut before its end: `validate_in`
      // wants every byte of a value (a reference to it covers them), where the oracle reads only
      // those of the variant that it is; and so does it of what pointers aim at, before the root.
      let end = region.root() + size_of::<T>();
      compared +=
        usize::from(agree::<T>(&bytes[..end], &whole(&bytes[..end]), region.root(), "cut"));
      for len in [region.root(), end - 1] {
        let mut follow = Follow { bytes: &bytes[..len], fuel: VALIDATE_FUEL };
        let result = puns::validate_in::<T, Follow<'_>>(&bytes[..len], region.root(), &mut follow);
        assert_eq!(result, Err(Stop::Invalid(region.root())), "{}: cut at {len}", name::<T>());
      }
    }
  }
  compared
}

/// The pointers of the `T` at `at` of `bytes` and of everything they aim at, in the order handed
/// over, by `validate_in` and, if it accepts the bytes, by `pointers_in`: `None` if it does not
/// (or runs out of fuel).
///
/// # Panics
/// If `pointers_in` does not hand over the same pointers, in the same order, or fails.
fn trace<T: Model>(bytes: &[u8], at: usize, what: &str) -> Option<()> {
  let follow = |bytes| Follow { bytes, fuel: VALIDATE_FUEL };
  let mut checked = Trace { follow: follow(bytes), trusted: false, seen: vec![] };
  puns::validate_in::<T, Trace<'_>>(bytes, at, &mut checked).ok()?;
  let mut trusted = Trace { follow: follow(bytes), trusted: true, seen: vec![] };
  // SAFETY: `validate_in` accepted the bytes as a `T` at `at`, and every pointer that it handed
  // over was followed, and what it aims at accepted, in turn.
  let walked = unsafe { puns::pointers_in::<T, Trace<'_>>(bytes.as_ptr(), at, &mut trusted) };
  assert_eq!(walked, Ok(()), "{}: {what}: `pointers_in` fails", name::<T>());
  assert_eq!(trusted.seen, checked.seen, "{}: {what}: other pointers", name::<T>());
  Some(())
}

/// `puns::pointers_in` hands over the pointers that `validate_in` hands over, in the same order.
///
/// On every image that `validate_in` accepts, following them through the buffer: the values of
/// the corpus of `T`, and every copy of one with one byte changed by one of `masks` that is still
/// valid (padding and gaps included). Returns how many images were compared.
///
/// # Panics
/// If they do not agree on one of them.
#[must_use]
pub fn check_pointers_in<T: Model>(masks: &[u8]) -> usize {
  let mut compared = 0;
  for region in T::corpus() {
    let image = image(&region);
    for fill in [0, 0xa5] {
      let bytes = initialized(&image, fill);
      assert!(trace::<T>(&bytes, region.root(), "the corpus").is_some(), "the corpus is valid");
      compared += 1;
      for at in 0..bytes.len() {
        for &mask in masks {
          let mut changed = bytes.clone();
          changed[at] ^= mask;
          let what = format!("{mask:#x} at {at}");
          compared += usize::from(trace::<T>(&changed, region.root(), &what).is_some());
        }
      }
    }
  }
  compared
}

/// The types of the model.
pub mod types {
  use core::mem::offset_of;

  use puns::{
    Pun,
    num::{i32be, u16le},
  };

  use super::{Aim, Desc, Model};
  use crate::relative::{Builder, Fix, One, Opt, Region, Seq, Text, addr, field};

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

  /// The region of `root`, whose pointers `build` made with `b`.
  fn region<T: Fix>(build: impl FnOnce(&mut Builder) -> T) -> Region<T> {
    let mut b = Builder::new();
    let root = build(&mut b);
    // SAFETY: every pointer of `root`, and of what it points to, made by `b` (each type of the
    // model fixes all of its pointers).
    unsafe { Region::new(b, root) }
  }

  /// What a pointer to one value aims at, shown.
  fn one<R, T: Model>(r: &Region<R>, pointer: &One<T>) -> String {
    r.get(pointer).show(r)
  }

  /// What a pointer that may be null aims at, shown.
  fn opt<R, T: Model>(r: &Region<R>, pointer: &Opt<T>) -> String {
    r.opt(pointer).map_or_else(|| "-".to_owned(), |value| value.show(r))
  }

  /// The sequence that a pointer aims at, shown.
  fn seq<R, T: Model>(r: &Region<R>, pointer: &Seq<T>) -> String {
    let values: Vec<String> = r.seq(pointer).iter().map(|value| value.show(r)).collect();
    format!("[{}]", values.join(", "))
  }

  /// What a pointer of the other byte order aims at, shown.
  fn big_endian<R>(r: &Region<R>, pointer: &One<u8, i32be>) -> String {
    r.get(pointer).to_string()
  }

  /// The text that a pointer aims at.
  fn text<R>(r: &Region<R>, pointer: &Text) -> String {
    r.text(pointer).to_owned()
  }

  macro_rules! plain {
    ($($ty:ty: $desc:expr;)+) => {$(
      impl Model for $ty {
        fn desc() -> Desc {
          $desc
        }

        fn corpus() -> Vec<Region<Self>> {
          unreachable!("a target only")
        }

        fn show<R>(&self, _: &Region<R>) -> String {
          format!("{self:?}")
        }
      }
    )+};
  }

  plain! {
    bool: flag();
    u8: bytes(1);
    u16le: bytes(2);
    [u8; 2]: bytes(2);
  }

  /// A chain of values, each holding the next: the first of `values` at the root.
  fn chain<T: Fix, V: Copy>(values: &[V], make: impl Fn(V, Opt<T>) -> T) -> Region<T> {
    region(|b| {
      let mut next = Opt::none();
      for value in values[1..].iter().rev() {
        next = b.opt(Some(make(*value, next)));
      }
      make(values[0], next)
    })
  }

  /// Chains of one value, of two, and a longer one.
  fn chains<T: Fix, V: Copy>(alphabet: &[V], make: impl Fn(V, Opt<T>) -> T) -> Vec<Region<T>> {
    let mut corpus = vec![];
    for first in alphabet {
      corpus.push(chain(&[*first], &make));
      for second in alphabet {
        corpus.push(chain(&[*first, *second], &make));
      }
    }
    let last = alphabet[alphabet.len() - 1];
    corpus.push(chain(&[alphabet[0], last, last], &make));
    corpus
  }

  /// A recursive type: a `bool`, and maybe one more of the same.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Flags {
    pub flag: bool,
    pub next: Opt<Flags>,
  }

  /// The same with a `u8`.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Bytes {
    pub flag: u8,
    pub next: Opt<Bytes>,
  }

  macro_rules! chained {
    ($($ty:ident: $flag:expr, $alphabet:expr;)+) => {$(
      // SAFETY: its pointer, at its position.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.next);
        }
      }

      impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![
            (offset_of!(Self, flag), $flag),
            (offset_of!(Self, next), pointer(true, Aim::One, Self::desc)),
          ])
        }

        fn corpus() -> Vec<Region<Self>> {
          chains(&$alphabet, |flag, next| $ty { flag, next })
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({}, {})", stringify!($ty), self.flag, opt(r, &self.next))
        }
      }
    )+};
  }

  chained! {
    Flags: flag(), [false, true];
    Bytes: bytes(1), [0, 1, 2, 255];
  }

  /// A tree of values: a value and its children.
  struct Node<V>(V, Vec<Node<V>>);

  /// Puts the children of `node`, then returns it.
  fn tree<T: Fix, V: Copy>(b: &mut Builder, node: &Node<V>, make: &impl Fn(V, Seq<T>) -> T) -> T {
    let children: Vec<T> = node.1.iter().map(|child| tree(b, child, make)).collect();
    let rest = b.seq(children);
    make(node.0, rest)
  }

  /// Lists of values of the same type, one level or two.
  fn lists<T: Fix, V: Copy>(alphabet: &[V], make: impl Fn(V, Seq<T>) -> T) -> Vec<Region<T>> {
    let leaf = |value| Node(value, vec![]);
    let mut corpus = vec![];
    for &first in alphabet {
      let mut shapes = vec![leaf(first)];
      for &second in alphabet {
        shapes.push(Node(first, vec![leaf(second)]));
        shapes.push(Node(first, vec![leaf(second), leaf(first)]));
        let deep = Node(second, vec![leaf(first), Node(first, vec![leaf(second)])]);
        shapes.push(Node(first, vec![leaf(second), deep]));
      }
      corpus.extend(shapes.iter().map(|shape| region(|b| tree(b, shape, &make))));
    }
    corpus
  }

  /// A recursive type through a sequence: a `bool`, and more of the same.
  #[derive(Pun)]
  #[repr(C)]
  pub struct FlagList {
    pub flag: bool,
    pub rest: Seq<FlagList>,
  }

  /// The same with a `u8`.
  #[derive(Pun)]
  #[repr(C)]
  pub struct ByteList {
    pub flag: u8,
    pub rest: Seq<ByteList>,
  }

  macro_rules! listed {
    ($($ty:ident: $flag:expr, $alphabet:expr;)+) => {$(
      // SAFETY: its pointer, at its position.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.rest);
        }
      }

      impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![
            (offset_of!(Self, flag), $flag),
            (offset_of!(Self, rest), pointer(false, Aim::Seq, Self::desc)),
          ])
        }

        fn corpus() -> Vec<Region<Self>> {
          lists(&$alphabet, |flag, rest| $ty { flag, rest })
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({}, {})", stringify!($ty), self.flag, seq(r, &self.rest))
        }
      }
    )+};
  }

  listed! {
    FlagList: flag(), [false, true];
    ByteList: bytes(1), [0, 1, 2, 255];
  }

  /// Structs that are one pointer: the type of the pointer, its description, how a value is
  /// made with what it aims at, how it is shown, and the values that it aims at.
  macro_rules! holders {
    ($($(#[$doc:meta])* $ty:ident($pointer:ty): $desc:expr; $build:expr; $show:expr; $values:expr;)+) => {$(
      $(#[$doc])*
      #[derive(Pun)]
      #[repr(C)]
      pub struct $ty {
        pub to: $pointer,
      }

      // SAFETY: its pointer, at its position.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.to);
        }
      }

      impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![(offset_of!(Self, to), $desc)])
        }

        fn corpus() -> Vec<Region<Self>> {
          $values.into_iter().map(|value| region(|b| $ty { to: $build(b, value) })).collect()
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({})", stringify!($ty), $show(r, &self.to))
        }
      }
    )+};
  }

  holders! {
    /// A pointer to a `bool`.
    ToFlag(One<bool>): pointer(false, Aim::One, flag);
      |b: &mut Builder, value| b.one(value); one; [false, true];
    /// A pointer to a `u8`.
    ToByte(One<u8>): pointer(false, Aim::One, || bytes(1));
      |b: &mut Builder, value| b.one(value); one; [0u8, 1, 2, 255];
    /// A pointer to a `u8`, or none.
    MaybeByte(Opt<u8>): pointer(true, Aim::One, || bytes(1));
      |b: &mut Builder, value| b.opt(value); opt; [None, Some(0u8), Some(1), Some(2), Some(255)];
    /// A pointer to a `u8`, whose offset is big-endian.
    ToByteBe(One<u8, i32be>): Desc::Pointer { big: true, nullable: false, aim: Aim::One, target: || bytes(1) };
      |b: &mut Builder, value| b.one(value); big_endian; [0u8, 1, 2, 255];
    /// A pointer to two bytes that are a number.
    ToWord(One<u16le>): pointer(false, Aim::One, || bytes(2));
      |b: &mut Builder, value| b.one(u16le::new(value)); one;
      [0u16, 1, 0x0100, 0x0201, 0xffff];
    /// A pointer to two bytes.
    ToPair(One<[u8; 2]>): pointer(false, Aim::One, || bytes(2));
      |b: &mut Builder, value| b.one(value); one; [[0u8, 0], [1, 0], [0, 1], [2, 1], [255, 255]];
    /// A pointer to a sequence of bytes.
    ToBytes(Seq<u8>): pointer(false, Aim::Seq, || bytes(1));
      |b: &mut Builder, value: &[u8]| b.seq(value.iter().copied()); seq;
      [&[][..], &[0][..], &[1, 2][..], &[255, 0, 1][..], &[97][..]];
    /// A pointer to a sequence of `bool`s.
    ToFlags(Seq<bool>): pointer(false, Aim::Seq, flag);
      |b: &mut Builder, value: &[bool]| b.seq(value.iter().copied()); seq;
      [&[][..], &[true][..], &[false, true][..]];
    /// A pointer to text.
    ToText(Text): pointer(false, Aim::Text, || bytes(1));
      |b: &mut Builder, value| b.text(value); text; ["", "a", "\u{e9}", "ab"];
  }

  /// Four bytes: what a pointer is made of, and is not.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Raw {
    pub raw: [u8; 4],
  }

  // SAFETY: no pointer.
  unsafe impl Fix for Raw {
    fn fix(&mut self, _: usize) {}
  }

  impl Model for Raw {
    fn desc() -> Desc {
      structure::<Self>(vec![(offset_of!(Self, raw), bytes(4))])
    }

    fn corpus() -> Vec<Region<Self>> {
      [[0; 4], [1, 0, 0, 0], [4, 0, 0, 0], [0, 0, 0, 4], [0xff; 4], [8, 0, 0, 0]]
        .into_iter()
        .map(|raw| region(|_| Raw { raw }))
        .collect()
    }

    fn show<R>(&self, _: &Region<R>) -> String {
      format!("Raw({:?})", self.raw)
    }
  }

  /// An enum with fields: a `bool`, or a pointer to one more of the same.
  #[derive(Pun)]
  #[repr(u8)]
  pub enum Tree {
    Leaf(bool),
    Node(One<Tree>),
  }

  // SAFETY: the pointer of the variant, at its position.
  unsafe impl Fix for Tree {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      if let Self::Node(node) = self {
        field(whole, at, node);
      }
    }
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
        corpus.push(region(|_| Tree::Leaf(value)));
        corpus.push(region(|b| Tree::Node(b.one(Tree::Leaf(value)))));
        corpus.push(region(|b| {
          let leaf = b.one(Tree::Leaf(value));
          Tree::Node(b.one(Tree::Node(leaf)))
        }));
      }
      corpus
    }

    fn show<R>(&self, r: &Region<R>) -> String {
      match self {
        Self::Leaf(value) => format!("Leaf({value})"),
        Self::Node(node) => format!("Node({})", one(r, node)),
      }
    }
  }

  /// The same with a `u8`, and one more variant: a sequence.
  #[derive(Pun)]
  #[repr(u8)]
  pub enum Wood {
    Leaf(u8),
    Node(One<Wood>),
    Pair(Seq<Wood>),
  }

  // SAFETY: the pointer of the variant, at its position.
  unsafe impl Fix for Wood {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      match self {
        Self::Leaf(_) => {}
        Self::Node(node) => field(whole, at, node),
        Self::Pair(pair) => field(whole, at, pair),
      }
    }
  }

  impl Model for Wood {
    fn desc() -> Desc {
      Desc::Enum {
        size: size_of::<Self>(),
        align: align_of::<Self>(),
        variants: vec![
          (0, vec![(1, bytes(1))]),
          (1, vec![(1, pointer(false, Aim::One, Self::desc))]),
          (2, vec![(1, pointer(false, Aim::Seq, Self::desc))]),
        ],
      }
    }

    fn corpus() -> Vec<Region<Self>> {
      let mut corpus = vec![];
      for value in [0, 1, 2, 255] {
        corpus.push(region(|_| Wood::Leaf(value)));
        corpus.push(region(|b| Wood::Node(b.one(Wood::Leaf(value)))));
        corpus.push(region(|b| {
          let leaf = b.one(Wood::Leaf(value));
          Wood::Node(b.one(Wood::Node(leaf)))
        }));
        corpus.push(region(|b| Wood::Pair(b.seq([Wood::Leaf(value), Wood::Leaf(1)]))));
        corpus.push(region(|b| {
          let (leaf, empty) = (b.one(Wood::Leaf(value)), b.seq([]));
          Wood::Pair(b.seq([Wood::Node(leaf), Wood::Pair(empty)]))
        }));
      }
      corpus
    }

    fn show<R>(&self, r: &Region<R>) -> String {
      match self {
        Self::Leaf(value) => format!("Leaf({value})"),
        Self::Node(node) => format!("Node({})", one(r, node)),
        Self::Pair(pair) => format!("Pair({})", seq(r, pair)),
      }
    }
  }
}
