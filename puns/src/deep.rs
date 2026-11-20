//! Proofs of what the self-relative pointers of a target aim at.
//!
//! A [`Layout`](crate::layout::Layout) does not hold what a pointer aims at. The layout of a
//! recursive type would have no end. Const evaluation has no identity of a type or of a static by
//! which an analysis could tell that it has been somewhere before: `TypeId` cannot be compared in
//! `const`, and neither can pointers. An instance of a generic function has such an identity: it
//! exists once, however often it is named. So the proof that a `D` can be read where an `S` is,
//! pointers followed, is a set of instances:
//!
//! - [`target::<X, Y>`](target) exists once for each pair of a type `X` that a pointer of the
//!   source aims at and a type `Y` that the pointer of the target at the same place aims at. Its
//!   `const` block is the whole proof of that pair up to the next pointers ([`behind`]). It
//!   requires that every `X` is a valid `Y` over the bytes of `Y`, with nothing left to check,
//!   and that every pointer of `Y` is, in `X`, a pointer whose target it can read ([`local`]).
//! - [`pair::<A, B>`](pair) names `target` for the pointers that `A` and `B` hold at the same
//!   place, through their fields, variants and elements. It names `target` as a function pointer
//!   that nothing calls. A recursive type thus closes a cycle of instances, which the compiler
//!   makes once. The proof is an induction over the pairs of types that can be met from `S` and
//!   `D`.
//!
//! [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref) call [`proof`], whose
//! `const` blocks are evaluated up to the first pointers, as in any function that runs. Behind a
//! pointer, the `const` block of `target` is evaluated because the compiler makes every function
//! that the code it compiles names, whether that code runs or not. Building a function pointer
//! needs the function, and `rustc` makes it whatever the optimizations. The Reference does not
//! promise this of a `const` block that is not executed. Compile-fail tests hold the compiler to
//! it.
//!
//! `pair` cannot choose what it names, because the compiler also makes the instances that a
//! branch names when a constant rules the branch out. So `pair` pairs by position: the fields of
//! two structs and the variants of two enums in the order of their declarations, and elements
//! with elements. This names more pairs than matter. Where the source has nothing, its side is
//! `()`. `target` has nothing to prove of a type without size, since no pointer aims at one
//! ([`Pointer::one`], [`Pointer::sequence`]), and the type that text aims at is `()`.
//!
//! `local` decides exactly which pairs matter, in `const` code. Every pointer of the target must
//! be, in the source, a pointer at the same offset that `pair` pairs it with, or the build fails.
//! This sets limits, each of which is a refusal:
//!
//! - A field that holds a pointer keeps its position among the fields of its struct.
//! - A variant that holds a pointer keeps its position among the variants of its enum. New
//!   variants go last.
//! - A struct that only holds a pointer is not that pointer.
//!
//! [`Pointer::one`]: crate::layout::Pointer::one
//! [`Pointer::sequence`]: crate::layout::Pointer::sequence

use crate::{
  Fields, Message, Pun, Variants,
  analysis::{Analysis, analyze},
  layout::{Field, Kind, Variant},
};

const LARGER: &str = "behind a pointer, the target is larger than what it is read from";
const UNSPECIFIED: &str = "behind a pointer, the pun depends on a part of the source or of the \
                           target whose layout Rust does not keep from one compilation to the \
                           next (a struct or a tuple of the `Rust` representation, an enum \
                           without `repr`): only the same type at the same place proves \
                           it.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the \
                           `unstable` feature of `puns`";
const UNINIT: &str = "behind a pointer, a byte that the target requires to be initialized is \
                      padding in what it is read from";
const UNPROVEN: &str = "behind a pointer, a pointer of the target is not a pointer of the source \
                        whose target it can read: only a pointer proves a pointer";
const UNCHECKED: &str = "behind a pointer, some values of the source are not valid for the \
                         target, or do not prove one of its rules: nothing is checked behind a \
                         pointer.\nGive the target a type that every value of the source is valid \
                         for";
const UNPAIRED: &str = "a pointer of the target is not, in the source, a pointer of the same \
                        scheme and offset that aims at the same kind of thing (a value, a \
                        sequence, text): only a pointer proves a pointer";
const FIELDS: &str = "a field of the target that holds a pointer is not paired with a field of \
                      the source at the same offset.\nFields are paired in the order of their \
                      declarations: give both structs the same fields, in the same order, up to \
                      the last one that holds a pointer";
const VARIANTS: &str = "a variant of the target that holds a pointer is not paired with the \
                        variant of the source of the same tag.\nVariants are paired in the order \
                        of their declarations: keep the variants of the source where they are, and \
                        add new ones after them";
const ELEMENTS: &str = "elements of the target that hold a pointer are not paired with elements \
                        of the source of the same size";
const LISTS: &str = "an implementation of `Pun` lists fields, variants, elements or a target of a \
                     pointer (`Fields`, `Variants`, `Elem`, `Pointee`) that are not those of its \
                     layout: pointers cannot be followed through it";

/// Fails with the message `why`, if there is one.
///
/// # Panics
///
/// Panics if `why` is `Some`. In const evaluation, the panic is a compile error.
const fn require(why: Option<&'static str>) {
  if let Some(why) = why {
    Message::new(why).panic();
  }
}

/// Proves what the pointers of `D` aim at, when a `D` is read where an `S` is.
///
/// [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref) call it, after their
/// plans have related the bytes of `S` to `D`. It does nothing at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Every pointer of `D` is, in `S`, a pointer at the same place whose target it can read
///   ([`local`]).
/// - For every pair of types that [`pair`] names behind pointers, [`behind`] finds nothing that
///   prevents the read.
///
/// These conditions are checked after monomorphization. `cargo check` does not report a
/// violation, but `cargo build` does.
#[inline(always)]
pub fn proof<S: ?Sized + Pun, D: ?Sized + Pun>() {
  const { require(local::<S, D>(0)) };
  pair::<S, D>();
}

/// Holds the proof that an `X` that a pointer of the source aims at can be read as the `Y` that
/// the pointer of the target at the same place aims at.
///
/// It is never called, only named by [`pair`]. Its `const` block is evaluated because the
/// compiler makes every function that compiled code names.
///
/// # Compile-time errors
///
/// An instance compiles only if [`behind`] finds that an `X` can be read as a `Y`, with no fuel.
fn target<X: Pun, Y: Pun>() {
  const { require(behind::<X, Y>(0)) };
  pair::<X, Y>();
}

/// Names the [`target`] of every pair of pointers that `A` and `B`, at the same place, may hold.
///
/// It names the `target` of `A::Pointee` and `B::Pointee`, which has nothing to prove unless
/// both are pointers. If `B` holds a pointer, it then pairs by position: the fields of `A` and
/// `B` in the order of their declarations, their variants in the same order, and their elements.
#[inline(always)]
fn pair<A: ?Sized + Pun, B: ?Sized + Pun>() {
  // Named, not called: this is what makes a recursive type a finite proof.
  let _: fn() = target::<A::Pointee, B::Pointee>;
  if const { B::LAYOUT.has_pointers() } {
    fields::<A::Fields, B::Fields>();
    variants::<A::Variants, B::Variants>();
    pair::<A::Elem, B::Elem>();
  }
}

/// Applies [`pair`] to the field `FB` of the target and the ones after it, each with the field
/// `FA` of the source at the same position.
#[inline(always)]
fn fields<FA: Fields, FB: Fields>() {
  if const { FB::FIELD.is_some() } {
    pair::<FA::Type, FB::Type>();
    fields::<FA::Next, FB::Next>();
  }
}

/// Applies [`fields`] to the variant `VB` of the target and the ones after it, each with the
/// variant `VA` of the source at the same position.
#[inline(always)]
fn variants<VA: Variants, VB: Variants>() {
  if const { VB::TAG.is_some() } {
    fields::<VA::Fields, VB::Fields>();
    variants::<VA::Next, VB::Next>();
  }
}

// -------------------------------------------------------------------------------------------------
// What is proven of a pair of types, up to the next pointers: `const` code, exact.
//
// With `fuel`, the next pointers are followed as well, that many times: this is how the model
// check asks what `view_ref` would do without failing to compile (`hook::view_ref`). The proof
// itself asks with none, the instances being what follows pointers there.

/// Returns why an `X` that a pointer aims at cannot be read as a `Y`, or `None` if it can.
///
/// A `Y` is read from the first bytes of an `X`. The pointers already showed that `Y` needs no
/// more alignment than `X`, and that the elements of two sequences have the same size. Every
/// value of `X` must be a valid `Y` with nothing left to check, since nothing is checked behind
/// a pointer. If `X` or `Y` has size 0, the pair is not a pair of pointers, and `None` is
/// returned.
///
/// With `fuel` greater than 0, the pointers of `Y` are followed that many more times. The model
/// check asks so (`hook::views`). The proof asks with 0, and follows pointers through the
/// instances of [`target`].
///
/// # Errors
///
/// Returns the message of the first reason found: `Y` is larger than `X`, the pun depends on a
/// layout that Rust leaves to the compiler, a byte that `Y` requires to be initialized is padding
/// in `X`, a pointer of `Y` is not proven by a pointer of `X`, or some value of `X` is not a
/// valid `Y` or does not meet one of its rules. Then the reasons of [`local`].
pub const fn behind<X: Pun, Y: Pun>(fuel: usize) -> Option<&'static str> {
  // Not a pair of pointers: no pointer aims at a type without size (`Pointer::one`,
  // `Pointer::sequence`), and the target type of text is `()`.
  if X::LAYOUT.size() == 0 || Y::LAYOUT.size() == 0 {
    return None;
  }
  let Analysis { fit, relation, .. } = analyze(&X::LAYOUT, &Y::LAYOUT);
  if !fit.shrink {
    return Some(LARGER);
  }
  if relation.unspecified.is_some() {
    return Some(UNSPECIFIED);
  }
  if relation.uninit.is_some() {
    return Some(UNINIT);
  }
  if relation.pointer.is_some() {
    return Some(UNPROVEN);
  }
  if relation.disjoint.is_some() || relation.partial.is_some() {
    return Some(UNCHECKED);
  }
  local::<X, Y>(fuel)
}

/// Returns why a pointer of `B` is not, in `A`, the pointer that [`pair`] pairs it with, or
/// `None` if every pointer of `B` is.
///
/// `A` and `B` are at the same offset of a source and of a target. A pointer of `B` must be
/// paired with a pointer of `A` whose target it can read ([`Pointer::reads_as`]). Fields are
/// paired in the order of their declarations, and must be at the same offset. Variants are paired
/// in the order of their declarations, and must have the same tag. Elements are paired only with
/// elements of the same size. A variant of `B` whose tag no variant of `A` has needs no pair, if
/// the variant of `A` at its position holds no pointer.
/// With `fuel` greater than 0, what two paired pointers aim at is checked as well, by [`behind`]
/// with one less `fuel`.
///
/// # Errors
///
/// Returns a message if the lists of `A` or `B` are not what their layouts hold ([`listed`]), or
/// if a pointer of `B` is not paired with a pointer of `A` as above.
///
/// [`Pointer::reads_as`]: crate::layout::Pointer::reads_as
pub const fn local<A: ?Sized + Pun, B: ?Sized + Pun>(fuel: usize) -> Option<&'static str> {
  let (a, b) = (A::LAYOUT, B::LAYOUT);
  if !b.has_pointers() {
    return None;
  }
  // This walk, and the one of `pair`, are over the lists of the two types: they are what the
  // layouts hold, which the analysis went by, or a pointer could be missed, or taken for another.
  if !listed::<A>() || !listed::<B>() {
    return Some(LISTS);
  }
  match b.kind {
    Kind::Pointer(to) => match a.kind {
      Kind::Pointer(from) if from.reads_as(&to).is_some() => {
        if fuel == 0 {
          None
        } else {
          behind::<A::Pointee, B::Pointee>(fuel - 1)
        }
      }
      _ => Some(UNPAIRED),
    },
    Kind::Struct(..) => local_fields::<A::Fields, B::Fields>(fuel),
    Kind::Enum(_) => local_variants::<A::Variants, A::Variants, B::Variants>(fuel),
    Kind::Array(..) | Kind::Slice(_) => {
      let elements = matches!(a.kind, Kind::Array(..) | Kind::Slice(_))
        && <A::Elem as Pun>::LAYOUT.size() == <B::Elem as Pun>::LAYOUT.size();
      if elements { local::<A::Elem, B::Elem>(fuel) } else { Some(ELEMENTS) }
    }
    Kind::Scalar(_) | Kind::Pattern(_) => None,
  }
}

/// Applies [`local`] to each field of the target from `FB` on that holds a pointer, with the
/// field of the source at the same position.
///
/// # Errors
///
/// Returns a message if such a field of the target has no field of the source at the same
/// position and offset, or if [`local`] returns one for the two fields.
const fn local_fields<FA: Fields, FB: Fields>(fuel: usize) -> Option<&'static str> {
  let Some(b) = FB::FIELD else { return None };
  if b.layout.has_pointers() {
    match FA::FIELD {
      Some(a) if a.offset() == b.offset() => {
        if let Some(why) = local::<FA::Type, FB::Type>(fuel) {
          return Some(why);
        }
      }
      _ => return Some(FIELDS),
    }
  }
  local_fields::<FA::Next, FB::Next>(fuel)
}

/// Applies [`local_fields`] to each variant of the target from `VB` on that holds a pointer, with
/// the variant of the source at the same position, from `VA` on.
///
/// `A0` is the first variant of the source. A variant of the target whose tag no variant of the
/// source has is never read through the target, so it needs no pair. It only has to be where
/// the variant of the source holds no pointer, since [`pair`] names nothing there that matters.
///
/// # Errors
///
/// Returns a message if a variant of the target that holds a pointer is not paired as above, or
/// if [`local_fields`] returns one for two paired variants.
const fn local_variants<A0: Variants, VA: Variants, VB: Variants>(
  fuel: usize,
) -> Option<&'static str> {
  let Some(tag) = VB::TAG else { return None };
  if has_pointers::<VB::Fields>() {
    match VA::TAG {
      Some(other) if other == tag => {
        if let Some(why) = local_fields::<VA::Fields, VB::Fields>(fuel) {
          return Some(why);
        }
      }
      // A variant that the source never is, is never read through the target: it only has to
      // be where `pair` names nothing that matters, which is where the source has no pointer.
      _ if !has_tag::<A0>(tag) && !has_pointers::<VA::Fields>() => {}
      _ => return Some(VARIANTS),
    }
  }
  local_variants::<A0, VA::Next, VB::Next>(fuel)
}

/// Returns whether the lists of `T` are what its layout holds, as [`Pun`] requires.
///
/// The lists are its fields in the order of their declarations, its variants, the type of its
/// elements, and, for a pointer, the type it aims at (`()` for text), of the size and alignment
/// that the pointer says. Only `T` itself is checked. [`local`] checks the types further down.
const fn listed<T: ?Sized + Pun>() -> bool {
  match T::LAYOUT.kind {
    Kind::Struct(fields, _) => listed_fields::<T::Fields>(fields, 0),
    Kind::Enum(layout) => listed_variants::<T::Variants>(layout.variants, 0),
    Kind::Array(elem, _) | Kind::Slice(elem) => <T::Elem as Pun>::LAYOUT.equals(elem),
    Kind::Pointer(pointer) => {
      let target = <T::Pointee as Pun>::LAYOUT;
      target.size() == pointer.size && target.align() == pointer.align
    }
    Kind::Scalar(_) | Kind::Pattern(_) => true,
  }
}

/// Returns whether the list `F` is the `fields` from the one declared at `index` on.
///
/// The type that the list gives a field is checked only if the field holds a pointer.
const fn listed_fields<F: Fields>(fields: &[Field], index: usize) -> bool {
  match F::FIELD {
    None => index == fields.len(),
    Some(field) => {
      index < fields.len()
        && field.index == index
        && (!field.layout.has_pointers() || <F::Type as Pun>::LAYOUT.equals(field.layout))
        && listed_fields::<F::Next>(fields, index + 1)
    }
  }
}

/// Returns whether the list `V` is the `variants` from the one at `index` on: the same tags, in
/// the same order, with the same fields.
const fn listed_variants<V: Variants>(variants: &[Variant], index: usize) -> bool {
  match V::TAG {
    None => index == variants.len(),
    Some(tag) => {
      index < variants.len()
        && variants[index].tag == tag
        && listed_fields::<V::Fields>(variants[index].fields(), 0)
        && listed_variants::<V::Next>(variants, index + 1)
    }
  }
}

/// Returns whether a field of the list `F` holds a pointer.
const fn has_pointers<F: Fields>() -> bool {
  match F::FIELD {
    Some(field) => field.layout.has_pointers() || has_pointers::<F::Next>(),
    None => false,
  }
}

/// Returns whether a variant of the list `V` has the tag `tag`.
const fn has_tag<V: Variants>(tag: u128) -> bool {
  match V::TAG {
    Some(other) => other == tag || has_tag::<V::Next>(tag),
    None => false,
  }
}
