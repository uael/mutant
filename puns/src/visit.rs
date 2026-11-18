//! Validation of a value in a buffer that holds self-relative pointers, for the crate that has
//! them: [`validate_in`], [`validate_elems_in`], and what they hand each pointer to, [`Visit`].
//!
//! The functions recurse over types, as `validate` does: each call is an instance in which the
//! layout is a constant, and what it says cannot be invalid is not compiled. A part of a value
//! that holds no pointer is validated by `validate` itself, the same code as `try_pun_ref`; only
//! where it fails is it walked again, to tell where.

use core::fmt;

use crate::{
  Fields, Pun, Root, Variants,
  layout::{self, Enum, Kind, Shape, load},
  ptr,
  validate::{All, check_rules, validate},
};

/// What a validation of the bytes of a buffer does with each self-relative pointer it meets.
///
/// [`validate_in`] and [`validate_elems_in`] hand each pointer of the value they validate to the
/// visitor: the crate that has the pointers follows them, as its scheme says, and validates what
/// they aim at in turn (or queues it).
///
/// `pointer::<P>(at)` is called once for each pointer that the value holds where it is (in its
/// fields, in the variant that its tag tells, in the elements of its arrays), in the order of
/// their positions, interleaved with the checks of the rest: `P` is the type of the pointer (its
/// `LAYOUT` is a pointer: [`Layout::as_pointer`](layout::Layout::as_pointer)), `at` its position
/// in the bytes. Bytes of a pointer that are all zero are no pointer: not handed over, valid if
/// the pointer may be null, else invalid there. An error returned by `pointer` ends the
/// validation, with that error.
///
/// ```
/// use puns::{Invalid, Pun, Visit};
///
/// /// Refuses every pointer: for values that hold none.
/// struct NoPointers;
///
/// impl Visit for NoPointers {
///   type Error = Invalid;
///   fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
///     Err(Invalid::at(at))
///   }
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Flags {
///   on: bool,
///   #[pun(valid(..=3))]
///   level: u8,
/// }
///
/// let bytes = [9, 1, 2, 1, 4];
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 1, &mut NoPointers), Ok(()));
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 3, &mut NoPointers), Err(Invalid::at(4)));
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 4, &mut NoPointers), Err(Invalid::at(4)));
/// ```
pub trait Visit {
  /// What ends a validation: [`Invalid`], or an error of the visitor's own.
  type Error: From<Invalid>;

  /// The pointer of type `P` at the position `at` of the bytes.
  ///
  /// # Errors
  /// Whatever the visitor finds wrong with it; the validation ends with that error.
  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Self::Error>;
}

/// Where bytes stop being a valid value ([`validate_in`]): a position in the bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Invalid {
  /// The position of the scalar that is not valid, of the tag of an enum that is no variant's,
  /// of the union that is none of its fields, or of the struct of which a rule does not hold; of
  /// the value if its bytes are not all there.
  pub at: usize,
}

impl Invalid {
  /// Bytes that stop being a valid value at `at`.
  #[must_use]
  pub const fn at(at: usize) -> Self {
    Self { at }
  }
}

impl fmt::Debug for Invalid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Invalid {{ at: {} }}", self.at)
  }
}

impl fmt::Display for Invalid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "invalid bytes at {}", self.at)
  }
}

impl core::error::Error for Invalid {}

/// What a self-relative pointer type `P` aims at: one value, or an element of a sequence; `()`
/// for text. What a [`Visit`] validates behind a pointer.
pub type Pointee<P> = <P as Pun>::Pointee;

/// Validates the `T` at the position `at` of `bytes`, everything of it but what its pointers aim
/// at, and hands each of its pointers to `visit` ([`Visit`]).
///
/// It is safe, for any bytes and any `at`: it reads nothing out of `bytes`, does not panic, and
/// does not recurse through pointers (the visitor does, or does not). The bytes may be at any
/// address.
///
/// # The contract
///
/// After `Ok(())`, unsafe code may rely on this, and on nothing more:
///
/// 1. **In bounds**: `at + size_of::<T>()` is at most `bytes.len()`.
/// 2. **Valid but for what pointers aim at**: the `size_of::<T>()` bytes from `at` are a valid
///    `T` as its layout says ([`Pun`]), provided that every pointer handed to `visit` is a valid
///    pointer of its type (S1 to S7 of [`SelfRelative`](crate::SelfRelative)): every scalar is
///    one of its valid values (`bool`, `char`, `NonZero`, enums without fields, `valid(..)` of a
///    field), the tag of every enum is the tag of a variant (or in its niche) whose fields are
///    valid, every union is one of its fields, and every rule of every struct holds (`assert`,
///    `size`, and any other), evaluated on the bytes of that struct only.
/// 3. **Every pointer handed over**: each self-relative pointer of the value, where it is, was
///    handed to `visit` once, with its type and its position, and `visit` returned `Ok`; but
///    bytes of a pointer that are all zero, which are no pointer: those of a pointer that may be
///    null (none, then), not of one that may not (which is not valid).
///
/// It says nothing of what the pointers aim at: whether they aim in the bytes, at what, whether
/// two aim at the same place. That is the visitor's business, as is the scheme of the pointers.
/// And it says nothing of addresses: it knows positions in `bytes` only. A `&T` made at
/// `bytes.as_ptr() + at` must be at an address aligned for `T` (`align_of::<T>()`), which is the
/// caller's duty: the buffer at an address so aligned, and `at` a multiple of it. Likewise for
/// what the pointers aim at, which their scheme places.
///
/// A `T` without size has no bytes to check: it is valid if it has a value. `T` is `Sized`: a
/// value whose size its bytes tell (a slice tail, `str`) is not one that a buffer of pointers
/// holds by value, and does not compile here.
///
/// # Errors
/// [`Invalid`] where the bytes stop being a valid `T` (the first such place, in the order of the
/// positions of what is checked; the error of the visitor if it comes first).
#[inline]
pub fn validate_in<T: Pun, V: Visit>(
  bytes: &[u8],
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let size = const { T::LAYOUT.size() };
  match bytes.get(at..).and_then(|rest| rest.get(..size)) {
    // SAFETY: the `size` bytes of a `T` at `at`, initialized, as `value` requires.
    Some(_) => unsafe { value::<T, V>(bytes.as_ptr(), at, visit) },
    None => Err(Invalid::at(at).into()),
  }
}

/// Validates the `count` values of `T` from the position `at` of `bytes`, `size_of::<T>()`
/// apart, in order, as [`validate_in`] validates one.
///
/// Elements that hold no pointer are validated as `try_pun_ref` does a slice, by blocks where it
/// can (then only the first invalid one is walked again, to tell where); elements that hold
/// pointers one by one, each pointer handed to `visit` in order. Elements of which every value is
/// valid are not looked at.
///
/// The contract is that of [`validate_in`], for each of the `count` values; in bounds means that
/// `at + count * size_of::<T>()` does not overflow and is at most `bytes.len()`.
///
/// # Errors
/// As [`validate_in`]: at the first element that is not valid, or if they are not all there (at
/// `at`).
#[inline]
pub fn validate_elems_in<T: Pun, V: Visit>(
  bytes: &[u8],
  at: usize,
  count: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let size = const { T::LAYOUT.size() };
  let Some(elems) =
    (size.checked_mul(count)).and_then(|len| bytes.get(at..).and_then(|rest| rest.get(..len)))
  else {
    return Err(Invalid::at(at).into());
  };
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  if size == 0 {
    // No bytes: one is as valid as any other.
    return if count == 0 { Ok(()) } else { validate_in::<T, V>(bytes, at, visit) };
  }
  if const { !T::LAYOUT.has_pointers() } {
    // SAFETY: `count` values of `T`, in bounds and initialized. `validate` reads them at any
    // alignment.
    let valid = unsafe { validate_elems::<T>(elems.as_ptr(), count) };
    if valid {
      return Ok(());
    }
  }
  let mut i = 0;
  while i < count {
    // SAFETY: the element `i`, in bounds (above).
    unsafe { value::<T, V>(bytes.as_ptr(), at + i * size, visit)? };
    i += 1;
  }
  Ok(())
}

/// Whatever the `count` values of `T` at `base` are valid: the slice validation of `validate`.
///
/// # Safety
/// `base` is valid for reads of `count` initialized `T`s, at any alignment.
#[inline(always)]
const unsafe fn validate_elems<T: Pun>(base: *const u8, count: usize) -> bool {
  // SAFETY: per the caller: a slice of `count` `T`s, which `validate` reads at any alignment.
  unsafe { validate::<All, Root, [T]>(ptr::from_raw_parts::<[T]>(base, count)) }
}

/// The `T` at the position `at` from `base`.
///
/// # Safety
/// `base + at` is valid for reads of the `size_of::<T>()` bytes of a `T`, all initialized (any
/// alignment).
#[inline(always)]
unsafe fn value<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  const {
    assert!(
      !(matches!(T::LAYOUT.kind, Kind::Enum(layout) if layout.union) && T::LAYOUT.has_pointers()),
      "a union that holds a self-relative pointer: which field it is cannot be told, so neither \
       can the pointer"
    );
  };
  // SAFETY: per the caller.
  let here = unsafe { base.add(at) };
  if const { !T::LAYOUT.has_pointers() } {
    // The code of `try_pun_ref`; where it fails, the walk below tells where.
    // SAFETY: per the caller; a sized `T` (`Sized` at the root, so are its parts).
    if unsafe { validate::<All, Root, T>(ptr::from_raw_parts::<T>(here, 0)) } {
      return Ok(());
    }
  }
  // SAFETY: per the caller; the layout of `T` tells what is at `here`.
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(spec) => {
        if spec.matches(here) {
          Ok(())
        } else {
          Err(Invalid::at(at).into())
        }
      }
      Shape::Array(len) => {
        let size = const { <T::Elem as Pun>::LAYOUT.size() };
        let mut i = 0;
        while i < len {
          value::<T::Elem, V>(base, at + i * size, visit)?;
          i += 1;
        }
        Ok(())
      }
      Shape::Struct(rules) => {
        fields::<T::Fields, V>(base, at, visit)?;
        if rules.is_empty() || check_rules::<T>(ptr::from_raw_parts::<T>(here, 0), rules) {
          Ok(())
        } else {
          Err(Invalid::at(at).into())
        }
      }
      // Not valid: `validate` said so above (a union holds no pointer, see below).
      Shape::Enum(layout) if layout.union => Err(Invalid::at(at).into()),
      Shape::Enum(layout) => enumeration::<T, V>(base, at, layout, visit),
      // All zeros is no pointer: valid if it may be null, else not (as `validate` says).
      Shape::Pointer(pointer) => {
        match (load(here, pointer.width, pointer.endian), pointer.nullable) {
          (0, true) => Ok(()),
          (0, false) => Err(Invalid::at(at).into()),
          _ => visit.pointer::<T>(at),
        }
      }
      // A sized value holds neither (the root is `Sized`, and so are its parts).
      Shape::Slice | Shape::Pattern(_) => Err(Invalid::at(at).into()),
    }
  }
}

/// The field `F` of the struct (or of the variant) at `at`, and the ones after it, in the order
/// of their declarations.
///
/// # Safety
/// As [`value`], for the struct (or the enum) that `F` is a field of.
#[inline(always)]
unsafe fn fields<F: Fields, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  let place = at + field.offset();
  // SAFETY: per the caller; the field is where its layout says.
  unsafe {
    match field.restriction() {
      // A scalar that accepts less than its type does.
      Some(spec) => {
        if !spec.matches(base.add(place)) {
          return Err(Invalid::at(place).into());
        }
      }
      None => value::<F::Type, V>(base, place, visit)?,
    }
    fields::<F::Next, V>(base, at, visit)
  }
}

/// The enum at `at`: its tag, and the fields of the variant that it tells.
///
/// # Safety
/// As [`value`]; `layout` is the one of `T`.
#[inline(always)]
unsafe fn enumeration<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  layout: &Enum,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { return Err(Invalid::at(at).into()) };
  let place = at + layout.tag.offset;
  // SAFETY: the tag is in the bytes of the enum.
  let tag = unsafe { load(base.add(place), spec.size, spec.endian) };
  // SAFETY: per the caller.
  unsafe { variants::<T::Variants, V>(base, at, tag, visit) }
    .unwrap_or_else(|| Err(Invalid::at(place).into()))
}

/// The fields of the variant, among `W` and the ones after it, whose tag is `tag`: `None` if there
/// is none.
///
/// # Safety
/// As [`value`], for the enum at `at`.
#[inline(always)]
unsafe fn variants<W: Variants, V: Visit>(
  base: *const u8,
  at: usize,
  tag: u128,
  visit: &mut V,
) -> Option<Result<(), V::Error>> {
  let expected = const { W::TAG }?;
  // The variant of a niche, or one that no value is (no range), has ranges of tags.
  let this = (const { W::RANGES }).map_or(tag == expected, |ranges| layout::contains(ranges, tag));
  // SAFETY: per the caller.
  unsafe {
    if this {
      Some(fields::<W::Fields, V>(base, at, visit))
    } else {
      variants::<W::Next, V>(base, at, tag, visit)
    }
  }
}
