//! Run-time validation of what the analysis could not prove.
//!
//! The functions of this module recurse over *types*: each call is its own instance, in which the
//! layout of the type and the marks of the analysis are constants. A check that is not marked is
//! not compiled, and one that is comes down to a load and a comparison.

use crate::analysis::Marks;
use crate::layout::{Enum, Kind, Rule, Shape, eval, load};
use crate::{Data, Fields, Path, Root, Variants, ptr};

/// The things of a target that have to be checked.
pub(crate) trait Checks {
  const MARKS: Marks;
}

/// Everything.
struct All;

impl Checks for All {
  const MARKS: Marks = Marks::ALL;
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
      Shape::Enum(layout) => validate_enum::<T>(ptr, layout),
    }
  }
}

/// Whatever the enum at `ptr` has the tag of one of its variants, and what that variant
/// requires of its fields: all of it, as nothing is known of what is in an enum.
#[inline(always)]
const unsafe fn validate_enum<T: ?Sized + Data>(ptr: *const T, layout: &Enum) -> bool {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!() };
  // SAFETY: the tag, and what any of the variants requires to be initialized, is: this is
  // checked by the analysis.
  unsafe {
    let tag = load(ptr.cast::<u8>().add(layout.tag.offset), spec.size, spec.endian);
    validate_variants::<T, T::Variants>(ptr, tag)
  }
}

/// Validate the fields of the variant of the enum at `ptr`, among `V` and the ones after it,
/// whose tag is `tag`.
const unsafe fn validate_variants<T: ?Sized + Data, V: Variants>(ptr: *const T, tag: u128) -> bool {
  let Some(expected) = (const { V::TAG }) else { return false };
  // SAFETY: guaranteed by the caller.
  unsafe {
    if tag == expected {
      validate_fields::<All, Root, T, V::Fields>(ptr)
    } else {
      validate_variants::<T, V::Next>(ptr, tag)
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
#[inline(always)]
pub(crate) const unsafe fn check_rules<T: ?Sized + Data>(ptr: *const T, rules: &[Rule]) -> bool {
  let base = ptr.cast::<u8>();
  let (len, tail, size) = tail_of(ptr);
  // What the regions before take of the tail.
  let mut regions = 0;
  // Not a loop, for the first rules: see `layout::eval`.
  macro_rules! unrolled {
    ($($index:literal)+) => {$(
      if $index < rules.len() {
        // SAFETY: the sized fields of the struct are readable.
        match unsafe { check_rule(&rules[$index], base, size, tail, len, regions) } {
          Some(size) => regions += size,
          None => return false,
        }
      }
    )+};
  }
  unrolled!(0 1 2 3 4 5 6 7);
  let mut i = 8;
  while i < rules.len() {
    // SAFETY: the sized fields of the struct are readable.
    match unsafe { check_rule(&rules[i], base, size, tail, len, regions) } {
      Some(size) => regions += size,
      None => return false,
    }
    i += 1;
  }
  true
}

/// The number of elements of the tail of the struct at `ptr`, the size of that tail, and the size
/// of the struct. The tail of a `Sized` struct is its last field.
#[inline(always)]
pub(crate) const fn tail_of<T: ?Sized + Data>(ptr: *const T) -> (usize, usize, usize) {
  let (layout, shape) = const { (T::LAYOUT, T::LAYOUT.tail_shape()) };
  if const { T::LAYOUT.is_unsized() } {
    // The value exists: its size does not overflow.
    let len = ptr::metadata(ptr);
    (len, len * layout.stride(), layout.head() + len * layout.stride())
  } else {
    (shape.len, shape.bytes, layout.size())
  }
}

/// `None` if the rule does not hold, else the size of the region if the rule is one (`0` if
/// not).
///
/// # Safety
/// `base` must be valid for reads of the `size` bytes of a struct that has the rule.
#[inline(always)]
const unsafe fn check_rule(
  rule: &Rule, base: *const u8, size: usize, tail: usize, len: usize, regions: usize,
) -> Option<usize> {
  // SAFETY: guaranteed by the caller.
  let value = unsafe {
    match *rule {
      Rule::Size(ops) | Rule::Len(ops) | Rule::Assert(ops) | Rule::Region(Some(ops)) => {
        eval(ops, base, size)
      }
      Rule::Region(None) => return Some(0),
    }
  };
  let Some(value) = value else { return None };
  match *rule {
    Rule::Size(_) if value == size as u128 => Some(0),
    Rule::Len(_) if value == len as u128 => Some(0),
    Rule::Assert(_) if value != 0 => Some(0),
    Rule::Region(_) if value <= (tail - regions) as u128 => Some(value as usize),
    _ => None,
  }
}
