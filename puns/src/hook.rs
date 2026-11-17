//! Test-only access to the verdicts of the analysis, for the model check of
//! `puns-model`: the same checks as the public functions, but answering `None` instead of
//! failing to compile.
//!
//! Not part of the API: only compiled with the `verdict` feature, which nothing but tests may
//! enable. It changes nothing else.

use crate::{Error, Need, Plan, Pun, Root, pun_unchecked, validate::validate};

/// What each public function would do with a pun from `S` to `D`: whatever it
/// compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Verdict {
  pub pun: bool,
  pub head: bool,
  pub try_pun: bool,
  pub pun_ref: bool,
  pub head_ref: bool,
  pub try_pun_ref: bool,
  pub pun_mut: bool,
  pub try_pun_mut: bool,
  /// Why `try_pun_ref` does not compile, if it does not.
  why: ([u8; 512], usize),
}

impl Verdict {
  /// Why `try_pun_ref` does not compile, if it does not (empty if it does).
  #[must_use]
  pub fn why(&self) -> &str {
    core::str::from_utf8(self.why.0.split_at(self.why.1).0).unwrap_or("(not UTF-8)")
  }
}

/// Whatever the pun from `S` to `D` meets the `need`.
const fn ok<S: ?Sized + Pun, D: ?Sized + Pun>(need: Need) -> bool {
  Plan::<S, D>::check(need).is_none()
}

/// What the analysis says of a pun from `S` to `D`.
#[must_use]
pub const fn verdict<S: ?Sized + Pun, D: ?Sized + Pun>() -> Verdict {
  const VALUE: Need = Need { head: false, fallible: false, reference: false };
  const REF: Need = Need { reference: true, ..VALUE };
  const HEAD: Need = Need { head: true, ..VALUE };
  const TRY: Need = Need { fallible: true, ..VALUE };
  const TRY_REF: Need = Need { fallible: true, ..REF };
  const HEAD_REF: Need = Need { head: true, ..REF };
  let mutable = Plan::<S, D>::MUTABLE;
  let why = match Plan::<S, D>::check(TRY_REF) {
    None => ([0; 512], 0),
    Some(message) => (message.bytes, message.len),
  };
  Verdict {
    pun: ok::<S, D>(VALUE),
    head: ok::<S, D>(HEAD),
    try_pun: ok::<S, D>(TRY),
    pun_ref: ok::<S, D>(REF),
    head_ref: ok::<S, D>(HEAD_REF),
    try_pun_ref: ok::<S, D>(TRY_REF),
    pun_mut: ok::<S, D>(REF) && mutable,
    try_pun_mut: ok::<S, D>(TRY_REF) && mutable,
    why,
  }
}

/// [`try_pun`](crate::try_pun), if it compiles.
#[must_use]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<Result<D, Error>> {
  if const { !verdict::<S, D>().try_pun } {
    return None;
  }
  // SAFETY: as in `try_pun`, which would compile.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Some(Err(Error::Validity));
  }
  // SAFETY: just validated.
  Some(Ok(unsafe { pun_unchecked(value) }))
}

/// [`pun`](crate::pun), if it compiles.
#[must_use]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().pun } {
    return None;
  }
  // SAFETY: as in `pun`, which would compile.
  Some(unsafe { pun_unchecked(value) })
}

/// [`head`](crate::head), if it compiles.
#[must_use]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().head } {
    return None;
  }
  // SAFETY: as in `head`, which would compile.
  Some(unsafe { pun_unchecked(value) })
}

/// [`try_pun_ref`](crate::try_pun_ref), if it compiles.
#[must_use]
pub fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<Result<&D, Error>> {
  if const { !verdict::<S, D>().try_pun_ref } {
    return None;
  }
  // SAFETY: as in `try_pun_ref`, which would compile.
  Some(Plan::<S, D>::try_cast(value).map(|target| unsafe { &*target }))
}

/// [`try_pun_mut`](crate::try_pun_mut), if it compiles.
#[must_use]
pub fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(
  value: &mut S,
) -> Option<Result<&mut D, Error>> {
  if const { !verdict::<S, D>().try_pun_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: as in `try_pun_mut`, which would compile.
  Some(Plan::<S, D>::try_cast(source).map(|target| unsafe { &mut *target.cast_mut() }))
}

/// [`pun_ref`](crate::pun_ref), if it compiles.
#[must_use]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().pun_ref } {
    return None;
  }
  // SAFETY: as in `pun_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`head_ref`](crate::head_ref), if it compiles.
#[must_use]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().head_ref } {
    return None;
  }
  // SAFETY: as in `head_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`from_head`](crate::from_head), if it compiles.
#[must_use]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Option<Result<(&D, &[u8]), Error>> {
  if const { !verdict::<[u8], D>().try_pun_ref } {
    return None;
  }
  Some(crate::from_head_unchecked::<D>(bytes))
}
