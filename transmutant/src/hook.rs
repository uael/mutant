//! Test-only access to the verdicts of the analysis, for the model check of
//! `transmutant-model`: the same checks as the public functions, but answering `None` instead of
//! failing to compile.
//!
//! Not part of the API: only compiled with the `verdict` feature, which nothing but tests may
//! enable. It changes nothing else.

use crate::{Data, Error, Need, Plan, Root, transmute_unchecked, validate::validate};

/// What each public function would do with a transmutation from `S` to `D`: whatever it
/// compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Verdict {
  pub transmute: bool,
  pub truncate: bool,
  pub try_transmute: bool,
  pub transmute_ref: bool,
  pub truncate_ref: bool,
  pub try_transmute_ref: bool,
  pub transmute_mut: bool,
  pub try_transmute_mut: bool,
  /// Why `try_transmute_ref` does not compile, if it does not.
  why: ([u8; 512], usize),
}

impl Verdict {
  /// Why `try_transmute_ref` does not compile, if it does not (empty if it does).
  #[must_use]
  pub fn why(&self) -> &str {
    core::str::from_utf8(self.why.0.split_at(self.why.1).0).unwrap_or("(not UTF-8)")
  }
}

/// Whatever the transmutation from `S` to `D` meets the `need`.
const fn ok<S: ?Sized + Data, D: ?Sized + Data>(need: Need) -> bool {
  Plan::<S, D>::check(need).is_none()
}

/// What the analysis says of a transmutation from `S` to `D`.
#[must_use]
pub const fn verdict<S: ?Sized + Data, D: ?Sized + Data>() -> Verdict {
  const VALUE: Need = Need { truncate: false, fallible: false, reference: false };
  const REF: Need = Need { reference: true, ..VALUE };
  const TRUNCATE: Need = Need { truncate: true, ..VALUE };
  const TRY: Need = Need { fallible: true, ..VALUE };
  const TRY_REF: Need = Need { fallible: true, ..REF };
  const TRUNCATE_REF: Need = Need { truncate: true, ..REF };
  let mutable = Plan::<S, D>::MUTABLE;
  let why = match Plan::<S, D>::check(TRY_REF) {
    None => ([0; 512], 0),
    Some(message) => (message.bytes, message.len),
  };
  Verdict {
    transmute: ok::<S, D>(VALUE),
    truncate: ok::<S, D>(TRUNCATE),
    try_transmute: ok::<S, D>(TRY),
    transmute_ref: ok::<S, D>(REF),
    truncate_ref: ok::<S, D>(TRUNCATE_REF),
    try_transmute_ref: ok::<S, D>(TRY_REF),
    transmute_mut: ok::<S, D>(REF) && mutable,
    try_transmute_mut: ok::<S, D>(TRY_REF) && mutable,
    why,
  }
}

/// [`try_transmute`](crate::try_transmute), if it compiles.
#[must_use]
pub const fn try_transmute<S: Data + Copy, D: Data + Copy>(value: S) -> Option<Result<D, Error>> {
  if const { !verdict::<S, D>().try_transmute } {
    return None;
  }
  // SAFETY: as in `try_transmute`, which would compile.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Some(Err(Error::Validity));
  }
  // SAFETY: just validated.
  Some(Ok(unsafe { transmute_unchecked(value) }))
}

/// [`transmute`](crate::transmute), if it compiles.
#[must_use]
pub const fn transmute<S: Data + Copy, D: Data + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().transmute } {
    return None;
  }
  // SAFETY: as in `transmute`, which would compile.
  Some(unsafe { transmute_unchecked(value) })
}

/// [`truncate`](crate::truncate), if it compiles.
#[must_use]
pub const fn truncate<S: Data + Copy, D: Data + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().truncate } {
    return None;
  }
  // SAFETY: as in `truncate`, which would compile.
  Some(unsafe { transmute_unchecked(value) })
}

/// [`try_transmute_ref`](crate::try_transmute_ref), if it compiles.
#[must_use]
pub fn try_transmute_ref<S: ?Sized + Data, D: ?Sized + Data>(
  value: &S,
) -> Option<Result<&D, Error>> {
  if const { !verdict::<S, D>().try_transmute_ref } {
    return None;
  }
  // SAFETY: as in `try_transmute_ref`, which would compile.
  Some(Plan::<S, D>::try_cast(value).map(|target| unsafe { &*target }))
}

/// [`try_transmute_mut`](crate::try_transmute_mut), if it compiles.
#[must_use]
pub fn try_transmute_mut<S: ?Sized + Data, D: ?Sized + Data>(
  value: &mut S,
) -> Option<Result<&mut D, Error>> {
  if const { !verdict::<S, D>().try_transmute_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: as in `try_transmute_mut`, which would compile.
  Some(Plan::<S, D>::try_cast(source).map(|target| unsafe { &mut *target.cast_mut() }))
}

/// [`transmute_ref`](crate::transmute_ref), if it compiles.
#[must_use]
pub const fn transmute_ref<S: ?Sized + Data, D: ?Sized + Data>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().transmute_ref } {
    return None;
  }
  // SAFETY: as in `transmute_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`truncate_ref`](crate::truncate_ref), if it compiles.
#[must_use]
pub const fn truncate_ref<S: ?Sized + Data, D: ?Sized + Data>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().truncate_ref } {
    return None;
  }
  // SAFETY: as in `truncate_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`unpack`](crate::unpack), if it compiles.
#[must_use]
pub const fn unpack<D: ?Sized + Data>(bytes: &[u8]) -> Option<Result<(&D, &[u8]), Error>> {
  if const { !verdict::<[u8], D>().try_transmute_ref } {
    return None;
  }
  Some(crate::unpack_unchecked::<D>(bytes))
}
