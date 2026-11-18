//! Run-time validation of what the analysis could not prove.
//!
//! The functions of this module recurse over *types*: each call is its own instance, in which the
//! layout of the type and the marks of the analysis are constants. A check that is not marked is
//! not compiled, and one that is comes down to a load and a comparison.

use crate::{
  Count, Fields, Path, Pun, Root, Variants, Walks,
  analysis::Marks,
  layout::{Enum, Kind, Machine, Op, Rule, Shape, eval, load, walk_limit},
  ptr,
};

/// The things of a target that have to be checked.
pub trait Checks {
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
pub const unsafe fn validate<C: Checks, P: Path, T: ?Sized + Pun>(ptr: *const T) -> bool {
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
      // Only a pointer proves a pointer, which the analysis made sure of: what may be left to
      // check is that one that may be null is not, where the target has no such value.
      Shape::Pointer(pointer) => {
        pointer.nullable || load(ptr.cast(), pointer.width, pointer.endian) != 0
      }
    }
  }
}

/// Whatever the enum at `ptr` has the tag of one of its variants, and what that variant
/// requires of its fields: all of it, as nothing is known of what is in an enum.
///
/// # Safety
/// As [`validate`], and `layout` must be the one of `T`.
#[inline(always)]
const unsafe fn validate_enum<T: ?Sized + Pun>(ptr: *const T, layout: &Enum) -> bool {
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
///
/// # Safety
/// As [`validate`]: `ptr` must be valid for reads of a `T`, whose variant of tag `tag`, if it
/// has one, has its fields initialized.
const unsafe fn validate_variants<T: ?Sized + Pun, V: Variants>(ptr: *const T, tag: u128) -> bool {
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

/// Validate the `len` elements at `ptr`.
///
/// # Safety
/// `ptr` must be valid for reads of `len` consecutive `T`s, each as [`validate`] requires.
const unsafe fn validate_elems<C: Checks, P: Path, T: Pun>(ptr: *const T, len: usize) -> bool {
  let mut i = 0;
  // Elements whose checks are a few comparisons are checked a block of about 64 bytes at a time,
  // without stopping within a block: then the optimizer can vectorise it, which it cannot do of
  // a loop that may stop at each element. The result is the same, and what is read is in the
  // slice.
  let block = const { T::LAYOUT.block() };
  if block > 1 {
    while len - i >= block {
      let (mut valid, mut j) = (true, 0);
      while j < block {
        // SAFETY: `ptr` points to `len` elements, and `i + j < len`.
        valid &= unsafe { validate::<C, P, T>(ptr.add(i + j)) };
        j += 1;
      }
      if !valid {
        return false;
      }
      i += block;
    }
  }
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
///
/// # Safety
/// As [`validate`], and `F` must be a field of `T`: of the struct, or of the variant of the enum
/// that the value at `ptr` is.
const unsafe fn validate_fields<C: Checks, P: Path, T: ?Sized + Pun, F: Fields>(
  ptr: *const T,
) -> bool {
  let Some(field) = (const { F::FIELD }) else { return true };
  // Where the list says that the field is must be where its layout says it is: the list is made
  // by the derive with paths it is told (`#[pun(crate = ..)]`), which could name another `At`.
  const {
    let leaf = match F::FIELD {
      Some(field) => P::LEAF + field.leaf(),
      None => P::LEAF,
    };
    assert!(<F::Path<P> as Path>::LEAF == leaf, "not where the field is");
  };
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
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Pun>() -> bool {
  // The rules of a struct are its last leaf.
  !T::LAYOUT.rules().is_empty() && C::MARKS.any(P::LEAF + T::LAYOUT.leaves - 1, 1)
}

/// Whatever the struct at `ptr` satisfies its `rules`, which are the ones of the layout of `T`.
///
/// # Safety
/// Same as [`validate`].
#[inline(always)]
pub const unsafe fn check_rules<T: ?Sized + Pun>(ptr: *const T, rules: &[Rule]) -> bool {
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
  // The rules that walk are the ones of the list.
  const { assert!(T::LAYOUT.walk_rules() == count_walks::<T, T::Walks>()) };
  // SAFETY: the bytes of the struct are readable.
  unsafe { check_walks::<T, T::Walks>(base, size) }
}

// -------------------------------------------------------------------------------------------------
// Expressions that walk

/// Number of rules of the list `W`, each of which must be a rule of `T` that walks.
const fn count_walks<T: ?Sized + Pun, W: Walks>() -> usize {
  if W::RULE == usize::MAX {
    return 0;
  }
  assert!(
    T::LAYOUT.walks(W::RULE, <W::Ops as Count>::VALUE, <W::Inner as Count>::VALUE),
    "not a rule that walks"
  );
  1 + count_walks::<T, W::Next>()
}

/// Whatever the struct at `base` satisfies the rules of `T` that walk: `W`, and the ones after
/// it.
///
/// Where [`eval`] is given an expression, which is a constant once it is inlined, the operations
/// are here constants of the instances of the functions that apply them: there is an instance
/// for each operation (`N` is its index, plus one), in which it is the only one that is
/// compiled. The two expressions of a walk are then what their arithmetic is, in a loop.
///
/// The recursions go down types, and stop where they are told to: a struct without such rules
/// costs nothing to compile.
///
/// # Safety
/// `base` must be valid for reads of the `size` bytes of a `T`.
const unsafe fn check_walks<T: ?Sized + Pun, W: Walks>(base: *const u8, size: usize) -> bool {
  if const { W::RULE == usize::MAX } {
    return true;
  }
  let mut machine = Machine::NEW;
  // SAFETY: guaranteed by the caller.
  unsafe {
    steps::<T, W, W::Ops>(&mut machine, base, size)
      && machine.result() != 0
      && check_walks::<T, W::Next>(base, size)
  }
}

/// Apply the `N` first operations of the expression of the rule `W` of `T`: `false` if one of
/// them fails, which is then the failure of the expression.
///
/// # Safety
/// As [`check_walks`]: `base` must be valid for reads of the `size` bytes of a `T`.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn steps<T: ?Sized + Pun, W: Walks, N: Count>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  if const { N::VALUE == 0 } {
    return true;
  }
  // SAFETY: guaranteed by the caller.
  unsafe {
    if !steps::<T, W, N::Less>(machine, base, size) {
      return false;
    }
    if const { T::LAYOUT.walk_at(W::RULE) == N::VALUE.wrapping_sub(1) } {
      return walk::<T, W>(machine, base, size);
    }
    let Some(op) = (const { T::LAYOUT.rule_op(W::RULE, N::VALUE.wrapping_sub(1)) }) else {
      return true;
    };
    machine.apply(op, N::VALUE.wrapping_sub(1), base, size)
  }
}

/// Apply the [`Op::Walk`] of the expression of the rule `W` of `T`: `false` if it fails.
///
/// # Safety
/// As [`check_walks`]: `base` must be valid for reads of the `size` bytes of a `T`.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn walk<T: ?Sized + Pun, W: Walks>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  let Some(Op::Walk { var, .. }) =
    (const { T::LAYOUT.rule_op(W::RULE, T::LAYOUT.walk_at(W::RULE)) })
  else {
    return true;
  };
  // Not computed: in a part of the expression that is not computed.
  if !machine.computes(const { T::LAYOUT.walk_at(W::RULE) }) {
    return true;
  }
  let mut state = machine.top();
  let (mut steps, limit) = (0, walk_limit(size));
  let result = loop {
    let (mut cond, mut next) = (machine.enter(var, state), machine.enter(var, state));
    // SAFETY: guaranteed by the caller.
    if !unsafe { walk_steps::<T, W, false, W::Inner>(&mut cond, base, size) } {
      break None;
    }
    if cond.result() == 0 {
      break Some(state);
    }
    if steps == limit {
      break None;
    }
    steps += 1;
    // SAFETY: guaranteed by the caller.
    if !unsafe { walk_steps::<T, W, true, W::Inner>(&mut next, base, size) } {
      break None;
    }
    state = next.result();
  };
  machine.replace(result)
}

/// Apply the `N` first operations of an expression of the walk of the rule `W` of `T`: its
/// condition, or with `NEXT` what follows.
///
/// # Safety
/// As [`check_walks`]: `base` must be valid for reads of the `size` bytes of a `T`.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn walk_steps<T: ?Sized + Pun, W: Walks, const NEXT: bool, N: Count>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  if const { N::VALUE == 0 } {
    return true;
  }
  // SAFETY: guaranteed by the caller.
  unsafe {
    if !walk_steps::<T, W, NEXT, N::Less>(machine, base, size) {
      return false;
    }
    let Some(op) = (const { T::LAYOUT.walk_op(W::RULE, NEXT, N::VALUE.wrapping_sub(1)) }) else {
      return true;
    };
    machine.apply(op, N::VALUE.wrapping_sub(1), base, size)
  }
}

// -------------------------------------------------------------------------------------------------

/// The number of elements of the tail of the struct at `ptr`, the size of that tail, and the size
/// of the struct. The tail of a `Sized` struct is its last field.
#[inline(always)]
pub const fn tail_of<T: ?Sized + Pun>(ptr: *const T) -> (usize, usize, usize) {
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
  rule: &Rule,
  base: *const u8,
  size: usize,
  tail: usize,
  len: usize,
  regions: usize,
) -> Option<usize> {
  // SAFETY: guaranteed by the caller.
  let value = unsafe {
    match *rule {
      Rule::Size(ops) | Rule::Len(ops) | Rule::Assert(ops) | Rule::Region(Some(ops)) => {
        eval(ops, base, size)
      }
      // See `check_walks`.
      Rule::Region(None) | Rule::Walk(_) => return Some(0),
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
