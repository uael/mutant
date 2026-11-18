//! Run-time validation of what the analysis could not prove.
//!
//! The functions of this module recurse over *types*: each call is its own instance, in which the
//! layout of the type and the marks of the analysis are constants. A check that is not marked is
//! not compiled, and one that is comes down to a load and a comparison.

use crate::{
  Fields, Path, Pun, Root, Rules, Steps, Variants,
  analysis::Marks,
  layout::{self, Dfa, Enum, Kind, Machine, Op, Rule, Shape, load, op_at, walk_limit},
  ptr,
  steps::{InRule, ONE, Ops, TWO, WALK},
};

/// The things of a target that have to be checked.
pub trait Checks {
  const MARKS: Marks;
}

/// Everything.
pub struct All;

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
      Shape::Struct => {
        validate_fields::<C, P, T, T::Fields>(ptr)
          && (const { !rules_marked::<C, P, T>() } || check_rules(ptr))
      }
      // `str::from_utf8` where the automaton is the one of UTF-8, by its states (an `id` proves
      // nothing): decided at compile time.
      Shape::Pattern(_) if const { matches!(T::LAYOUT.kind, Kind::Pattern(dfa) if dfa.is_utf8()) } => {
        Dfa::matches_utf8(ptr.cast(), ptr::metadata(ptr))
      }
      Shape::Pattern(dfa) => dfa.matches(ptr.cast(), ptr::metadata(ptr)),
      Shape::Enum(layout) if layout.union => validate_union::<T, T::Variants>(ptr),
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
  let this = match const { V::RANGES } {
    None => tag == expected,
    // The variant of a niche, or one that no value is (no range).
    Some(ranges) => layout::contains(ranges, tag),
  };
  // SAFETY: guaranteed by the caller.
  unsafe {
    if this {
      validate_fields::<All, Root, T, V::Fields>(ptr)
    } else {
      validate_variants::<T, V::Next>(ptr, tag)
    }
  }
}

/// Whatever the union at `ptr` is one of its fields, `V` or one after it (each of which is a
/// variant of one field).
///
/// # Safety
/// As [`validate`], and every byte that one of the fields requires to be initialized must be:
/// the analysis refuses a union of which a field would read a byte that may be padding.
const unsafe fn validate_union<T: ?Sized + Pun, V: Variants>(ptr: *const T) -> bool {
  if const { V::TAG.is_none() } {
    return false;
  }
  // SAFETY: guaranteed by the caller.
  unsafe { validate_fields::<All, Root, T, V::Fields>(ptr) || validate_union::<T, V::Next>(ptr) }
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

/// Whatever the struct at `ptr` satisfies the rules of the layout of `T`.
///
/// # Safety
/// Same as [`validate`].
#[inline(always)]
pub const unsafe fn check_rules<T: ?Sized + Pun>(ptr: *const T) -> bool {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let (len, tail, size) = tail_of(ptr);
  // SAFETY: the bytes of the struct are readable.
  unsafe { check_listed::<T, T::Rules>(ptr.cast(), size, tail, len, 0) }
}

/// Whatever the struct at `base` satisfies the rule `L` of `T` and the ones after it, the regions
/// before taking `regions` bytes of its tail of `tail` bytes, `len` elements.
///
/// # Safety
/// `base` must be valid for reads of the `size` bytes of a `T`.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_listed<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  tail: usize,
  len: usize,
  regions: usize,
) -> bool {
  if const { L::RULE == usize::MAX } {
    return true;
  }
  // SAFETY: guaranteed by the caller.
  let Some(value) = (unsafe { eval::<InRule<T, L>, L::Expr>(base, size) }) else { return false };
  let regions = match const { T::LAYOUT.rule(L::RULE) } {
    Some(Rule::Size(_)) if value == size as u128 => regions,
    Some(Rule::Len(_)) if value == len as u128 => regions,
    Some(Rule::Assert(_)) if value != 0 => regions,
    Some(Rule::Region(_)) if value <= (tail - regions) as u128 => regions + value as usize,
    _ => return false,
  };
  // SAFETY: guaranteed by the caller.
  unsafe { check_listed::<T, L::Next>(base, size, tail, len, regions) }
}

/// Whatever `L` lists the rules of `T` that have an expression from the `from`th one, each once,
/// in order, with every operation of its expression (as [`covers`] tells).
const fn listed<T: ?Sized + Pun, L: Rules>(from: usize) -> bool {
  let rules = T::LAYOUT.rules();
  let mut i = from;
  while i < rules.len() && rules[i].ops().is_none() {
    i += 1;
  }
  if L::RULE == usize::MAX {
    return i >= rules.len();
  }
  L::RULE == i && covers::<InRule<T, L>, L::Expr>() && listed::<T, L::Next>(i + 1)
}

/// Whatever `S` is every operation of the expression `X`, each once, in order, with the walks
/// where they are. (That each of its operations is a walk where `S` says that it is one, and only
/// there, and the same of the expressions of each walk, is asserted where it is applied.)
const fn covers<X: Ops, S: Steps>() -> bool {
  !X::OPS.is_empty() && S::FIRST == 0 && S::END == X::OPS.len()
}

/// The `size` or `len` rule of the struct `T`, or of the struct that its last field is (and so
/// on): its value over the struct at `base`, of which `n` bytes can be read, `None` if it fails.
/// This is how `from_head` finds how long a value is; [`layout::Layout::length_rule`] tells which
/// rule it is, and where, which this finds by the types.
///
/// # Safety
/// `base` must be valid for reads of the sized fields of the struct that has the rule, and of `n`
/// bytes, initialized. `T` must have such a rule, by `length_rule`.
#[inline(always)]
pub const unsafe fn length<T: ?Sized + Pun>(base: *const u8, n: usize) -> Option<u128> {
  // The rule is the struct's own, or of its last field: the one that `length_rule` finds.
  let (own, offset) = const {
    let rule = T::LAYOUT.length_rule();
    let own = match T::LAYOUT.length_rule() {
      Some(rule) => rule.base == 0 && sizing::<T, T::Rules>() != usize::MAX,
      None => false,
    };
    let fields = T::LAYOUT.struct_fields();
    let offset = if fields.is_empty() { 0 } else { fields[fields.len() - 1].offset };
    if let (Some(rule), false) = (rule, own) {
      // The last field, by the types, is the last field of the layout, which has that rule.
      let last = <<T::Fields as Fields>::Last as Fields>::FIELD;
      let inner = <<<T::Fields as Fields>::Last as Fields>::Type as Pun>::LAYOUT.length_rule();
      assert!(
        matches!(last, Some(field) if field.offset() == offset)
          && matches!(inner, Some(inner) if inner.base + offset == rule.base
            && inner.elements == rule.elements
            && layout::same_ops(inner.ops, rule.ops)),
        "not the last field of the struct"
      );
    }
    (own, offset)
  };
  if own {
    // SAFETY: guaranteed by the caller.
    return unsafe { length_listed::<T, T::Rules>(base, n) };
  }
  // SAFETY: the struct that has the rule is in the last field, at `offset`.
  unsafe { length::<<<T::Fields as Fields>::Last as Fields>::Type>(base.add(offset), n - offset) }
}

/// The index of the first `size` or `len` rule of `T` in `L`, `usize::MAX` if none.
const fn sizing<T: ?Sized + Pun, L: Rules>() -> usize {
  match T::LAYOUT.rule(L::RULE) {
    _ if L::RULE == usize::MAX => usize::MAX,
    Some(Rule::Size(_) | Rule::Len(_)) => L::RULE,
    _ => sizing::<T, L::Next>(),
  }
}

/// The value of the first `size` or `len` rule of `T` in `L`.
///
/// # Safety
/// As [`length`].
#[inline(always)]
const unsafe fn length_listed<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  n: usize,
) -> Option<u128> {
  if const { L::RULE == usize::MAX } {
    return None;
  }
  if const { matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Size(_) | Rule::Len(_))) } {
    const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
    // SAFETY: guaranteed by the caller.
    return unsafe { eval::<InRule<T, L>, L::Expr>(base, n) };
  }
  // SAFETY: guaranteed by the caller.
  unsafe { length_listed::<T, L::Next>(base, n) }
}

/// The bytes of the `index`th region of the tail of the struct `T` at `base`, whose size is
/// `size`, whose tail starts at `head` and is `tail` bytes long: from the rule `L` on, the regions
/// before taking `start` bytes, `count` of them.
///
/// # Safety
/// `base` must be valid for reads of the `size` bytes of a `T`.
#[inline(always)]
pub const unsafe fn region_at<'a, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  index: usize,
  (start, count): (usize, usize),
) -> &'a [u8] {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let rest = tail - start;
  // The last region, which takes what remains, has no expression: it is not in the list.
  if const { L::RULE == usize::MAX } {
    assert!(count == index && T::LAYOUT.has_last_region(), "no such region");
    // SAFETY: `start <= tail`, the size of what follows the `head` first bytes.
    return unsafe { core::slice::from_raw_parts(base.add(head + start), rest) };
  }
  if const { !matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Region(_))) } {
    // SAFETY: guaranteed by the caller.
    return unsafe { region_at::<T, L::Next>(base, size, (head, tail), index, (start, count)) };
  }
  // A region that does not fit is cut short: the fields that size it are not trusted.
  // SAFETY: guaranteed by the caller.
  let region = match unsafe { eval::<InRule<T, L>, L::Expr>(base, size) } {
    Some(region) if region <= rest as u128 => region as usize,
    _ => rest,
  };
  if count == index {
    // SAFETY: `start + region <= tail`, the size of what follows the `head` first bytes.
    return unsafe { core::slice::from_raw_parts(base.add(head + start), region) };
  }
  // SAFETY: guaranteed by the caller.
  unsafe { region_at::<T, L::Next>(base, size, (head, tail), index, (start + region, count + 1)) }
}

// -------------------------------------------------------------------------------------------------
// Expressions

/// The value of the expression `X`, whose operations are `S`, over the struct at `base`, whose
/// size is `size` (or, for a `size` or `len` rule, of which `size` bytes can be read): `None` if
/// it fails.
///
/// The operations are constants of the instances of the functions that apply them, one instance
/// for each ([`steps`], [`walk`]), in which it is the only one that is compiled: what is compiled
/// is the arithmetic of the expression, and the loops of its walks, never an interpreter. The
/// recursions go down types, a tree of the operations: as deep as its logarithm.
///
/// # Safety
/// `base` must be valid for reads of the sized fields of the struct whose rule `X` is an
/// expression of, and of `size` bytes, initialized.
#[inline(always)]
const unsafe fn eval<X: Ops, S: Steps>(base: *const u8, size: usize) -> Option<u128> {
  let mut machine = Machine::NEW;
  // SAFETY: guaranteed by the caller.
  if unsafe { steps::<X, S>(&mut machine, base, size) } { Some(machine.result()) } else { None }
}

/// Apply the operations `S` of the expression `X`: `false` if one of them fails, which is then the
/// failure of the expression.
///
/// # Safety
/// As [`eval`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn steps<X: Ops, S: Steps>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  // An operation that `S` says is not a walk is not one: this is what makes every walk applied
  // by `walk`, which `Machine::apply` does not do.
  const {
    assert!(
      S::KIND != ONE || matches!(op_at(X::OPS, S::AT), Some(op) if !matches!(op, Op::Walk { .. })),
      "not the operations of the expression"
    );
  };
  // SAFETY: guaranteed by the caller.
  unsafe {
    match const { S::KIND } {
      ONE => match const { op_at(X::OPS, S::AT) } {
        Some(op) => machine.apply(op, S::AT, base, size),
        None => false,
      },
      TWO => {
        if !steps::<X, S::A>(machine, base, size) {
          return false;
        }
        steps::<X, S::B>(machine, base, size)
      }
      WALK => walk::<X, S>(machine, base, size),
      _ => true,
    }
  }
}

/// Apply the [`Op::Walk`] `W` of the expression `X`: `false` if it fails.
///
/// # Safety
/// As [`eval`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn walk<X: Ops, W: Steps>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  // A walk where `W` says, whose expressions are every operation of both of its own.
  const {
    assert!(
      W::KIND != WALK
        || matches!(op_at(X::OPS, W::AT), Some(Op::Walk { .. }))
          && covers::<W::InCond<X>, W::Cond>()
          && covers::<W::InThen<X>, W::Then>(),
      "not the operations of the expression"
    );
  };
  let Some(Op::Walk { var, .. }) = (const { op_at(X::OPS, W::AT) }) else {
    return false;
  };
  // Not computed: in a part of the expression that is not computed.
  if !machine.computes(W::AT) {
    return true;
  }
  let mut state = machine.top();
  let (mut steps, limit) = (0, walk_limit(size));
  let result = loop {
    let (mut cond, mut next) = (machine.enter(var, state), machine.enter(var, state));
    // SAFETY: guaranteed by the caller.
    if !unsafe { self::steps::<W::InCond<X>, W::Cond>(&mut cond, base, size) } {
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
    if !unsafe { self::steps::<W::InThen<X>, W::Then>(&mut next, base, size) } {
      break None;
    }
    state = next.result();
  };
  machine.replace(result)
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
