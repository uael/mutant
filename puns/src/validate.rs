//! Run-time checks of what the analysis could not prove.
//!
//! The functions of this module recurse over types, not over values. Each call is an instance of
//! its own, in which the layout of the type and the marks of the analysis are constants. A check
//! that is not marked is not compiled. A check that is marked comes down to a load and a
//! comparison.

use crate::{
  Fields, Path, Pun, Root, Rules, Steps, Variants,
  analysis::Marks,
  layout::{self, Dfa, Enum, Kind, Machine, Op, Rule, Shape, load, op_at, walk_limit},
  narrow::{Planned, fit},
  ptr,
  steps::{InRule, ONE, Ops, TWO, WALK},
};

/// The parts of a target that a validation checks.
pub trait Checks {
  /// The constrained scalars and rules to check, by index of leaf in the target.
  const MARKS: Marks;
}

/// The [`Checks`] that check every part.
pub struct All;

impl Checks for All {
  const MARKS: Marks = Marks::ALL;
}

/// Returns whether the parts of the `T` at `ptr` that `C` marks are valid.
///
/// `P` is where `T` is in the target, which tells the indexes of its leaves. Parts that `C` does
/// not mark are not checked, and their checks are not compiled. An enum and a union are checked
/// as a whole once one of their leaves is marked.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - The bytes of that `T` that are not padding in its layout are initialized.
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

/// Returns whether the enum at `ptr` has the tag of one of its variants, and the fields of that
/// variant are valid.
///
/// Every part of the variant is checked, since nothing is known of what is in an enum.
///
/// # Panics
///
/// Panics if the tag of `layout` is not a scalar. No layout made by this crate has such a tag.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate`] hold.
/// - `layout` is the layout of `T`.
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

/// Returns whether the variant of tag `tag`, among `V` and the ones after it, has valid fields
/// in the enum at `ptr`.
///
/// Returns `false` if no such variant has the tag. A variant whose tag is a range of values (the
/// variant of a niche) has the tag if `tag` is in that range.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`.
/// - If a variant has the tag `tag`, the bytes of its fields that are not padding are
///   initialized.
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

/// Returns whether the union at `ptr` is a valid value of one of its fields, `V` or one after it.
///
/// Each field of a union is a variant of one field in its layout.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate`] hold.
/// - Every byte that one of the fields requires to be initialized is initialized. The analysis
///   refuses a union of which a field would read a byte that may be padding in the source.
const unsafe fn validate_union<T: ?Sized + Pun, V: Variants>(ptr: *const T) -> bool {
  if const { V::TAG.is_none() } {
    return false;
  }
  // SAFETY: guaranteed by the caller.
  unsafe { validate_fields::<All, Root, T, V::Fields>(ptr) || validate_union::<T, V::Next>(ptr) }
}

/// Returns whether the parts that `C` marks of the `len` elements at `ptr` are valid.
///
/// Elements whose checks are a few comparisons are checked in blocks of about 64 bytes, without a
/// branch within a block. The result is the same as one element at a time.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of `len` consecutive `T`s, each as [`validate`] requires.
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

/// Returns whether the parts that `C` marks of the field `F` of the `T` at `ptr`, and of the
/// fields after it, are valid.
///
/// A field restricted by `#[pun(valid(..))]` is checked against its restriction, not against its
/// type.
///
/// # Compile-time errors
///
/// An instance compiles only if `F::Path<P>` is the leaf of the field in the layout. The derive
/// writes the list with paths that the user may redefine, so this is checked.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate`] hold.
/// - `F` is a field of `T`: of the struct, or of the variant of the enum that the value at `ptr`
///   is.
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

/// Returns whether `C` marks the rules of the struct `T` at the place `P`.
///
/// The rules of a struct are its last leaf.
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Pun>() -> bool {
  // The rules of a struct are its last leaf.
  !T::LAYOUT.rules().is_empty() && C::MARKS.any(P::LEAF + T::LAYOUT.leaves - 1, 1)
}

/// Returns whether the struct at `ptr` meets the rules of the layout of `T`.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists every rule of `T` that has an expression, once,
/// in order, with every operation of each ([`listed`]).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate`] hold.
#[inline(always)]
pub const unsafe fn check_rules<T: ?Sized + Pun>(ptr: *const T) -> bool {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let (len, tail, size) = tail_of(ptr);
  // SAFETY: the bytes of the struct are readable.
  unsafe { check_listed::<T, T::Rules>(ptr.cast(), size, tail, len, 0) }
}

/// Returns whether the struct at `base` meets the rule `L` of `T` and the ones after it.
///
/// The struct is `size` bytes, and its tail is `tail` bytes of `len` elements. The regions before
/// `L` take the first `regions` bytes of the tail. A `size` rule must be `size`, a `len` rule
/// `len`, and an assertion not 0. A region must fit in what is left of the tail.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, initialized.
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

/// Returns whether `L` lists the rules of `T` that have an expression, from the `from`th rule on.
///
/// Each must be listed once, in order, with every operation of its expression ([`covers`]).
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

/// Returns whether `S` spans every operation of the expression `X`, from the first to the last.
///
/// That `S` has a walk where `X` has one, and only there, and the same of the expressions of each
/// walk, is asserted where each operation is applied ([`steps`], [`walk`]).
const fn covers<X: Ops, S: Steps>() -> bool {
  !X::OPS.is_empty() && S::FIRST == 0 && S::END == X::OPS.len()
}

/// Returns the value of the `size` or `len` rule that sets the length of the `T` at `base`.
///
/// The rule is the one of `T`, or of its last field, and so on. [`layout::Layout::length_rule`]
/// tells which rule it is and where. This function finds the same rule through the types.
/// `from_head` uses it to find how long a value is.
///
/// # Errors
///
/// Returns `None` if the expression of the rule fails, as when it reads beyond the `n` bytes.
///
/// # Compile-time errors
///
/// An instance compiles only if, where the rule is not the one of `T`, the last field of
/// `T::Fields` is the last field of the layout, and has that rule.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of `n` bytes, initialized.
/// - These bytes cover the sized fields of the struct that has the rule.
/// - `T` has such a rule, by `length_rule`.
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

/// Returns the index of the first `size` or `len` rule of `T` in `L`, or `usize::MAX` if there
/// is none.
const fn sizing<T: ?Sized + Pun, L: Rules>() -> usize {
  match T::LAYOUT.rule(L::RULE) {
    _ if L::RULE == usize::MAX => usize::MAX,
    Some(Rule::Size(_) | Rule::Len(_)) => L::RULE,
    _ => sizing::<T, L::Next>(),
  }
}

/// Returns the value of the first `size` or `len` rule of `T` in `L`.
///
/// # Errors
///
/// Returns `None` if there is no such rule or if its expression fails.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`length`] hold.
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

/// Returns the bytes of the `index`th region of the tail of the struct `T` at `base`.
///
/// The struct is `size` bytes. Its tail starts at `head` and is `tail` bytes long. The search
/// starts at the rule `L`, after `count` regions that take the first `start` bytes of the tail. A
/// region whose expression fails, or does not fit in what is left of the tail, takes what is
/// left: the fields that size it are not trusted. The last region, which has no expression,
/// takes what is left too.
///
/// # Panics
///
/// Panics if `T` has no `index`th region.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, initialized.
/// - `head + tail` is at most `size`, and `start` is at most `tail`.
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

/// Returns the value of the expression `X`, whose operations are `S`, over the struct at `base`.
///
/// The struct is `size` bytes. For a `size` or `len` rule, `size` is the number of bytes that can
/// be read. Each operation is a constant of an instance of [`steps`] or [`walk`] of its own, in
/// which it is the only operation compiled. So what is compiled is the arithmetic of the
/// expression and the loops of its walks, never an interpreter. The recursions go down a
/// balanced tree of the operations, so they are as deep as its logarithm.
///
/// # Errors
///
/// Returns `None` if the expression fails: an overflow, a division by zero, a shift by 128 or
/// more, or a read out of the `size` bytes.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of `size` bytes, initialized.
/// - These bytes cover the sized fields of the struct of which `X` is a rule.
#[inline(always)]
const unsafe fn eval<X: Ops, S: Steps>(base: *const u8, size: usize) -> Option<u128> {
  let mut machine = Machine::NEW;
  // SAFETY: guaranteed by the caller.
  if unsafe { steps::<X, S>(&mut machine, base, size) } { Some(machine.result()) } else { None }
}

/// Applies the operations `S` of the expression `X` to `machine`.
///
/// # Errors
///
/// Returns `false` if one of the operations fails. That failure is the failure of the
/// expression.
///
/// # Compile-time errors
///
/// An instance compiles only if `S` is a walk exactly where `X` has an [`Op::Walk`].
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`eval`] hold.
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
        Some(op) => machine.apply(op, S::AT, const { Planned::<X>::PLAN.how[S::AT] }, base, size),
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

/// Applies the [`Op::Walk`] `W` of the expression `X` to `machine`.
///
/// The state starts as the top of the stack. While the condition holds, the next expression
/// replaces it. A walk that is in a part of the expression that is not computed does nothing.
///
/// # Errors
///
/// Returns `false` if the condition or the next expression fails, or if the walk takes more steps
/// than [`walk_limit`] allows for `size`.
///
/// # Compile-time errors
///
/// An instance compiles only if `X` has an [`Op::Walk`] where `W` is a walk, and `W` spans every
/// operation of its two expressions.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`eval`] hold.
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
  // What the state may be, which the optimizer cannot find in a loop: the width it fits.
  let fits = const { Planned::<X>::PLAN.how[W::AT].result };
  let mut state = fit(machine.top(), fits);
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
    state = fit(next.result(), fits);
  };
  machine.replace(result)
}

// -------------------------------------------------------------------------------------------------

/// Returns the number of elements of the tail of the struct at `ptr`, the size of that tail in
/// bytes, and the size of the struct.
///
/// The tail of a `Sized` struct is its last field.
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
