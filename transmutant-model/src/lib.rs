//! Model check of the analysis of `transmutant` against an oracle that does not use it.
//!
//! For each type, [`Oracle::check`] tells whatever some bytes (`None`: uninitialized) are a valid
//! value, written by hand from what the language and the documentation of the type say, with the
//! offsets that the compiler gives (`offset_of!`, addresses of the fields of a value). The valid
//! values of a type are found by enumeration ([`values`]), and every verdict of the library on a
//! pair of types is compared with what they imply ([`check_pair`]).
//!
//! What a value of a struct is known to be does not include what `#[data(valid(..))]` or a rule
//! requires: safe code can write anything to a plain field. The oracle has two modes for that.

pub mod types;

use std::{collections::BTreeMap, fmt::Write as _, mem::MaybeUninit, sync::Mutex};

use transmutant::{
  Data,
  hook::{self, Verdict},
};

/// Bytes of a value, `None` where they are uninitialized.
pub type Bytes = [Option<u8>];

/// Whatever the value was decided without reading an uninitialized byte.
pub type Valid = Result<bool, Uninit>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uninit;

/// What the bytes of a type must be.
pub trait Oracle: Data + Copy + 'static {
  /// Whatever `b[off..off + size_of::<Self>()]` is a valid value. With `target`, requirements
  /// (`valid(..)`, rules) count; without, only what the type system guarantees. Every byte that is
  /// looked at is set in `reads`.
  ///
  /// # Errors
  /// [`Uninit`] if what decides is an uninitialized byte.
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid;
}

/// Read the integer of `size` bytes at `off`.
///
/// # Errors
/// [`Uninit`] if one of them is uninitialized.
pub fn int(
  b: &Bytes,
  off: usize,
  size: usize,
  little: bool,
  reads: &mut [bool],
) -> Result<u128, Uninit> {
  let mut value = 0u128;
  let mut uninit = false;
  for i in 0..size {
    reads[off + i] = true;
    let byte = if little { b[off + size - 1 - i] } else { b[off + i] };
    match byte {
      Some(byte) => value = (value << 8) | u128::from(byte),
      None => uninit = true,
    }
  }
  if uninit { Err(Uninit) } else { Ok(value) }
}

pub const NATIVE_LITTLE: bool = cfg!(target_endian = "little");

/// All the fields of a struct: valid if each is, an uninitialized byte under any is an error.
///
/// # Errors
/// [`Uninit`] if one of `results` is.
pub fn all(results: &[Valid]) -> Valid {
  if results.iter().any(std::result::Result::is_err) {
    return Err(Uninit);
  }
  Ok(results.iter().all(|r| *r == Ok(true)))
}

macro_rules! scalar_oracle {
  ($($ty:ty => |$v:ident| $pred:expr, $little:expr;)+) => {$(
    impl Oracle for $ty {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
        let $v = int(b, off, size_of::<$ty>(), $little, reads)?;
        Ok($pred)
      }
    }
  )+};
}

use core::num::NonZero;

use transmutant::num::{
  u8c, u8in, u16be, u16bec, u16le, u16lec, u24be, u24bec, u24le, u24lec, u32be, u32bec, u32le,
  u32lec,
};

scalar_oracle! {
  u8 => |_v| true, true;
  i8 => |_v| true, true;
  u16 => |_v| true, true;
  i16 => |_v| true, true;
  u32 => |_v| true, true;
  u64 => |_v| true, true;
  i32 => |_v| true, true;
  f32 => |_v| true, true;
  bool => |v| v <= 1, true;
  char => |v| v <= 0xd7ff || (0xe000..=0x10_ffff).contains(&v), NATIVE_LITTLE;
  NonZero<u8> => |v| v != 0, true;
  NonZero<u16> => |v| v != 0, true;
  NonZero<u32> => |v| v != 0, true;
  Option<NonZero<u8>> => |_v| true, true;
  Option<NonZero<u16>> => |_v| true, true;
  Option<NonZero<u32>> => |_v| true, true;
  u16le => |_v| true, true;
  u16be => |_v| true, true;
  u24le => |_v| true, true;
  u24be => |_v| true, true;
  u32le => |_v| true, true;
  u32be => |_v| true, true;
}

impl<const V: u8> Oracle for u8c<V> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    Ok(int(b, off, 1, true, reads)? == u128::from(V))
  }
}

impl<const MIN: u8, const MAX: u8> Oracle for u8in<MIN, MAX> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(u128::from(MIN) <= v && v <= u128::from(MAX))
  }
}

macro_rules! literal_oracle {
  ($($ty:ident: $raw:ty, $size:literal, $little:literal;)+) => {$(
    impl<const V: $raw> Oracle for $ty<V> {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
        Ok(int(b, off, $size, $little, reads)? == u128::from(V))
      }
    }
  )+};
}

literal_oracle! {
  u16lec: u16, 2, true;
  u16bec: u16, 2, false;
  u24lec: u32, 3, true;
  u24bec: u32, 3, false;
  u32lec: u32, 4, true;
  u32bec: u32, 4, false;
}

impl Oracle for () {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

impl<T: ?Sized + 'static> Oracle for core::marker::PhantomData<T> {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

impl<T: Oracle, const N: usize> Oracle for [T; N] {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let results: Vec<Valid> =
      (0..N).map(|i| T::check(b, off + i * size_of::<T>(), reads, target)).collect();
    all(&results)
  }
}

impl<T: Oracle> Oracle for core::mem::ManuallyDrop<T> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    T::check(b, off, reads, target)
  }
}

impl<A: Oracle> Oracle for (A,) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    A::check(b, off + core::mem::offset_of!(Self, 0), reads, target)
  }
}

impl<A: Oracle, B: Oracle> Oracle for (A, B) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle> Oracle for (A, B, C) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, target),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle, D: Oracle> Oracle for (A, B, C, D) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, target),
      D::check(b, off + core::mem::offset_of!(Self, 3), reads, target),
    ])
  }
}

/// The byte values that the enumeration of the values of a type of `size` bytes takes for each
/// byte: all of them up to 2 bytes, else the bounds of the ranges of the types of the model.
pub fn alphabet(size: usize) -> Vec<Option<u8>> {
  let bytes: Vec<u8> = match size {
    // Miri cannot afford them all.
    0..=1 if cfg!(miri) => vec![0, 1, 2, 3, 5, 7, 0x7f, 0x80, 0xfe, 0xff],
    2 if cfg!(miri) => vec![0, 1, 2, 0x12, 0x34, 0x7f, 0x80, 0xff],
    3.. if cfg!(miri) => vec![0, 1, 0x10, 0x34, 0xff],
    0..=2 => (0..=255).collect(),
    3 => vec![
      0, 1, 2, 3, 5, 7, 0x10, 0x11, 0x12, 0x34, 0x56, 0x7f, 0x80, 0xc2, 0xd7, 0xd8, 0xdf, 0xe0,
      0xfe, 0xff,
    ],
    4 => vec![0, 1, 2, 7, 0x10, 0x11, 0x34, 0x7f, 0x80, 0xd7, 0xd8, 0xe0, 0xfe, 0xff],
    _ => vec![0, 1, 2, 0xff],
  };
  let mut alphabet: Vec<Option<u8>> = bytes.into_iter().map(Some).collect();
  alphabet.push(None);
  alphabet
}

/// Every pattern of `size` bytes over the alphabet.
#[must_use]
pub fn patterns(size: usize) -> Vec<Vec<Option<u8>>> {
  let alphabet = alphabet(size);
  let mut out = vec![vec![]];
  for _ in 0..size {
    let mut next = Vec::with_capacity(out.len() * alphabet.len());
    for prefix in &out {
      for &byte in &alphabet {
        let mut pattern = prefix.clone();
        pattern.push(byte);
        next.push(pattern);
      }
    }
    out = next;
  }
  out
}

/// The values of `T` (as a source: what safe code may hold), over the alphabet: the patterns that
/// are valid and that are uninitialized wherever they are not looked at (padding).
#[must_use]
pub fn values<T: Oracle>() -> Vec<Vec<Option<u8>>> {
  let size = size_of::<T>();
  patterns(size)
    .into_iter()
    .filter(|p| {
      let mut reads = vec![false; size];
      T::check(p, 0, &mut reads, false) == Ok(true) && (0..size).all(|i| reads[i] || p[i].is_none())
    })
    .collect()
}

/// A `T` made of `bytes`, which the oracle says are a valid `T`.
#[must_use]
pub fn make<T: Oracle>(bytes: &Bytes) -> T {
  let mut value = MaybeUninit::<T>::uninit();
  let ptr = value.as_mut_ptr().cast::<u8>();
  for (i, byte) in bytes.iter().enumerate() {
    if let Some(byte) = byte {
      // SAFETY: in bounds.
      unsafe { ptr.add(i).write(*byte) };
    }
  }
  // SAFETY: the oracle says that these bytes are a `T`. If it is wrong, Miri says so.
  unsafe { value.assume_init() }
}

/// The bytes of a value, as the oracle sees them.
pub fn bytes_of<T: Oracle>(value: &T) -> Vec<Option<u8>> {
  let size = size_of::<T>();
  let mut reads = vec![false; size];
  // Which bytes are looked at depends on the value: find them with the bytes of the value,
  // reading only what the oracle reads (the others may be padding).
  let ptr = std::ptr::from_ref::<T>(value).cast::<u8>();
  let mut known: Vec<Option<u8>> = vec![None; size];
  loop {
    let before = reads.clone();
    let _ = T::check(&known, 0, &mut reads, false);
    for i in 0..size {
      if reads[i] {
        // SAFETY: a byte that the oracle reads is not padding.
        known[i] = Some(unsafe { ptr.add(i).read() });
      }
    }
    if reads == before {
      return known;
    }
  }
}

/// What the library did wrong, or did not do as well as it could.
#[derive(Default)]
pub struct Report {
  pub pairs: usize,
  pub values: usize,
  pub conversions: usize,
  /// Soundness: a verdict that lets an invalid value through.
  pub unsound: Vec<String>,
  /// A run-time check that rejects a valid value, or a verdict that contradicts the truth in a
  /// way that is not unsound.
  pub incorrect: Vec<String>,
  /// Valid conversions rejected, or demoted to run-time checks, by cause.
  pub imprecise: BTreeMap<String, Vec<String>>,
  /// How the library classified the pairs.
  pub classes: BTreeMap<String, usize>,
}

pub static REPORT: Mutex<Option<Report>> = Mutex::new(None);

fn record(f: impl FnOnce(&mut Report)) {
  let mut report = REPORT.lock().unwrap();
  f(report.get_or_insert_with(Report::default));
}

fn show(p: &Bytes) -> String {
  let mut s = String::from("[");
  for (i, b) in p.iter().enumerate() {
    if i > 0 {
      s.push(' ');
    }
    match b {
      Some(b) => write!(s, "{b:02x}").unwrap(),
      None => s.push_str("__"),
    }
  }
  s.push(']');
  s
}

fn name<T: ?Sized>() -> String {
  std::any::type_name::<T>()
    .replace("transmutant_model::", "")
    .replace("transmutant::num::", "")
    .replace("core::num::nonzero::", "")
    .replace("core::option::", "")
    .replace("core::mem::manually_drop::", "")
    .replace("core::marker::", "")
}

/// What a pattern is for a type: valid for what the type system guarantees (`hard`), and for what
/// it requires on top (`soft`: `valid(..)`, rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Truth {
  hard: Valid,
  soft: Valid,
}

fn truth<D: Oracle>(p: &Bytes) -> Truth {
  let p = &p[..size_of::<D>()];
  let mut reads = vec![false; p.len()];
  let hard = D::check(p, 0, &mut reads, false);
  let soft = D::check(p, 0, &mut reads, true);
  Truth { hard, soft }
}

/// Compare what the library says of `S -> D` with the truth. `limit` bounds the number of values
/// of the source that are converted at run time (all of them with `usize::MAX`).
///
/// A requirement of the target (`valid(..)`, a rule) that the library does not check is not
/// unsound, and it is by design when the source has the same requirement: it is then only
/// counted. Anything that lets a value through that the type system does not allow is unsound.
///
/// # Panics
/// If a conversion that the verdict says compiles does not, or if the report is poisoned.
pub fn check_pair<S: Oracle, D: Oracle>(limit: usize) {
  let v: Verdict = const { hook::verdict::<S, D>() };
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  let same = std::any::TypeId::of::<S>() == std::any::TypeId::of::<D>();
  let source = values::<S>();
  let truths: Vec<Truth> =
    if ds <= ss { source.iter().map(|p| truth::<D>(p)).collect() } else { vec![] };
  // What the source requires of itself, which proves the same requirement of the target.
  let claims: Vec<bool> = source.iter().map(|p| same || truth::<S>(p).soft == Ok(true)).collect();
  let any_uninit = truths.iter().any(|t| t.hard.is_err());
  // A requirement that reads uninitialized bytes is unsound to check, whatever the source says
  // of itself (only the identity proves it without checking it).
  let soft_uninit = !same && truths.iter().any(|t| t.soft.is_err());
  let all_hard = ds <= ss && truths.iter().all(|t| t.hard == Ok(true));
  let all_valid = ds <= ss && truths.iter().all(|t| t.soft == Ok(true));
  let none_valid = truths.iter().all(|t| t.soft != Ok(true));
  let class = match () {
    () if v.transmute_ref => "transmute_ref",
    () if v.transmute => "transmute (not by reference: alignment)",
    () if v.try_transmute_ref => "try_transmute_ref",
    () if v.try_transmute => "try_transmute (not by reference)",
    () if v.truncate || v.truncate_ref => "truncate only",
    () => "rejected",
  };
  let mut unsound = vec![];
  let mut incorrect = vec![];
  let mut imprecise: Vec<(String, String)> = vec![];
  let mut conversions = 0;
  let example = |pred: &dyn Fn(&Truth) -> bool| {
    source
      .iter()
      .zip(&truths)
      .find(|(_, t)| pred(t))
      .map_or_default(|(p, t)| format!("{} ({t:?})", show(p)))
  };

  // Sizes and alignment.
  if (v.transmute || v.try_transmute) && ss != ds {
    unsound.push(format!("{pair}: by value between sizes {ss} and {ds}"));
  }
  if v.truncate && ds > ss {
    unsound.push(format!("{pair}: truncate to a larger type"));
  }
  if (v.transmute_ref || v.transmute_mut || v.truncate_ref) && align_of::<D>() > align_of::<S>() {
    unsound.push(format!("{pair}: reference to a more aligned type"));
  }
  if ds <= ss {
    let infallible =
      if ss == ds { v.transmute || v.transmute_ref } else { v.truncate || v.truncate_ref };
    let fallible = ss == ds && (v.try_transmute || v.try_transmute_ref);
    // (a) infallible implies inclusion, without padding.
    if infallible && !all_hard {
      unsound.push(format!(
        "{pair}: compiles without a check, but {}",
        example(&|t| t.hard != Ok(true))
      ));
    } else if infallible && !all_valid {
      let unproven =
        source.iter().zip(&truths).zip(&claims).any(|((_, t), c)| t.soft != Ok(true) && !c);
      if unproven {
        incorrect.push(format!(
          "{pair}: a requirement is not checked: {}",
          example(&|t| t.soft != Ok(true))
        ));
      }
    }
    if ss == ds && !v.try_transmute && !v.try_transmute_ref {
      // (c) never implies emptiness; uninit is only for what reads padding.
      let why = v.why();
      if why.contains("no value of the source") && !none_valid {
        imprecise.push((
          "rejected as never valid, but some values are".into(),
          format!("{pair}: {}", example(&|t| t.soft == Ok(true))),
        ));
      } else if why.contains("is padding") && !any_uninit && !soft_uninit {
        imprecise.push(("rejected for padding that no valid target reads".into(), pair.clone()));
      } else if !why.contains("no value") && !why.contains("padding") {
        incorrect.push(format!("{pair}: try_transmute_ref rejected: {}", why.trim()));
      }
    }
    if fallible && any_uninit {
      unsound.push(format!("{pair}: try compiles, but {}", example(&|t| t.hard.is_err())));
    }
    if fallible && soft_uninit {
      unsound.push(format!(
        "{pair}: try compiles, and a requirement reads padding: {}",
        example(&|t| t.soft.is_err())
      ));
    }
    if fallible && !infallible && all_valid && !source.is_empty() {
      imprecise.push(("demoted to a run-time check".into(), pair.clone()));
    }
    // (b) the run-time check accepts exactly the valid values.
    if v.try_transmute && !any_uninit && !soft_uninit {
      for ((p, t), claim) in source.iter().zip(&truths).zip(&claims).take(limit) {
        let value = make::<S>(p);
        let result = hook::try_transmute::<S, D>(value).expect("try_transmute compiles");
        // By reference, an address that is not aligned for the target is an error of its own
        // (Miri puts values anywhere their alignment allows, natively they are often more aligned).
        let result_ref = if v.try_transmute_ref {
          let result = hook::try_transmute_ref::<S, D>(&value).expect("try_transmute_ref compiles");
          let misaligned = !(&raw const value).addr().is_multiple_of(align_of::<D>());
          if misaligned && result.err() != Some(transmutant::Error::Alignment) {
            unsound.push(format!(
              "{pair}: try_transmute_ref of a misaligned {} is not an alignment error",
              show(p)
            ));
          }
          (!misaligned).then(|| result.is_ok())
        } else {
          None
        };
        conversions += 1;
        let accepted = result.is_ok();
        if let Ok(d) = result {
          let got = bytes_of(&d);
          let want: Vec<Option<u8>> =
            (0..ds).map(|i| if got[i].is_some() { p[i] } else { None }).collect();
          if got != want {
            unsound.push(format!("{pair}: {} became {}", show(p), show(&got)));
          }
        }
        for (what, accepted) in
          [("try_transmute", Some(accepted)), ("try_transmute_ref", result_ref)]
        {
          let Some(accepted) = accepted else { continue };
          match (accepted, t.hard, t.soft) {
            (true, Ok(true), Ok(true)) | (false, _, Ok(false) | Err(_)) | (false, Ok(false), _) => {
            }
            (true, Ok(true), _) if *claim => {}
            (true, Ok(true), _) => incorrect.push(format!(
              "{pair}: {what} accepts {} ({t:?}), a requirement is not checked",
              show(p)
            )),
            (true, _, _) => unsound.push(format!("{pair}: {what} accepts {} ({t:?})", show(p))),
            (false, _, _) => {
              incorrect.push(format!("{pair}: {what} rejects the valid {} ({t:?})", show(p)));
            }
          }
        }
        if unsound.len() + incorrect.len() > 3 {
          break;
        }
      }
    }
    // (d) a `&mut` needs the inclusion both ways.
    if ss == ds && (v.transmute_mut || v.try_transmute_mut) {
      for q in values::<D>() {
        let mut reads = vec![false; ss];
        let back = S::check(&q, 0, &mut reads, false);
        if back != Ok(true) {
          unsound
            .push(format!("{pair}: &mut, and the target {} is not a source ({back:?})", show(&q)));
          break;
        }
      }
    }
    // Truncation: the first bytes of the source, unchanged.
    if ds < ss && v.truncate {
      for p in source.iter().take(limit) {
        let d = hook::truncate::<S, D>(make::<S>(p)).expect("truncate compiles");
        let got = bytes_of(&d);
        let want: Vec<Option<u8>> =
          (0..ds).map(|i| if got[i].is_some() { p[i] } else { None }).collect();
        conversions += 1;
        if got != want {
          unsound.push(format!("{pair}: {} truncated to {}", show(p), show(&got)));
          break;
        }
      }
    }
  }
  record(|report| {
    report.pairs += 1;
    report.values += source.len();
    report.conversions += conversions;
    *report.classes.entry(class.into()).or_default() += 1;
    report.unsound.extend(unsound);
    report.incorrect.extend(incorrect);
    for (cause, pair) in imprecise {
      report.imprecise.entry(cause).or_default().push(pair);
    }
  });
}

/// Print the report, and fail if anything is unsound or incorrect.
///
/// # Panics
/// If anything is unsound or incorrect.
pub fn finish(title: &str) {
  let report = REPORT.lock().unwrap().take().unwrap_or_default();
  println!(
    "== {title}: {} pairs, {} source values, {} conversions at run time",
    report.pairs, report.values, report.conversions
  );
  for (class, count) in &report.classes {
    println!("   {count:5} {class}");
  }
  for (cause, pairs) in &report.imprecise {
    println!("   imprecise, {cause}: {}", pairs.len());
    for pair in pairs.iter().take(12) {
      println!("      {pair}");
    }
  }
  for line in &report.incorrect {
    println!("   INCORRECT {line}");
  }
  for line in &report.unsound {
    println!("   UNSOUND {line}");
  }
  assert!(
    report.unsound.is_empty() && report.incorrect.is_empty(),
    "{title}: {} unsound, {} incorrect",
    report.unsound.len(),
    report.incorrect.len()
  );
}

/// Every ordered pair of the types, through `check_pair`.
#[macro_export]
macro_rules! all_pairs {
  ($limit:expr; $($ty:ty),+ $(,)?) => {
    $crate::all_pairs!(@outer $limit; [$($ty),+]; $($ty),+);
  };
  (@outer $limit:expr; $all:tt; $($s:ty),+) => {
    $( $crate::all_pairs!(@inner $limit; $s; $all); )+
  };
  (@inner $limit:expr; $s:ty; [$($d:ty),+]) => {
    $( $crate::check_pair::<$s, $d>($limit); )+
  };
}
