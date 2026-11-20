//! The implementations of `Pun` for `Option<T>` and `Result<T, E>`, for any `T` and `E`.
//!
//! Both are enums without `repr`. Their layouts are found as the layout of an enum without `repr`
//! that derives `Pun` is found (`discover.rs`). Where `core` documents the layout, as for an
//! `Option` of a `NonZero` (`Pun::NULL_OPTIMIZED`), it is a layout that Rust specifies.
//! Otherwise it is a layout that Rust leaves to the compiler.
//!
//! If `T::OPTION` gives the layout of `Option<T>`, `Option<T>` has that layout. This is the case
//! of a self-relative pointer, whose `Option` [`self_relative!`](crate::self_relative) makes the
//! same pointer, which may be null. Nothing is found then, and the lists of the variants of
//! `Option<T>` are empty, since a pointer is not looked into.

use core::{marker::PhantomData, mem::MaybeUninit};

use crate::{
  At, Fields, Path, Pun, Variants,
  layout::{Desc, Enum, Field, Id, Layout, Variant},
};

/// Returns the offset of `field` in `whole`.
///
/// `field` must be a field of the value in `whole`, or the result is meaningless.
const fn offset<F, E>(field: &F, whole: &MaybeUninit<E>) -> usize {
  // SAFETY: `field` is in `whole`, after its start: the same allocation.
  unsafe { core::ptr::from_ref(field).byte_offset_from(whole.as_ptr()) }.cast_unsigned()
}

/// What the layout of `Option` and `Result` is found from, and what is found.
///
/// The constants are those that the derive writes for an enum without `repr`.
trait Found: Sized {
  /// Whether the layout is given by `Pun::OPTION` rather than found.
  const GIVEN: bool;
  /// A sample of each of the two variants. It is an uninitialized value for a variant without
  /// values, and for `Some` if the layout is given.
  const SAMPLES: [MaybeUninit<Self>; 2];
  /// The offset of the field of each variant, or 0 if it has none.
  const OFFSETS: [usize; 2];
  /// What the experiments on the samples find.
  const DESC: Desc<2>;
  /// The layout of the tag.
  const TAG: Layout;
  /// The layouts of the two variants.
  const VARIANTS: [Variant; 2];
  /// The layout of the enum.
  const ENUM: Enum;
}

/// Makes the layout of the `variant`th variant of `E`.
///
/// With a `field` type, the variant has one field of that type, at the offset that `E::OFFSETS`
/// gives, unless the variant has no value. Without one, the variant has no field.
macro_rules! variant {
  ($e:ty, $variant:literal) => {
    Variant::discovered(
      $variant,
      const { &<$e as Found>::DESC }.variant_ranges($variant),
      <$e as Found>::DESC.holds_tag($variant),
      const { &Layout::variant::<$e>(&[]) },
    )
  };
  ($e:ty, $variant:literal, $field:ty) => {
    Variant::discovered(
      $variant,
      const { &<$e as Found>::DESC }.variant_ranges($variant),
      <$e as Found>::DESC.holds_tag($variant),
      const {
        &Layout::variant::<$e>({
          let fields: &'static [Field] = if <$e as Found>::DESC.inhabited($variant) {
            const {
              &Layout::sort_fields([Field::new(
                <$e as Found>::OFFSETS[$variant],
                &<$field as Pun>::LAYOUT,
              )])
            }
          } else {
            &[]
          };
          fields
        })
      },
    )
  };
}

impl<T: Pun> Found for Option<T> {
  const GIVEN: bool = T::OPTION.is_some();
  const SAMPLES: [MaybeUninit<Self>; 2] = if Self::GIVEN || !T::LAYOUT.is_inhabited() {
    [Layout::wrap(None), Layout::uninit()]
  } else {
    // SAFETY: a valid `T`, made by its layout.
    [Layout::wrap(None), Layout::wrap(Some(unsafe { Layout::sample::<T>().assume_init() }))]
  };
  const OFFSETS: [usize; 2] = if Self::GIVEN || !T::LAYOUT.is_inhabited() {
    [0, 0]
  } else {
    let sample = &<Self as Found>::SAMPLES[1];
    // SAFETY: the sample of `Some`, a valid value.
    match unsafe { &*sample.as_ptr() } {
      Some(value) => [0, offset(value, sample)],
      // Not the variant: `Layout::discover` finds that the sample is not.
      None => [0, 0],
    }
  };
  const DESC: Desc<2> = if Self::GIVEN {
    Desc::none()
  } else {
    Layout::discover::<Self, 2>(
      &<Self as Found>::SAMPLES,
      &[&[], &[Field::new(<Self as Found>::OFFSETS[1], &T::LAYOUT)]],
    )
  };
  const TAG: Layout = const { &<Self as Found>::DESC }.tag();
  const VARIANTS: [Variant; 2] = [variant!(Self, 0), variant!(Self, 1, T)];
  const ENUM: Enum = Enum::discovered(
    const { &<Self as Found>::TAG },
    <Self as Found>::DESC.tag_offset(),
    const { &<Self as Found>::VARIANTS },
    Layout::size_align::<Self>(),
  );
}

impl<T: Pun, E: Pun> Found for Result<T, E> {
  const GIVEN: bool = false;
  const SAMPLES: [MaybeUninit<Self>; 2] = [
    if T::LAYOUT.is_inhabited() {
      // SAFETY: a valid `T`, made by its layout.
      Layout::wrap(Ok(unsafe { Layout::sample::<T>().assume_init() }))
    } else {
      Layout::uninit()
    },
    if E::LAYOUT.is_inhabited() {
      // SAFETY: a valid `E`, made by its layout.
      Layout::wrap(Err(unsafe { Layout::sample::<E>().assume_init() }))
    } else {
      Layout::uninit()
    },
  ];
  const OFFSETS: [usize; 2] = {
    let (ok, err) = (&<Self as Found>::SAMPLES[0], &<Self as Found>::SAMPLES[1]);
    let at = if T::LAYOUT.is_inhabited() {
      // SAFETY: the sample of `Ok`, a valid value.
      match unsafe { &*ok.as_ptr() } {
        Ok(value) => offset(value, ok),
        // Not the variant: `Layout::discover` finds that the sample is not.
        Err(_) => 0,
      }
    } else {
      0
    };
    let other = if E::LAYOUT.is_inhabited() {
      // SAFETY: the sample of `Err`, a valid value.
      match unsafe { &*err.as_ptr() } {
        Err(value) => offset(value, err),
        Ok(_) => 0,
      }
    } else {
      0
    };
    [at, other]
  };
  const DESC: Desc<2> = Layout::discover::<Self, 2>(
    &<Self as Found>::SAMPLES,
    &[
      &[Field::new(<Self as Found>::OFFSETS[0], &T::LAYOUT)],
      &[Field::new(<Self as Found>::OFFSETS[1], &E::LAYOUT)],
    ],
  );
  const TAG: Layout = const { &<Self as Found>::DESC }.tag();
  const VARIANTS: [Variant; 2] = [variant!(Self, 0, T), variant!(Self, 1, E)];
  const ENUM: Enum = Enum::discovered(
    const { &<Self as Found>::TAG },
    <Self as Found>::DESC.tag_offset(),
    const { &<Self as Found>::VARIANTS },
    Layout::size_align::<Self>(),
  );
}

// SAFETY: the layout that `rustc` gave `Option<T>`, found by experiments that fail to compile
// rather than find a wrong one (see `discover.rs`); or, if `T` gives it (`Pun::OPTION`), that
// one, which the implementation of `Pun` for `T` promises.
unsafe impl<T: Pun> Pun for Option<T> {
  type Elem = ();
  type Fields = ();
  type Variants = OptionNone<T>;
  type Rules = ();
  // What a pointer that may be null aims at, when `T` is a pointer.
  type Pointee = T::Pointee;
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = if let Some(layout) = T::OPTION {
    // The declarer of the pointer promises that it is a `NonZero` (see `SelfRelative`).
    layout
  } else {
    let layout = Layout::enumeration::<Self>(const { &<Self as Found>::ENUM })
      .identified(Id::new("Option", &[T::LAYOUT.id]));
    // Where `core` documents it (a `NonZero`, a `repr(transparent)` struct around one), what is
    // found is what it documents: a niche at zero, which is `None`. Else Rust specifies nothing
    // of it (section 12 of the design).
    if T::NULL_OPTIMIZED { layout } else { layout.unspecified() }
  };
}

// SAFETY: the layout that `rustc` gave `Result<T, E>`, found by experiments (`discover.rs`).
unsafe impl<T: Pun, E: Pun> Pun for Result<T, E> {
  type Elem = ();
  type Fields = ();
  type Variants = ResultOk<T, E>;
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = {
    let layout = Layout::enumeration::<Self>(const { &<Self as Found>::ENUM })
      .identified(Id::new("Result", &[T::LAYOUT.id, E::LAYOUT.id]));
    // Where `core` documents it: one of `T` and `E` of the `Option` that it documents, the
    // other of size 0 and alignment 1. What is found is then what it documents: the variant of
    // the `NonZero` is its value, and the other one, a single value, the only bytes that are not
    // one: zeros.
    if T::NULL_OPTIMIZED && is_1_zst::<E>() || E::NULL_OPTIMIZED && is_1_zst::<T>() {
      layout
    } else {
      layout.unspecified()
    }
  };
}

/// Returns whether `T` has size 0 and alignment 1 (a "1-ZST").
const fn is_1_zst<T>() -> bool {
  size_of::<T>() == 0 && align_of::<T>() == 1
}

/// The variant `None` of `Option<T>` in the list of its variants (`Pun::Variants`).
#[doc(hidden)]
pub struct OptionNone<T>(PhantomData<T>);
/// The variant `Some` of `Option<T>` in the list of its variants.
#[doc(hidden)]
pub struct OptionSome<T>(PhantomData<T>);
/// The field of `Some` in the list of its fields (`Variants::Fields`).
#[doc(hidden)]
pub struct OptionSomeField<T>(PhantomData<T>);
/// The variant `Ok` of `Result<T, E>` in the list of its variants (`Pun::Variants`).
#[doc(hidden)]
pub struct ResultOk<T, E>(PhantomData<(T, E)>);
/// The variant `Err` of `Result<T, E>` in the list of its variants.
#[doc(hidden)]
pub struct ResultErr<T, E>(PhantomData<(T, E)>);
/// The field of `Ok` in the list of its fields (`Variants::Fields`).
#[doc(hidden)]
pub struct ResultOkField<T, E>(PhantomData<(T, E)>);
/// The field of `Err` in the list of its fields.
#[doc(hidden)]
pub struct ResultErrField<T, E>(PhantomData<(T, E)>);

/// Returns the tag of the `index`th variant of `E`, or `None` if the layout of `E` is given.
///
/// In a list of variants, `None` ends the list: a given layout lists no variant.
const fn tag<E: Pun + Found>(index: usize) -> Option<u128> {
  if E::GIVEN { None } else { Some(E::LAYOUT.variant_tag(index)) }
}

/// Returns the values of the tag of the `index`th variant of `E`, or `None` if they are its tag
/// alone or if the layout of `E` is given.
const fn ranges<E: Pun + Found>(index: usize) -> Option<&'static [(u128, u128)]> {
  if E::GIVEN { None } else { E::LAYOUT.variant_ranges(index) }
}

/// Returns the field of the `index`th variant of `E`, or `None` if the variant has no value or
/// if the layout of `E` is given.
const fn field<E: Pun + Found>(index: usize) -> Option<Field> {
  if E::GIVEN || !E::DESC.inhabited(index) {
    None
  } else {
    Some(*E::LAYOUT.variant_field(index, 0))
  }
}

// SAFETY: the variants of the layout of `Option<T>`, if it is found.
unsafe impl<T: Pun> Variants for OptionNone<T> {
  type Fields = ();
  type Next = OptionSome<T>;
  const TAG: Option<u128> = tag::<Option<T>>(0);
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Option<T>>(0);
}

// SAFETY: as above.
unsafe impl<T: Pun> Variants for OptionSome<T> {
  type Fields = OptionSomeField<T>;
  type Next = ();
  const TAG: Option<u128> = tag::<Option<T>>(1);
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Option<T>>(1);
}

// SAFETY: the field of `Some` in the layout, if it has values.
unsafe impl<T: Pun> Fields for OptionSomeField<T> {
  type Type = T;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Option<T>>(1);
}

// SAFETY: the variants of the layout of `Result<T, E>`.
unsafe impl<T: Pun, E: Pun> Variants for ResultOk<T, E> {
  type Fields = ResultOkField<T, E>;
  type Next = ResultErr<T, E>;
  const TAG: Option<u128> = tag::<Result<T, E>>(0);
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Result<T, E>>(0);
}

// SAFETY: as above.
unsafe impl<T: Pun, E: Pun> Variants for ResultErr<T, E> {
  type Fields = ResultErrField<T, E>;
  type Next = ();
  const TAG: Option<u128> = tag::<Result<T, E>>(1);
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Result<T, E>>(1);
}

// SAFETY: the field of `Ok` in the layout, if it has values.
unsafe impl<T: Pun, E: Pun> Fields for ResultOkField<T, E> {
  type Type = T;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Result<T, E>>(0);
}

// SAFETY: the field of `Err` in the layout, if it has values.
unsafe impl<T: Pun, E: Pun> Fields for ResultErrField<T, E> {
  type Type = E;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Result<T, E>>(1);
}
