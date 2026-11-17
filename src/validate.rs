//! Run-time validation of what the analysis could not prove.
//!
//! The functions of this module recurse over *types*: each call is its own instance, in which the
//! layout of the type and the marks of the analysis are constants. A check that is not marked is
//! not compiled, and one that is comes down to a load and a comparison.

use crate::analysis::Marks;
use crate::layout::{Rule, Shape, eval};
use crate::{Data, Fields, Path, ptr};

/// The things of a target that have to be checked.
pub(crate) trait Checks {
  const MARKS: Marks;
}

/// Whatever the marked things of the `T` at `ptr` are valid. `P` tells where `T` is in the target.
///
/// # Safety
/// `ptr` must be valid for reads of a `T`, whose bytes that are not padding must be initialized.
/// It does not have to be aligned.
pub(crate) const unsafe fn validate<C: Checks, P: Path, T: ?Sized + Data>(ptr: *const T) -> bool {
  if const { !C::MARKS.any(P::LEAF, T::LAYOUT.leaves) } {
    return true;
  }
  // SAFETY: guaranteed by the caller; the layout of `T` tells what `ptr` points to.
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(spec) => spec.matches(ptr.cast()),
      Shape::Array(len) => validate_elems::<C, P, T::Elem>(ptr.cast(), len),
      Shape::Slice => validate_elems::<C, P, T::Elem>(ptr.cast(), ptr::metadata(ptr)),
      Shape::Struct(rules) => {
        validate_fields::<C, P, T, T::Fields>(ptr)
          && (const { !rules_marked::<C, P, T>() } || check_rules(ptr, rules))
      }
      Shape::Pattern(dfa) => dfa.matches(ptr.cast(), ptr::metadata(ptr)),
    }
  }
}

const unsafe fn validate_elems<C: Checks, P: Path, T: Data>(ptr: *const T, len: usize) -> bool {
  let mut i = 0;
  while i < len {
    // SAFETY: `ptr` points to `len` elements.
    if !unsafe { validate::<C, P, T>(ptr.add(i)) } {
      return false;
    }
    i += 1;
  }
  true
}

/// Validate the field `F` of the struct `T` at `ptr`, and the fields after it.
const unsafe fn validate_fields<C: Checks, P: Path, T: ?Sized + Data, F: Fields>(
  ptr: *const T,
) -> bool {
  let Some(field) = (const { F::FIELD }) else { return true };
  // SAFETY: `F` is a field of `T`: it is where it says it is.
  unsafe {
    let valid = match field.restriction() {
      // A scalar that accepts less than its type does: there is no type to recurse into.
      Some(spec) => {
        (const { !C::MARKS.any(<F::Path<P> as Path>::LEAF, 1) })
          || spec.matches(ptr.cast::<u8>().add(field.offset()))
      }
      None => validate::<C, F::Path<P>, F::Type>(ptr::field::<T, F::Type>(ptr, field.offset())),
    };
    valid && validate_fields::<C, P, T, F::Next>(ptr)
  }
}

/// Whatever the rules of the struct `T` have to be checked.
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Data>() -> bool {
  // The rules of a struct are its last leaf.
  !T::LAYOUT.rules().is_empty() && C::MARKS.any(P::LEAF + T::LAYOUT.leaves - 1, 1)
}

/// Whatever the struct at `ptr` satisfies its `rules`.
///
/// # Safety
/// Same as [`validate`].
pub(crate) const unsafe fn check_rules<T: ?Sized + Data>(ptr: *const T, rules: &[Rule]) -> bool {
  let (head, stride) = const { (T::LAYOUT.head(), T::LAYOUT.stride()) };
  let (base, len) = (ptr.cast::<u8>(), ptr::metadata(ptr));
  // The value exists: its size does not overflow.
  let tail = len * stride;
  let (mut regions, mut i) = (0, 0);
  while i < rules.len() {
    // SAFETY: the sized fields of the struct are readable.
    let valid = unsafe {
      match rules[i] {
        Rule::Size(ops) => matches!(eval(ops, base), Some(size) if size == (head + tail) as u128),
        Rule::Len(ops) => matches!(eval(ops, base), Some(count) if count == len as u128),
        Rule::Assert(ops) => matches!(eval(ops, base), Some(value) if value != 0),
        Rule::Region(Some(ops)) => match eval(ops, base) {
          Some(size) if size <= (tail - regions) as u128 => {
            regions += size as usize;
            true
          }
          _ => false,
        },
        Rule::Region(None) => true,
      }
    };
    if !valid {
      return false;
    }
    i += 1;
  }
  true
}
