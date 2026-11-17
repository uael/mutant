//! Value-level description of the memory layout and of the valid bit patterns of a type.
//!
//! Every [`Pun`](crate::Pun) type exposes a `const LAYOUT: Layout`: a small tree, built at
//! compile time, that tells for every byte of the type whether it is padding, an unconstrained
//! initialized byte, or part of a *scalar* with a restricted set of valid values.
//!
//! The whole static analysis of the crate is ordinary `const fn` code running over those values
//! (see `analysis.rs`); nothing is encoded in the type system.

use core::mem;

use crate::ptr;

// -------------------------------------------------------------------------------------------------
// `Endian`

/// Byte order of a scalar.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endian {
  Little,
  Big,
}

impl Endian {
  /// Byte order of the target.
  pub const NATIVE: Endian =
    if cfg!(target_endian = "little") { Endian::Little } else { Endian::Big };

  #[inline]
  pub(crate) const fn is_little(self) -> bool {
    matches!(self, Endian::Little)
  }

  #[inline]
  pub(crate) const fn equals(self, other: Endian) -> bool {
    self.is_little() == other.is_little()
  }
}

// -------------------------------------------------------------------------------------------------
// `Spec`

/// Values a scalar is allowed to take.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Valid {
  /// Anything, including uninitialized bytes.
  Uninit,
  /// Any initialized value.
  Any,
  /// Sorted, disjoint and non-adjacent inclusive ranges. Never empty, never the full range.
  In(&'static [(u128, u128)]),
}

/// Specification of a scalar: its size, its byte order and the values it accepts.
#[derive(Copy, Clone, Debug)]
pub struct Spec {
  pub(crate) size: usize,
  pub(crate) endian: Endian,
  /// What a value must be to be accepted: what is required of a target.
  pub(crate) valid: Valid,
  /// What a value is known to be, whatever safe code does: what a source guarantees.
  ///
  /// This is `valid`, but for fields that are restricted by `#[pun(valid(..))]`: nothing
  /// prevents safe code from writing any value of the type of the field to it.
  pub(crate) known: Valid,
}

/// Greatest value of an unsigned integer of `size` bytes.
#[inline]
pub(crate) const fn max_value(size: usize) -> u128 {
  if size >= 16 { u128::MAX } else { (1u128 << (8 * size)) - 1 }
}

impl Spec {
  /// A scalar the size of `T` that accepts every initialized bit pattern.
  #[inline]
  #[must_use]
  pub const fn any<T>() -> Self {
    Self { size: size_of::<T>(), endian: Endian::NATIVE, valid: Valid::Any, known: Valid::Any }
  }

  /// A scalar the size of `T` that accepts everything, uninitialized bytes included.
  #[inline]
  #[must_use]
  pub const fn uninit<T>() -> Self {
    let (size, endian) = (size_of::<T>(), Endian::NATIVE);
    Self { size, endian, valid: Valid::Uninit, known: Valid::Uninit }
  }

  /// A scalar the size of `T` whose value, read as an unsigned integer in `endian` byte order,
  /// must be in one of `ranges`.
  ///
  /// `ranges` are inclusive and must be sorted, disjoint and non-adjacent; this is checked at
  /// compile time. See [`sorted_ranges`] to build them from a list of values.
  #[must_use]
  pub const fn ranges<T>(endian: Endian, ranges: &'static [(u128, u128)]) -> Self {
    Self { size: size_of::<T>(), endian, valid: Valid::Any, known: Valid::Any }.with_ranges(ranges)
  }

  /// Same as `self`, with other valid `ranges`, see [`Spec::ranges`].
  ///
  /// # Panics
  /// If the scalar is not 1 to 16 bytes wide, or if `ranges` is empty, not sorted, overlapping,
  /// adjacent or out of the bounds of the scalar.
  #[must_use]
  pub const fn with_ranges(self, ranges: &'static [(u128, u128)]) -> Self {
    let (size, endian) = (self.size, self.endian);
    assert!(size >= 1 && size <= 16, "a constrained scalar must be 1 to 16 bytes wide");
    assert!(!ranges.is_empty(), "a scalar must have at least one valid value");
    let max = max_value(size);
    let mut i = 0;
    while i < ranges.len() {
      let (lo, hi) = ranges[i];
      assert!(lo <= hi && hi <= max, "scalar range is empty or out of bounds");
      if i > 0 {
        let prev = ranges[i - 1].1;
        assert!(prev < lo && lo - prev > 1, "scalar ranges must be sorted and non-adjacent");
      }
      i += 1;
    }
    let full = ranges.len() == 1 && ranges[0].0 == 0 && ranges[0].1 == max;
    let valid = if full { Valid::Any } else { Valid::In(ranges) };
    Self { size, endian, valid, known: valid }
  }

  /// Whatever `T` has no size, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_zero_sized<T>() -> bool {
    size_of::<T>() == 0
  }

  /// [`Spec::ranges`], in the byte order of the target.
  #[doc(hidden)]
  #[must_use]
  pub const fn native_ranges<T>(ranges: &'static [(u128, u128)]) -> Self {
    Self::ranges::<T>(Endian::NATIVE, ranges)
  }

  /// [`sorted_ranges`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sorted_ranges<const N: usize>(values: [u128; N]) -> ([(u128, u128); N], usize) {
    sorted_ranges(values)
  }

  /// [`range`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn range<T>(start: Option<u128>, end: Option<u128>, inclusive: bool) -> (u128, u128) {
    range::<T>(start, end, inclusive)
  }

  /// [`merge_ranges`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn merge_ranges<const N: usize>(
    ranges: [(u128, u128); N],
  ) -> ([(u128, u128); N], usize) {
    merge_ranges(ranges)
  }

  /// [`bits_of`], for `#[derive(Pun)]`.
  ///
  /// # Safety
  /// Every byte of `value` must be initialized.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn bits_of<T>(value: &T) -> u128 {
    // SAFETY: guaranteed by the caller.
    unsafe { bits_of(value) }
  }

  /// Whatever the scalar is only restricted by a `#[pun(valid(..))]` that the type system does
  /// not enforce: it is then proven by restrictions of the same kind.
  #[inline]
  pub(crate) const fn is_soft(&self) -> bool {
    matches!(self.known, Valid::Any) && matches!(self.valid, Valid::In(_))
  }

  /// Same as `self`, read in another byte order.
  #[inline]
  #[must_use]
  pub const fn with_endian(self, endian: Endian) -> Self {
    Self { endian, ..self }
  }

  /// Whatever the scalar is an initialized integer of at most 16 bytes.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_integer(&self) -> bool {
    self.size <= 16 && !matches!(self.known, Valid::Uninit) && !matches!(self.valid, Valid::Uninit)
  }

  /// The scalar at `ptr`, as an unsigned integer.
  ///
  /// # Safety
  /// `self` must be [an integer](Self::is_integer), and `ptr` must be valid for reads of one.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const unsafe fn load(&self, ptr: *const u8) -> u128 {
    // SAFETY: guaranteed by the caller.
    unsafe { load(ptr, self.size, self.endian) }
  }

  /// Whatever the scalar restricts the values it accepts.
  #[inline]
  pub(crate) const fn is_constrained(&self) -> bool {
    matches!(self.valid, Valid::In(_))
  }

  /// Whatever `value` is valid for this scalar.
  #[must_use]
  pub const fn contains(&self, value: u128) -> bool {
    match self.valid {
      Valid::Uninit | Valid::Any => self.size >= 16 || value <= max_value(self.size),
      Valid::In(ranges) => contains(ranges, value),
    }
  }

  /// Whatever the scalar stored at `ptr` is valid.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `self.size` initialized bytes (no alignment requirement).
  #[inline(always)]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8) -> bool {
    match self.valid {
      Valid::Uninit | Valid::Any => true,
      // SAFETY: guaranteed by the caller.
      Valid::In(ranges) => contains(ranges, unsafe { load(ptr, self.size, self.endian) }),
    }
  }
}

#[inline(always)]
const fn contains(ranges: &[(u128, u128)], value: u128) -> bool {
  let mut i = 0;
  while i < ranges.len() {
    if ranges[i].0 <= value && value <= ranges[i].1 {
      return true;
    }
    i += 1;
  }
  false
}

/// Read an unsigned integer of `size <= 16` bytes.
///
/// # Safety
/// `ptr` must be valid for reads of `size` initialized bytes.
#[inline(always)]
pub(crate) const unsafe fn load(ptr: *const u8, size: usize, endian: Endian) -> u128 {
  let mut buf = [0u8; 16];
  // SAFETY: `size <= 16` and `ptr` is readable for `size` bytes.
  unsafe {
    match endian {
      Endian::Little => {
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), size);
        u128::from_le_bytes(buf)
      }
      Endian::Big => {
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr().add(16 - size), size);
        u128::from_be_bytes(buf)
      }
    }
  }
}

/// Sort `values` and merge them into inclusive ranges; returns the ranges and their count.
///
/// ```
/// use puns::layout::sorted_ranges;
///
/// assert_eq!(sorted_ranges([7, 1, 0, 2]), ([(0, 2), (7, 7), (0, 0), (0, 0)], 2));
/// ```
#[must_use]
pub const fn sorted_ranges<const N: usize>(mut values: [u128; N]) -> ([(u128, u128); N], usize) {
  // Insertion sort.
  let mut i = 1;
  while i < N {
    let mut j = i;
    while j > 0 && values[j - 1] > values[j] {
      values.swap(j - 1, j);
      j -= 1;
    }
    i += 1;
  }
  // Merge duplicates and neighbors.
  let (mut ranges, mut len, mut i) = ([(0, 0); N], 0, 0);
  while i < N {
    if len > 0 && values[i] - ranges[len - 1].1 <= 1 {
      ranges[len - 1].1 = values[i];
    } else {
      ranges[len] = (values[i], values[i]);
      len += 1;
    }
    i += 1;
  }
  (ranges, len)
}

// -------------------------------------------------------------------------------------------------
// `Layout`

/// What all the bytes of a type have in common, if anything.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Uniform {
  /// Nothing: the type must be looked into.
  No,
  /// Every byte is padding.
  Pad,
  /// Every byte is initialized and unconstrained.
  Any,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Kind {
  Scalar(Spec),
  Array(&'static Layout, usize),
  Slice(&'static Layout),
  Struct(&'static [Field], &'static [Rule]),
  /// A tail of bytes that a [`Dfa`] accepts.
  Pattern(&'static Dfa),
  /// A tag, and the fields of the variant that the tag tells.
  Enum(&'static Enum),
}

/// Layout of an enum with fields, see [`Layout::enumeration`].
#[derive(Debug)]
pub struct Enum {
  /// A scalar whose valid values are the tags of the variants.
  pub(crate) tag: Field,
  pub(crate) variants: &'static [Variant],
  /// Size and alignment that the representation of the enum specifies.
  pub(crate) specified: (usize, usize),
}

impl Enum {
  /// The enum that `repr(C)` and `repr(u8)` (or any other integer) specify: a tag, which is a
  /// scalar whose valid values are the tags of the `variants`, then one of the `shadows`, the
  /// `repr(C)` structs of the fields of each variant, of which the sizes and alignments are
  /// given.
  ///
  /// With `repr(C)`, the tag is `outside`: the fields of a variant are at the offset of the
  /// shadows (`fields`) plus their offset in the shadow of the variant. Without, the tag is the
  /// first field of each shadow.
  ///
  /// # Panics
  /// If `fields` is not where the fields of the variants are.
  #[must_use]
  pub const fn new(
    tag: &'static Layout,
    outside: bool,
    shadows: &[(usize, usize)],
    fields: usize,
    variants: &'static [Variant],
  ) -> Self {
    let (mut size, mut align, mut i) = (0, 1, 0);
    while i < shadows.len() {
      if shadows[i].0 > size {
        size = shadows[i].0;
      }
      if shadows[i].1 > align {
        align = shadows[i].1;
      }
      i += 1;
    }
    let specified = if outside {
      assert!(fields == Self::fields(tag, shadows), "not where the fields of the variants are");
      let align = if tag.align > align { tag.align } else { align };
      (tail_offset(fields + size, align, usize::MAX), align)
    } else {
      assert!(fields == 0, "not where the fields of the variants are");
      (tail_offset(size, align, usize::MAX), align)
    };
    Self { tag: Field::new(0, tag), variants, specified }
  }

  /// Offset of the fields of the variants of a `repr(C)` enum, whose tag is outside of the
  /// `shadows` (see [`Enum::new`]).
  #[must_use]
  pub const fn fields(tag: &Layout, shadows: &[(usize, usize)]) -> usize {
    let (mut align, mut i) = (1, 0);
    while i < shadows.len() {
      if shadows[i].1 > align {
        align = shadows[i].1;
      }
      i += 1;
    }
    tail_offset(tag.size, align, usize::MAX)
  }
}

/// A variant of an enum with fields, see [`Layout::enumeration`].
#[derive(Copy, Clone, Debug)]
pub struct Variant {
  /// Value of the tag.
  pub(crate) tag: u128,
  /// The enum when it is this variant: a struct of its fields, in which the tag is padding.
  pub(crate) layout: &'static Layout,
}

impl Variant {
  /// - `tag` is what the tag of the enum is for this variant, as an integer of native endianness,
  /// - `layout` is the [`Layout::variant`] of its fields.
  #[must_use]
  pub const fn new(tag: u128, layout: &'static Layout) -> Self {
    Self { tag, layout }
  }

  /// Fields, by offset from the start of the enum.
  pub(crate) const fn fields(&self) -> &'static [Field] {
    let Kind::Struct(fields, _) = self.layout.kind else { panic!("not a variant") };
    fields
  }
}

/// A field of a struct, see [`Layout::structure`].
#[derive(Copy, Clone, Debug)]
pub struct Field {
  pub(crate) offset: usize,
  pub(crate) layout: &'static Layout,
  /// Declaration index.
  pub(crate) index: usize,
  /// Number of constrained scalars declared before this field in the struct.
  pub(crate) leaf: usize,
  /// The layout is a scalar that accepts less than the type of the field does.
  pub(crate) restricted: bool,
}

impl Field {
  #[inline]
  #[must_use]
  pub const fn new(offset: usize, layout: &'static Layout) -> Self {
    Self { offset, layout, index: 0, leaf: 0, restricted: false }
  }

  /// A scalar field that only accepts some of the values of its type, see
  /// [`Layout::restricted`].
  ///
  /// # Panics
  /// If `layout` is not a scalar.
  #[inline]
  #[must_use]
  pub const fn restricted(offset: usize, layout: &'static Layout) -> Self {
    assert!(matches!(layout.kind, Kind::Scalar(_)), "only scalar fields can be restricted");
    Self { offset, layout, index: 0, leaf: 0, restricted: true }
  }

  /// The specification of a restricted field.
  #[doc(hidden)]
  #[must_use]
  pub const fn restriction(&self) -> Option<Spec> {
    match self.layout.kind {
      Kind::Scalar(spec) if self.restricted => Some(spec),
      _ => None,
    }
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn offset(&self) -> usize {
    self.offset
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn leaf(&self) -> usize {
    self.leaf
  }
}

/// Memory layout and valid bit patterns of a type.
#[derive(Copy, Clone, Debug)]
pub struct Layout {
  pub(crate) kind: Kind,
  /// Size of the type if it is `Sized`, otherwise the offset of the first element of its tail
  /// slice.
  pub(crate) size: usize,
  pub(crate) align: usize,
  /// Size of an element of the tail slice, `0` if the type is `Sized`.
  pub(crate) tail: usize,
  pub(crate) unsized_: bool,
  /// Number of things that validation may have to check: constrained scalars, patterns, and
  /// structs with rules (array and slice elements are counted once).
  pub(crate) leaves: usize,
  pub(crate) uniform: Uniform,
  /// Whatever a struct of the layout has rules.
  pub(crate) has_rules: bool,
}

/// What [`validate`](crate::validate) has to do for a type.
#[derive(Copy, Clone)]
pub(crate) enum Shape {
  Scalar(Spec),
  Array(usize),
  Slice,
  Struct(&'static [Rule]),
  Pattern(&'static Dfa),
  Enum(&'static Enum),
}

/// Round `value` up to a multiple of `align` (a power of two).
#[inline]
pub(crate) const fn round_up(value: usize, align: usize) -> Option<usize> {
  match value.checked_add(align - 1) {
    Some(v) => Some(v & !(align - 1)),
    None => None,
  }
}

/// Offset of the last field of a `repr(C)` struct.
///
/// - `end` is the end offset of the field that precedes it (`0` if there is none),
/// - `align` is the alignment of its type,
/// - `pack` is the `N` of `repr(packed(N))`, or `usize::MAX`.
#[doc(hidden)]
#[must_use]
pub const fn tail_offset(end: usize, align: usize, pack: usize) -> usize {
  match round_up(end, if align < pack { align } else { pack }) {
    Some(offset) => offset,
    None => panic!("struct is too large"),
  }
}

/// What [`Layout::PIN`] is made of.
fn pin<T>(_: T) {}

impl Layout {
  /// The types that the code that `#[derive(Pun)]` generates names by their path in the crate
  /// it is told of (`#[pun(crate = ..)]`), as they must be: the functions that it calls are
  /// associated functions of these types, or of `Layout` and `Spec`, which the traits that it
  /// implements name. A path that leads to other types, with functions of the same name, does
  /// not compile (`fn` types are not paths, nothing can stand for them).
  #[doc(hidden)]
  #[expect(
    clippy::type_complexity,
    reason = "the derive writes this tuple of `fn` types out, with the paths it is told"
  )]
  pub const PIN: (fn(Rule), fn(Op), fn(Field), fn(Enum), fn(Variant)) =
    (pin::<Rule>, pin::<Op>, pin::<Field>, pin::<Enum>, pin::<Variant>);

  /// An uninitialized `T`, whose fields `#[derive(Pun)]` finds the offsets of.
  #[doc(hidden)]
  #[must_use]
  pub const fn uninit<T>() -> mem::MaybeUninit<T> {
    mem::MaybeUninit::uninit()
  }

  /// The layout of a `repr(C)` struct whose fields have the given sizes and alignments, after
  /// what `start` (an end offset and an alignment) says is before them: the offsets of the
  /// fields, and the size and alignment of the struct. For the variants of enums with fields,
  /// whose layout RFC 2195 specifies in these terms.
  #[doc(hidden)]
  #[must_use]
  pub const fn repr_c<const N: usize>(
    start: (usize, usize),
    fields: [(usize, usize); N],
  ) -> ([usize; N], (usize, usize)) {
    let (mut offsets, (mut end, mut align), mut i) = ([0; N], start, 0);
    while i < N {
      offsets[i] = tail_offset(end, fields[i].1, usize::MAX);
      end = offsets[i] + fields[i].0;
      if fields[i].1 > align {
        align = fields[i].1;
      }
      i += 1;
    }
    (offsets, (tail_offset(end, align, usize::MAX), align))
  }

  /// Size and alignment of `T`, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn size_align<T>() -> (usize, usize) {
    (size_of::<T>(), align_of::<T>())
  }

  /// [`tail_offset`] in a struct that is not packed, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset(end: usize, align: usize) -> usize {
    tail_offset(end, align, usize::MAX)
  }

  /// [`tail_offset`] in a `repr(packed(pack))` struct, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset_packed(end: usize, align: usize, pack: usize) -> usize {
    tail_offset(end, align, pack)
  }

  /// Layout of a scalar.
  ///
  /// # Panics
  /// If `spec` does not have the size of `T`.
  #[must_use]
  pub const fn scalar<T>(spec: Spec) -> Self {
    assert!(spec.size == size_of::<T>(), "`Spec` size mismatch");
    Self {
      kind: Kind::Scalar(spec),
      size: spec.size,
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: spec.is_constrained() as usize,
      uniform: match spec.valid {
        Valid::Uninit => Uniform::Pad,
        Valid::Any => Uniform::Any,
        Valid::In(_) => Uniform::No,
      },
      has_rules: false,
    }
  }

  /// Same scalar, that only accepts the values of `ranges` (see [`Spec::ranges`]), which must
  /// all be valid for the scalar.
  ///
  /// # Panics
  /// If `self` is not an initialized scalar, or if `ranges` are not valid values of it (see
  /// [`Spec::with_ranges`]).
  #[must_use]
  pub const fn restricted(&self, ranges: &'static [(u128, u128)]) -> Self {
    let Kind::Scalar(spec) = self.kind else { panic!("only scalars can be restricted") };
    assert!(!matches!(spec.valid, Valid::Uninit), "only initialized scalars can be restricted");
    // The restriction is not something that the type of the field enforces.
    let new = Spec { known: spec.known, ..spec.with_ranges(ranges) };
    let mut i = 0;
    while i < ranges.len() {
      assert!(
        spec.contains(ranges[i].0) && spec.contains(ranges[i].1),
        "restricted to values that are not valid for the type"
      );
      if let Valid::In(old) = spec.valid {
        assert!(
          matches!(crate::analysis::relate_ranges(ranges, old), crate::analysis::Rel::Subset),
          "restricted to values that are not valid for the type"
        );
      }
      i += 1;
    }
    Self {
      kind: Kind::Scalar(new),
      leaves: new.is_constrained() as usize,
      uniform: if new.is_constrained() { Uniform::No } else { self.uniform },
      ..*self
    }
  }

  /// Layout of an unsized type made of bytes that `dfa` accepts, such as `str`.
  ///
  /// # Safety
  /// `probe` must be a pointer to a type whose pointers have a slice length, which is
  /// `PROBE_LEN` (see `Pun::PROBE`).
  ///
  /// # Panics
  /// If `probe` is not a pointer to bytes, or if `dfa` is not well formed.
  pub const unsafe fn pattern<T: ?Sized>(probe: *const T, dfa: &'static Dfa) -> Self {
    // SAFETY: the caller gives the probe of a type whose pointers have a slice length.
    unsafe { ptr::verify::<T>(probe) };
    // SAFETY: the metadata of `probe` is a small slice length.
    assert!(unsafe { mem::size_of_val_raw(probe) } == ptr::PROBE_LEN, "not a byte string");
    dfa.check();
    Self {
      kind: Kind::Pattern(dfa),
      size: 0,
      align: 1,
      tail: 1,
      unsized_: true,
      leaves: 1,
      uniform: Uniform::No,
      has_rules: false,
    }
  }

  /// Layout of a type the size and alignment of `T` whose bytes are all padding.
  #[must_use]
  pub const fn padding<T>() -> Self {
    Self {
      kind: Kind::Struct(&[], &[]),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 0,
      uniform: Uniform::Pad,
      has_rules: false,
    }
  }

  /// Layout of `[T; len]`.
  ///
  /// # Panics
  /// If `elem` is unsized, or if the array is larger than `usize::MAX` bytes.
  #[must_use]
  pub const fn array(elem: &'static Layout, len: usize) -> Self {
    assert!(!elem.unsized_);
    Self {
      kind: Kind::Array(elem, len),
      size: match elem.size.checked_mul(len) {
        Some(size) => size,
        None => panic!("array is too large"),
      },
      align: elem.align,
      tail: 0,
      unsized_: false,
      leaves: elem.leaves,
      uniform: elem.uniform,
      has_rules: elem.has_rules,
    }
  }

  /// Layout of `[T]`.
  ///
  /// # Safety
  /// The length of `probe` must be `PROBE_LEN` (see `Pun::PROBE`).
  ///
  /// # Panics
  /// If `elem` is unsized or does not have the size of `T`.
  pub const unsafe fn slice<T>(probe: *const [T], elem: &'static Layout) -> Self {
    assert!(!elem.unsized_ && elem.size == size_of::<T>());
    // SAFETY: `[T]` is a slice.
    unsafe { ptr::verify::<[T]>(probe) };
    Self {
      kind: Kind::Slice(elem),
      size: 0,
      align: elem.align,
      tail: elem.size,
      unsized_: true,
      leaves: elem.leaves,
      uniform: elem.uniform,
      has_rules: elem.has_rules,
    }
  }

  /// Layout of the struct `T`.
  ///
  /// - `probe` is `T::PROBE`,
  /// - `fields` are the fields of `T` as returned by [`Layout::sort_fields`],
  /// - `rules` are what values of `T` must satisfy beyond the validity of their fields.
  ///
  /// The size and alignment of `T` come from the compiler. If the last field is unsized, so is
  /// `T`, and the way the compiler sizes `T` is checked against the model used by this crate.
  ///
  /// # Safety
  /// If `T` is unsized, its tail must be a slice, and the slice length of `probe` must be
  /// `PROBE_LEN` (see `Pun::PROBE`).
  ///
  /// # Panics
  /// If the fields overlap, are not sorted or are out of the bounds of `T`, if a rule is
  /// malformed, or if `T` is not laid out as this crate models it.
  pub const unsafe fn structure<T: ?Sized>(
    probe: *const T,
    fields: &'static [Field],
    rules: &'static [Rule],
  ) -> Self {
    let (mut i, mut end, mut leaves) = (0, 0, (!rules.is_empty()) as usize);
    let mut has_rules = !rules.is_empty();
    let (mut all_any, mut all_pad) = (true, true);
    let mut tail: Option<&Field> = None;
    while i < fields.len() {
      let field = &fields[i];
      assert!(tail.is_none(), "only the last field of a struct can be unsized");
      leaves += field.layout.leaves;
      has_rules |= field.layout.has_rules;
      if field.layout.unsized_ {
        assert!(field.offset >= end, "fields must not overlap");
        tail = Some(field);
      } else if field.layout.size > 0 {
        assert!(field.offset >= end, "fields must be sorted and must not overlap");
        all_any &= field.offset == end && matches!(field.layout.uniform, Uniform::Any);
        all_pad &= matches!(field.layout.uniform, Uniform::Pad);
        end = field.offset + field.layout.size;
      }
      i += 1;
    }
    match tail {
      None => {
        // SAFETY: `T` is `Sized` (none of its fields is unsized): `probe` is not even looked at.
        let (size, align) = unsafe { (mem::size_of_val_raw(probe), mem::align_of_val_raw(probe)) };
        assert!(end <= size, "field out of bounds");
        let uniform = match (all_any && end == size, all_pad) {
          _ if size == 0 => Uniform::Any,
          (true, _) => Uniform::Any,
          (_, true) => Uniform::Pad,
          _ => Uniform::No,
        };
        check_rules(rules, end);
        let this = Self {
          kind: Kind::Struct(fields, rules),
          size,
          align,
          tail: 0,
          unsized_: false,
          leaves,
          uniform,
          has_rules,
        };
        let mut i = 0;
        while i < rules.len() {
          assert!(
            !matches!(rules[i], Rule::Len(_)) || this.tail_shape().array,
            "`len` is about a last field that is a slice, or an array"
          );
          i += 1;
        }
        this
      }
      Some(field) => {
        // SAFETY: the tail of `T` is a slice (the tail of its last field is).
        unsafe { ptr::verify::<T>(probe) };
        // SAFETY: the metadata of `probe` is a small slice length.
        let align = unsafe { mem::align_of_val_raw(probe) };
        check_rules(rules, end);
        let this = Self {
          kind: Kind::Struct(fields, rules),
          size: field.offset + field.layout.size,
          align,
          tail: field.layout.tail,
          unsized_: true,
          leaves,
          uniform: Uniform::No,
          has_rules,
        };
        // Check the size model against the compiler.
        let mut len = 0;
        while len <= 8 {
          // SAFETY: `T` has a slice tail of `len` elements, and its size is tiny.
          let size = unsafe { mem::size_of_val_raw(ptr::raw_with_len(probe, len)) };
          assert!(
            matches!(this.size_of_val(len), Some(s) if s == size),
            "unsupported layout: the trailing padding of this unsized struct is not the one of \
             its unsized field (packed struct with an aligned unsized field?)",
          );
          len += 1;
        }
        this
      }
    }
  }

  /// Layout of a variant of the enum `T`: its `fields`, as returned by [`Layout::sort_fields`],
  /// with their offsets from the start of the enum.
  #[must_use]
  pub const fn variant<T>(fields: &'static [Field]) -> Self {
    let (mut i, mut leaves, mut has_rules) = (0, 0, false);
    while i < fields.len() {
      leaves += fields[i].layout.leaves;
      has_rules |= fields[i].layout.has_rules;
      i += 1;
    }
    Self {
      kind: Kind::Struct(fields, &[]),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves,
      uniform: Uniform::No,
      has_rules,
    }
  }

  /// Layout of the enum with fields `T`, whose representation must be one that Rust specifies
  /// (`repr(C)`, `repr(u8)`...): a tag at `tag.offset`, then the fields of a variant.
  ///
  /// What is known of such an enum is its tag, and which of its bytes are initialized; what it
  /// requires is checked as a whole, at run time, unless the source is an enum whose variants
  /// each prove the variant of the same tag.
  ///
  /// # Panics
  /// If `T` is not laid out as its representation specifies, or if the variants do not fit it:
  /// a tag that is not a value of the tag or that two variants share, a field out of bounds.
  #[must_use]
  pub const fn enumeration<T>(layout: &'static Enum) -> Self {
    let (size, tag) = (size_of::<T>(), &layout.tag);
    let Kind::Scalar(spec) = tag.layout.kind else { panic!("the tag of an enum is a scalar") };
    // An `align(N)` on the enum is the only thing that the shadows do not tell.
    assert!(
      align_of::<T>() >= layout.specified.1
        && size == tail_offset(layout.specified.0, align_of::<T>(), usize::MAX),
      "the enum is not laid out as its representation specifies"
    );
    assert!(spec.is_integer() && tag.offset + spec.size <= size, "tag out of bounds");
    let mut i = 0;
    while i < layout.variants.len() {
      let variant = &layout.variants[i];
      assert!(spec.contains(variant.tag), "not a value of the tag");
      let (fields, mut j, mut end) = (variant.fields(), 0, 0);
      assert!(variant.layout.size == size, "not a variant of this enum");
      while j < fields.len() {
        let field = &fields[j];
        assert!(!field.layout.unsized_, "the fields of an enum are sized");
        if field.layout.size > 0 {
          let field_end = field.offset + field.layout.size;
          assert!(field.offset >= end && field_end <= size, "fields overlap or are out of bounds");
          assert!(
            field_end <= tag.offset || field.offset >= tag.offset + spec.size,
            "a field overlaps the tag"
          );
          end = field_end;
        }
        j += 1;
      }
      let mut j = 0;
      while j < i {
        assert!(layout.variants[j].tag != variant.tag, "two variants have the same tag");
        j += 1;
      }
      i += 1;
    }
    Self {
      kind: Kind::Enum(layout),
      size,
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 1,
      uniform: Uniform::No,
      // What the fields of the variants require is a matter of the enum as a whole.
      has_rules: false,
    }
  }

  /// The `index`th operation of the expression of the `rule`th rule, if any.
  pub(crate) const fn rule_op(&self, rule: usize, index: usize) -> Option<Op> {
    let rules = self.rules();
    if rule >= rules.len() {
      return None;
    }
    match rules[rule].ops() {
      Some(ops) if index < ops.len() => Some(ops[index]),
      _ => None,
    }
  }

  /// Index of the [`Op::Walk`] of the expression of the `rule`th rule, `usize::MAX` if none.
  pub(crate) const fn walk_at(&self, rule: usize) -> usize {
    let mut i = 0;
    while let Some(op) = self.rule_op(rule, i) {
      if matches!(op, Op::Walk { .. }) {
        return i;
      }
      i += 1;
    }
    usize::MAX
  }

  /// The `index`th operation of an expression (the condition, or with `next` what follows) of
  /// the [`Op::Walk`] of the `rule`th rule.
  pub(crate) const fn walk_op(&self, rule: usize, next: bool, index: usize) -> Option<Op> {
    let Some(Op::Walk { cond, next: then, .. }) = self.rule_op(rule, self.walk_at(rule)) else {
      return None;
    };
    let ops = if next { then } else { cond };
    if index < ops.len() { Some(ops[index]) } else { None }
  }

  /// Whatever the `rule`th rule is a [`Rule::Walk`] whose expression has `ops` operations, and
  /// whose walk has expressions of at most `inner` operations.
  pub(crate) const fn walks(&self, rule: usize, ops: usize, inner: usize) -> bool {
    let rules = self.rules();
    if rule >= rules.len() {
      return false;
    }
    let Rule::Walk(expression) = rules[rule] else { return false };
    let Some(Op::Walk { cond, next, .. }) = self.rule_op(rule, self.walk_at(rule)) else {
      return false;
    };
    expression.len() == ops && cond.len() <= inner && next.len() <= inner
  }

  /// Number of rules that are a [`Rule::Walk`].
  pub(crate) const fn walk_rules(&self) -> usize {
    let (rules, mut count, mut i) = (self.rules(), 0, 0);
    while i < rules.len() {
      count += matches!(rules[i], Rule::Walk(_)) as usize;
      i += 1;
    }
    count
  }

  /// The tag of the `variant`th variant of an enum.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_tag(&self, variant: usize) -> u128 {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    layout.variants[variant].tag
  }

  /// The `index`th field, as declared, of the `variant`th variant of an enum.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_field(&self, variant: usize, index: usize) -> &Field {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    let (fields, mut i) = (layout.variants[variant].fields(), 0);
    while i < fields.len() {
      if fields[i].index == index {
        return &fields[i];
      }
      i += 1;
    }
    panic!("no such field")
  }

  /// Whatever the bytes from `offset` are initialized, and where bytes that may be otherwise
  /// start: in every value of the layout (`always`), or in some.
  ///
  /// This does not look into arrays whose elements are not all one or the other: their bytes
  /// are not known to be initialized (`always`), or are all required to be (not `always`).
  ///
  /// With `checked` (and not `always`), the padding of a struct whose rules read the bytes of its
  /// value is required to be initialized, as these rules may be checked.
  pub(crate) const fn segment(&self, offset: usize, always: bool, checked: bool) -> (bool, usize) {
    if checked && !always && self.rules_read_bytes() {
      return (true, self.size);
    }
    match self.uniform {
      Uniform::Any => return (true, self.size),
      Uniform::Pad => return (false, self.size),
      Uniform::No => {}
    }
    match self.kind {
      Kind::Scalar(spec) => (!matches!(spec.valid, Valid::Uninit), self.size),
      Kind::Pattern(_) | Kind::Slice(_) => panic!("not sized"),
      Kind::Array(..) => (!always, self.size),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          let field = &fields[i];
          if offset < field.offset {
            // Padding before the field.
            return (false, field.offset);
          }
          if offset < field.offset + field.layout.size {
            let (init, end) = field.layout.segment(offset - field.offset, always, checked);
            return (init, field.offset + end);
          }
          i += 1;
        }
        (false, self.size)
      }
      Kind::Enum(layout) => {
        let (start, end) = (layout.tag.offset, layout.tag.offset + layout.tag.layout.size);
        if offset >= start && offset < end {
          return (true, end);
        }
        // The same in every variant up to `stop`.
        let (mut i, mut every, mut some) = (0, true, false);
        let mut stop = if offset < start { start } else { self.size };
        while i < layout.variants.len() {
          let (init, end) = layout.variants[i].layout.segment(offset, always, checked);
          (every, some) = (every && init, some || init);
          if end < stop {
            stop = end;
          }
          i += 1;
        }
        (if always { every } else { some }, stop)
      }
    }
  }

  /// Number the fields of a struct (given in declaration order) and sort them by offset.
  #[must_use]
  pub const fn sort_fields<const N: usize>(mut fields: [Field; N]) -> [Field; N] {
    let (mut i, mut leaf) = (0, 0);
    while i < N {
      (fields[i].index, fields[i].leaf) = (i, leaf);
      leaf += fields[i].layout.leaves;
      i += 1;
    }
    // Stable insertion sort.
    let mut i = 1;
    while i < N {
      let mut j = i;
      while j > 0 && fields[j - 1].offset > fields[j].offset {
        fields.swap(j - 1, j);
        j -= 1;
      }
      i += 1;
    }
    fields
  }

  /// Size and alignment of the type.
  #[inline]
  #[must_use]
  pub const fn align(&self) -> usize {
    self.align
  }

  /// Size of the type if it is `Sized`, otherwise the offset of its tail slice.
  #[inline]
  #[must_use]
  pub const fn size(&self) -> usize {
    self.size
  }

  #[inline]
  pub(crate) const fn is_unsized(&self) -> bool {
    self.unsized_
  }

  /// Whatever the size of values of the type depends on a slice length.
  #[inline]
  pub(crate) const fn is_dynamic(&self) -> bool {
    self.unsized_ && self.tail > 0
  }

  /// Size of an element of the tail slice if the type [is dynamic](Self::is_dynamic), else `0`.
  #[inline]
  pub(crate) const fn stride(&self) -> usize {
    if self.unsized_ { self.tail } else { 0 }
  }

  /// Size of a value whose tail slice has `len` elements, trailing padding included.
  #[inline]
  pub(crate) const fn size_of_val(&self, len: usize) -> Option<usize> {
    if !self.unsized_ {
      return Some(self.size);
    }
    match self.tail.checked_mul(len) {
      Some(tail) => match self.size.checked_add(tail) {
        Some(extent) => round_up(extent, self.align),
        None => None,
      },
      None => None,
    }
  }

  /// Offset of the field declared at `index`.
  #[doc(hidden)]
  #[must_use]
  pub const fn field(&self, index: usize) -> &Field {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let mut i = 0;
    while i < fields.len() {
      if fields[i].index == index {
        return &fields[i];
      }
      i += 1;
    }
    panic!("no such field")
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn field_offset(&self, index: usize) -> usize {
    self.field(index).offset
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn field_leaf(&self, index: usize) -> usize {
    self.field(index).leaf
  }

  #[inline]
  pub(crate) const fn shape(&self) -> Shape {
    match self.kind {
      Kind::Scalar(spec) => Shape::Scalar(spec),
      Kind::Array(_, len) => Shape::Array(len),
      Kind::Slice(_) => Shape::Slice,
      Kind::Struct(_, rules) => Shape::Struct(rules),
      Kind::Pattern(dfa) => Shape::Pattern(dfa),
      Kind::Enum(layout) => Shape::Enum(layout),
    }
  }

  /// Whatever a rule of the struct reads bytes of its value other than its fields (`le`, `be`,
  /// `crc32`, `sum16`): its padding, if it has any, is then read when the rule is checked.
  pub(crate) const fn rules_read_bytes(&self) -> bool {
    let (rules, mut i) = (self.rules(), 0);
    while i < rules.len() {
      if let Some(ops) = rules[i].ops()
        && ops_read_bytes(ops)
      {
        return true;
      }
      i += 1;
    }
    false
  }

  /// Rules of a struct.
  #[inline]
  pub(crate) const fn rules(&self) -> &'static [Rule] {
    match self.kind {
      Kind::Struct(_, rules) => rules,
      _ => &[],
    }
  }

  /// The rule that tells how long a value is from its first bytes, if any: the first `Size` or
  /// `Len` rule of the type, or else of its last field, and so on.
  pub(crate) const fn length_rule(&self) -> Option<LengthRule> {
    let (mut layout, mut base) = (self, 0);
    loop {
      let Kind::Struct(fields, rules) = layout.kind else { return None };
      let mut i = 0;
      while i < rules.len() {
        match rules[i] {
          Rule::Size(ops) => return Some(LengthRule { base, elements: false, ops }),
          Rule::Len(ops) => return Some(LengthRule { base, elements: true, ops }),
          _ => {}
        }
        i += 1;
      }
      if fields.is_empty() || !layout.unsized_ {
        return None;
      }
      let last = &fields[fields.len() - 1];
      (layout, base) = (last.layout, base + last.offset);
    }
  }

  /// The specification of the integer that the type is, if it is one: an initialized scalar of 1
  /// to 16 bytes, or a struct whose only field (but for the ones without size) is one.
  #[doc(hidden)]
  #[must_use]
  pub const fn as_integer(&self) -> Option<Spec> {
    match self.kind {
      Kind::Scalar(spec) if spec.is_integer() && spec.size >= 1 => Some(spec),
      Kind::Struct(fields, _) if !self.unsized_ => {
        let (mut inner, mut i): (Option<&Layout>, usize) = (None, 0);
        while i < fields.len() {
          let field = &fields[i];
          if field.layout.size > 0 {
            if inner.is_some() || field.offset != 0 || field.layout.size != self.size {
              return None;
            }
            inner = Some(field.layout);
          }
          i += 1;
        }
        match inner {
          Some(inner) => inner.as_integer(),
          None => None,
        }
      }
      _ => None,
    }
  }

  /// The tail of a struct, as rules see it: its slice if it is unsized, else its last field.
  pub(crate) const fn tail_shape(&self) -> TailShape {
    let mut shape = TailShape { start: self.size, bytes: 0, len: 0, array: false };
    let Kind::Struct(fields, _) = self.kind else { return shape };
    if self.unsized_ || fields.is_empty() {
      return shape;
    }
    // The last field, as declared.
    let (mut last, mut i) = (&fields[0], 1);
    while i < fields.len() {
      if fields[i].index > last.index {
        last = &fields[i];
      }
      i += 1;
    }
    (shape.start, shape.bytes) = (last.offset, last.layout.size);
    // The array it ends with, if any.
    let mut layout = last.layout;
    loop {
      match layout.kind {
        Kind::Array(_, len) => {
          (shape.len, shape.array) = (len, true);
          return shape;
        }
        Kind::Struct(fields, _) if !fields.is_empty() => layout = fields[fields.len() - 1].layout,
        _ => return shape,
      }
    }
  }

  /// Offset of the first byte of the tail slice (the size of a `Sized` type).
  #[inline]
  pub(crate) const fn head(&self) -> usize {
    self.size
  }

  /// Whatever the all-zeros bit pattern is valid.
  pub(crate) const fn zeroable(&self) -> bool {
    match self.kind {
      Kind::Scalar(spec) => spec.contains(0),
      Kind::Array(elem, len) => len == 0 || elem.zeroable(),
      Kind::Slice(elem) => elem.zeroable(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.zeroable() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Pattern(dfa) => dfa.states[0].accept,
      // The variant whose tag is 0, if any.
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].tag == 0 {
            let (fields, mut j) = (layout.variants[i].fields(), 0);
            while j < fields.len() {
              if !fields[j].layout.zeroable() {
                return false;
              }
              j += 1;
            }
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Whatever two layouts describe the same thing.
  #[expect(
    clippy::many_single_char_names,
    reason = "`a` and `x` are parts of `self`, `b` and `y` the same parts of `other`"
  )]
  pub(crate) const fn equals(&self, other: &Layout) -> bool {
    if self.size != other.size
      || self.align != other.align
      || self.tail != other.tail
      || self.unsized_ != other.unsized_
    {
      return false;
    }
    match (self.kind, other.kind) {
      // What a scalar requires, and what it is known to be: a layout that only requires what
      // another guarantees (`valid(..)` on a `u8`, and a `bool`) is not the same.
      (Kind::Scalar(a), Kind::Scalar(b)) => {
        a.size == b.size
          && same_valid(a.valid, b.valid, a.endian, b.endian)
          && same_valid(a.known, b.known, a.endian, b.endian)
      }
      (Kind::Array(a, n), Kind::Array(b, m)) => n == m && a.equals(b),
      (Kind::Slice(a), Kind::Slice(b)) => a.equals(b),
      (Kind::Struct(a, ra), Kind::Struct(b, rb)) => {
        if a.len() != b.len() || ra.len() != rb.len() {
          return false;
        }
        let mut i = 0;
        while i < a.len() {
          if a[i].offset != b[i].offset || !a[i].layout.equals(b[i].layout) {
            return false;
          }
          i += 1;
        }
        let mut i = 0;
        while i < ra.len() {
          if !ra[i].equals(&rb[i]) {
            return false;
          }
          i += 1;
        }
        true
      }
      (Kind::Pattern(a), Kind::Pattern(b)) => a.id == b.id,
      (Kind::Enum(a), Kind::Enum(b)) => {
        if a.tag.offset != b.tag.offset
          || !a.tag.layout.equals(b.tag.layout)
          || a.variants.len() != b.variants.len()
        {
          return false;
        }
        let mut i = 0;
        while i < a.variants.len() {
          let (x, y) = (&a.variants[i], &b.variants[i]);
          if x.tag != y.tag || !x.layout.equals(y.layout) {
            return false;
          }
          i += 1;
        }
        true
      }
      _ => false,
    }
  }
}

/// Whatever two sets of valid values of scalars of the same size are the same.
#[expect(
  clippy::many_single_char_names,
  reason = "`a` and `x` are of one scalar, `b` and `y` the same of the other"
)]
const fn same_valid(a: Valid, b: Valid, endian_a: Endian, endian_b: Endian) -> bool {
  match (a, b) {
    (Valid::Uninit, Valid::Uninit) | (Valid::Any, Valid::Any) => true,
    (Valid::In(x), Valid::In(y)) => {
      let mut i = 0;
      while i < x.len() && i < y.len() && x[i].0 == y[i].0 && x[i].1 == y[i].1 {
        i += 1;
      }
      endian_a.equals(endian_b) && x.len() == y.len() && i == x.len()
    }
    _ => false,
  }
}

// -------------------------------------------------------------------------------------------------
// `Dfa`

/// A deterministic finite automaton over bytes: the language of an unsized type like `str`.
///
/// The first state is the initial one. At most 64 states.
#[derive(Debug)]
pub struct Dfa {
  /// Two automata with the same `id` must be the same.
  pub id: u32,
  pub states: &'static [DfaState],
}

#[derive(Debug)]
pub struct DfaState {
  /// Whatever a value can end in this state.
  pub accept: bool,
  /// `(first byte, last byte, next state)`, sorted and disjoint.
  pub edges: &'static [(u8, u8, u8)],
}

impl Dfa {
  /// UTF-8, as of the Unicode standard (table 3-7, "well-formed UTF-8 byte sequences").
  pub const UTF8: &'static Dfa = &Dfa {
    id: 1,
    states: &[
      // 0: between two characters.
      DfaState {
        accept: true,
        edges: &[
          (0x00, 0x7f, 0),
          (0xc2, 0xdf, 1),
          (0xe0, 0xe0, 4),
          (0xe1, 0xec, 2),
          (0xed, 0xed, 5),
          (0xee, 0xef, 2),
          (0xf0, 0xf0, 6),
          (0xf1, 0xf3, 3),
          (0xf4, 0xf4, 7),
        ],
      },
      // 1, 2, 3: that many continuation bytes to go.
      DfaState { accept: false, edges: &[(0x80, 0xbf, 0)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 1)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 2)] },
      // 4: after `E0`, no overlong encoding.
      DfaState { accept: false, edges: &[(0xa0, 0xbf, 1)] },
      // 5: after `ED`, no surrogate.
      DfaState { accept: false, edges: &[(0x80, 0x9f, 1)] },
      // 6: after `F0`, no overlong encoding.
      DfaState { accept: false, edges: &[(0x90, 0xbf, 2)] },
      // 7: after `F4`, nothing above `U+10FFFF`.
      DfaState { accept: false, edges: &[(0x80, 0x8f, 2)] },
    ],
  };

  const fn check(&self) {
    assert!(!self.states.is_empty() && self.states.len() <= 64, "a DFA has 1 to 64 states");
    let mut i = 0;
    while i < self.states.len() {
      let (edges, mut j) = (self.states[i].edges, 0);
      while j < edges.len() {
        assert!(edges[j].0 <= edges[j].1 && (edges[j].2 as usize) < self.states.len());
        assert!(j == 0 || edges[j - 1].1 < edges[j].0, "DFA edges must be sorted and disjoint");
        j += 1;
      }
      i += 1;
    }
  }

  /// States reached from the states of `set` (a bit for each) with a byte of `lo..=hi`, and
  /// whatever every such byte leads somewhere from every state of `set`.
  pub(crate) const fn step(&self, set: u64, lo: u8, hi: u8) -> (u64, bool) {
    let (mut next, mut total, mut state) = (0, true, 0);
    while state < self.states.len() {
      if set & (1 << state) != 0 {
        // `need` is the first byte of `lo..=hi` that no edge covers so far.
        let (edges, mut need, mut i) = (self.states[state].edges, lo as u16, 0);
        while i < edges.len() && need <= hi as u16 {
          let (first, last, to) = edges[i];
          if last as u16 >= need && first <= hi {
            total &= first as u16 <= need;
            next |= 1 << to;
            need = last as u16 + 1;
          }
          i += 1;
        }
        total &= need > hi as u16;
      }
      state += 1;
    }
    (next, total)
  }

  /// Whatever a value can end in every state of `set`.
  pub(crate) const fn accepts(&self, set: u64) -> bool {
    let mut state = 0;
    while state < self.states.len() {
      if set & (1 << state) != 0 && !self.states[state].accept {
        return false;
      }
      state += 1;
    }
    true
  }

  /// Every state that can be reached from the initial one.
  pub(crate) const fn reachable(&self) -> u64 {
    let mut set = 1;
    loop {
      let next = set | self.step(set, 0, 0xff).0;
      if next == set {
        return set;
      }
      set = next;
    }
  }

  /// Whatever the automaton accepts the `len` bytes at `ptr`.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `len` initialized bytes.
  #[inline]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8, len: usize) -> bool {
    // SAFETY: guaranteed by the caller.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    if self.id == Self::UTF8.id {
      return core::str::from_utf8(bytes).is_ok();
    }
    let (mut state, mut i) = (0, 0);
    'bytes: while i < len {
      let (edges, mut j) = (self.states[state].edges, 0);
      while j < edges.len() {
        if edges[j].0 <= bytes[i] && bytes[i] <= edges[j].1 {
          state = edges[j].2 as usize;
          i += 1;
          continue 'bytes;
        }
        j += 1;
      }
      return false;
    }
    self.states[state].accept
  }
}

// -------------------------------------------------------------------------------------------------
// `Rule`

/// An operation of a postfix expression over the scalar fields of a struct, see [`Rule`].
///
/// Expressions are evaluated on `u128`. What cannot be computed (an overflow, an underflow, a
/// division by zero, a shift by 128 or more, a read out of the value) makes the rule that holds
/// the expression fail, unless it is the right side of a `&&` or of a `||` that the left side
/// decides without, as in Rust: `kind != 7 || be(40, 2) == 0` holds for a value that is not of
/// kind 7, however short, and `be(40, 2) == 0 || kind != 7` fails for one that is too short.
/// `<<` drops the bits that it shifts out, as in Rust.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
  /// Push a constant.
  Lit(u128),
  /// Push the unsigned integer of `size` bytes at `offset` in the struct.
  Load {
    offset: usize,
    size: u8,
    big: bool,
  },
  /// Push the size in bytes of the struct (not in `size` and `len` rules, which define it).
  Size,
  /// Replace an offset by the unsigned integer of `size` bytes that is there in the value.
  At {
    size: u8,
    big: bool,
  },
  /// Replace two offsets by the CRC-32 (the one of Ethernet) of the bytes between them.
  Crc32,
  /// Replace two offsets by the ones' complement sum of the big-endian 16 bits words between
  /// them (the checksum of IP, before it is complemented).
  Sum16,
  Add,
  Sub,
  Mul,
  Div,
  Rem,
  And,
  Or,
  Xor,
  Shl,
  Shr,
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
  /// `&&`
  Both,
  /// `||`
  Either,
  /// `!`, on a boolean.
  Not,
  /// Replace a condition and two values by the first value if the condition is true, by the
  /// second if not: only that one has to be computed.
  Select,
  /// Take a value off the stack: it is the `index`th variable.
  Set(u8),
  /// Push the `index`th variable.
  Get(u8),
  /// Make the value on top fail if one of the variables of a mask (a bit for each) could not be
  /// computed: what a `let` binds is computed, as in Rust, whether it is used or not. At the end
  /// of the block of the `let`s, so that a block that is not computed (in a branch of an `if`
  /// that is not taken) does not fail.
  Let(u8),
  /// Replace a value by what it becomes when, for as long as the expression `cond` is true of
  /// it, it is replaced by the expression `next`. In both, the value is the variable `var`.
  ///
  /// This is how what is at an offset that depends on what is before is reached: options, a
  /// chain of headers. A walk takes at most `8 * size + 128` steps, where `size` is the size of
  /// the value in bytes: a step for each bit of the value, and for each bit of a `u128`.
  Walk {
    var: u8,
    cond: &'static [Op],
    next: &'static [Op],
  },
}

/// The types of the constants of rules: unsigned integers, which they compute on.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
  message = "a constant of `#[pun(..)]` is an unsigned integer: `{Self}` is not one",
  label = "`#[pun(..)]` computes on unsigned integers",
  note = "an expression of `#[pun(..)]` means what it means in Rust, where every integer is a `u128`"
)]
pub trait Unsigned: Copy + seal::Unsigned {}

/// Implemented by this crate only.
mod seal {
  pub trait Unsigned {}
}

impl seal::Unsigned for u8 {}
impl seal::Unsigned for u16 {}
impl seal::Unsigned for u32 {}
impl seal::Unsigned for u64 {}
impl seal::Unsigned for u128 {}
impl seal::Unsigned for usize {}
impl Unsigned for u8 {}
impl Unsigned for u16 {}
impl Unsigned for u32 {}
impl Unsigned for u64 {}
impl Unsigned for u128 {}
impl Unsigned for usize {}

impl Op {
  /// What `as usize` keeps of a value.
  #[doc(hidden)]
  pub const USIZE_MAX: u128 = usize::MAX as u128;

  /// The value of a constant of a rule.
  #[doc(hidden)]
  #[must_use]
  pub const fn constant<T: Unsigned>(value: &T) -> u128 {
    // SAFETY: an unsigned integer is initialized, and has no padding.
    unsafe { load(core::ptr::from_ref::<T>(value).cast(), size_of::<T>(), Endian::NATIVE) }
  }

  /// Whatever the expression `ops` may fail to be computed, from the values that what it reads
  /// can take: the sizes of the fields, the bounds of the operands. For the `let`s of a block,
  /// which [`Op::Let`] checks only if they may fail. Conservative: an expression that reads a
  /// variable or the bytes of the value, or that walks, may fail.
  #[doc(hidden)]
  #[must_use]
  pub const fn fallible(ops: &[Op]) -> bool {
    // The least and the greatest value of each slot of the stack.
    let (mut slots, mut depth, mut at) = ([(0u128, 0u128); STACK], 0usize, 0);
    while at < ops.len() {
      let (pops, value) = match ops[at] {
        Op::Lit(value) => (0, (value, value)),
        Op::Load { size, .. } => (0, (0, max_value(size as usize))),
        Op::Size => (0, (0, usize::MAX as u128)),
        Op::Let(_) => (1, slots[depth - 1]),
        Op::Not | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Both | Op::Either => {
          (if matches!(ops[at], Op::Not) { 1 } else { 2 }, (0, 1))
        }
        Op::Select => {
          let ((a, b), (c, d)) = (slots[depth - 2], slots[depth - 1]);
          (3, (if a < c { a } else { c }, if b > d { b } else { d }))
        }
        op @ (Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::And
        | Op::Or
        | Op::Xor
        | Op::Shl
        | Op::Shr) => {
          let ((a, b), (c, d)) = (slots[depth - 2], slots[depth - 1]);
          let value = match op {
            Op::Add => match b.checked_add(d) {
              Some(most) => (a + c, most),
              None => return true,
            },
            Op::Sub if a >= d => (a - d, b - c),
            Op::Mul => match b.checked_mul(d) {
              Some(most) => (a * c, most),
              None => return true,
            },
            Op::Div if c > 0 => (a / d, b / c),
            Op::Rem if c > 0 => (0, if b < d { b } else { d - 1 }),
            Op::And => (0, if b < d { b } else { d }),
            Op::Or | Op::Xor => (0, u128::MAX >> (b | d).leading_zeros()),
            Op::Shl if d < 128 => (0, u128::MAX),
            Op::Shr if d < 128 => (a >> d, b >> c),
            _ => return true,
          };
          (2, value)
        }
        Op::At { .. } | Op::Crc32 | Op::Sum16 | Op::Set(_) | Op::Get(_) | Op::Walk { .. } => {
          return true;
        }
      };
      depth -= pops;
      slots[depth] = value;
      depth += 1;
      at += 1;
    }
    false
  }

  /// Push the value of the field of type `T` at `offset`: an integer, or a struct that only
  /// holds one.
  ///
  /// # Panics
  /// If `T` is not an integer.
  #[must_use]
  pub const fn load<T: crate::Pun>(offset: usize) -> Self {
    let Some(spec) = T::LAYOUT.as_integer() else {
      panic!(
        "a rule reads the integer fields of its struct, and this field is not an integer: read \
         its bytes with `le(offset, size)` or `be(offset, size)`"
      )
    };
    Op::Load { offset, size: spec.size as u8, big: !spec.endian.is_little() }
  }

  #[expect(
    clippy::many_single_char_names,
    reason = "`a`, `b`, `c` of one side are compared with `x`, `y`, `z` of the other"
  )]
  const fn equals(&self, other: &Op) -> bool {
    match (*self, *other) {
      (Op::Lit(a), Op::Lit(b)) => a == b,
      (Op::Load { offset: a, size: b, big: c }, Op::Load { offset: x, size: y, big: z }) => {
        a == x && b == y && c == z
      }
      (Op::At { size: a, big: b }, Op::At { size: x, big: y }) => a == x && b == y,
      (Op::Set(a), Op::Set(b)) | (Op::Get(a), Op::Get(b)) | (Op::Let(a), Op::Let(b)) => a == b,
      (Op::Walk { var: a, cond: b, next: c }, Op::Walk { var: x, cond: y, next: z }) => {
        a == x && same_ops(b, y) && same_ops(c, z)
      }
      (
        Op::Lit(_)
        | Op::Load { .. }
        | Op::At { .. }
        | Op::Set(_)
        | Op::Get(_)
        | Op::Let(_)
        | Op::Walk { .. },
        _,
      )
      | (
        _,
        Op::Lit(_)
        | Op::Load { .. }
        | Op::At { .. }
        | Op::Set(_)
        | Op::Get(_)
        | Op::Let(_)
        | Op::Walk { .. },
      ) => false,
      // Other operations hold no data.
      (a, b) => a.code() == b.code(),
    }
  }

  const fn code(self) -> u8 {
    // SAFETY-free: the discriminant of a variant without data, by exhaustion.
    match self {
      Op::Lit(_) => 0,
      Op::Load { .. } => 1,
      Op::Add => 2,
      Op::Sub => 3,
      Op::Mul => 4,
      Op::Div => 5,
      Op::Rem => 6,
      Op::And => 7,
      Op::Or => 8,
      Op::Xor => 9,
      Op::Shl => 10,
      Op::Shr => 11,
      Op::Eq => 12,
      Op::Ne => 13,
      Op::Lt => 14,
      Op::Le => 15,
      Op::Gt => 16,
      Op::Ge => 17,
      Op::Both => 18,
      Op::Either => 19,
      Op::Not => 20,
      Op::Size => 21,
      Op::At { .. } => 22,
      Op::Crc32 => 23,
      Op::Sum16 => 24,
      Op::Select => 25,
      Op::Set(_) => 26,
      Op::Get(_) => 27,
      Op::Walk { .. } => 28,
      Op::Let(_) => 29,
    }
  }
}

/// Whatever two expressions are the same.
const fn same_ops(a: &[Op], b: &[Op]) -> bool {
  let mut i = 0;
  while i < a.len() && i < b.len() && a[i].equals(&b[i]) {
    i += 1;
  }
  a.len() == b.len() && i == a.len()
}

/// Something that values of a struct must satisfy, beyond the validity of their fields.
///
/// Rules are checked when a value is made from something else (`try_pun*`, `from_head`), and
/// tell `from_head` how long a value is. They are not invariants that unsafe code may rely on: safe
/// code can write to the fields of a struct.
#[derive(Copy, Clone, Debug)]
pub enum Rule {
  /// The size in bytes of the value is the result of the expression.
  Size(&'static [Op]),
  /// The number of elements of the tail slice of the value is the result of the expression.
  Len(&'static [Op]),
  /// The expression is true.
  Assert(&'static [Op]),
  /// The expression, which has one [`Op::Walk`], is true.
  Walk(&'static [Op]),
  /// The tail of the value starts with a region of as many bytes as the result of the
  /// expression (`None`: what remains). Regions follow each other, in the order of the rules.
  Region(Option<&'static [Op]>),
}

/// Whatever an expression reads bytes of the value that are not those of its integer fields.
const fn ops_read_bytes(ops: &[Op]) -> bool {
  let mut i = 0;
  while i < ops.len() {
    match ops[i] {
      Op::At { .. } | Op::Crc32 | Op::Sum16 => return true,
      Op::Walk { cond, next, .. } if ops_read_bytes(cond) || ops_read_bytes(next) => return true,
      _ => {}
    }
    i += 1;
  }
  false
}

/// The operations of an expression can read the whole value, not only its sized fields.
const fn reads_value(op: &Op) -> bool {
  matches!(op, Op::Size | Op::At { .. } | Op::Crc32 | Op::Sum16 | Op::Walk { .. })
}

impl Rule {
  const fn ops(&self) -> Option<&'static [Op]> {
    match *self {
      Rule::Size(ops)
      | Rule::Len(ops)
      | Rule::Assert(ops)
      | Rule::Region(Some(ops))
      | Rule::Walk(ops) => Some(ops),
      Rule::Region(None) => None,
    }
  }

  const fn equals(&self, other: &Rule) -> bool {
    let same_kind = matches!(
      (self, other),
      (Rule::Size(_), Rule::Size(_))
        | (Rule::Len(_), Rule::Len(_))
        | (Rule::Assert(_), Rule::Assert(_))
        | (Rule::Walk(_), Rule::Walk(_))
        | (Rule::Region(_), Rule::Region(_))
    );
    match (self.ops(), other.ops()) {
      (None, None) => same_kind,
      (Some(a), Some(b)) => same_kind && same_ops(a, b),
      _ => false,
    }
  }
}

/// See [`Layout::tail_shape`].
#[derive(Copy, Clone)]
pub(crate) struct TailShape {
  /// Offset of the tail.
  pub(crate) start: usize,
  /// Size of the tail, if the struct is `Sized`.
  pub(crate) bytes: usize,
  /// Number of elements of the array that the tail ends with, if the struct is `Sized`.
  pub(crate) len: usize,
  pub(crate) array: bool,
}

/// See [`Layout::length_rule`].
#[derive(Copy, Clone)]
pub(crate) struct LengthRule {
  /// Offset of the struct that has the rule.
  pub(crate) base: usize,
  /// The expression is a number of elements of the tail slice, rather than the size in bytes of
  /// the struct.
  pub(crate) elements: bool,
  pub(crate) ops: &'static [Op],
}

/// Depth of the stack of [`eval`].
const STACK: usize = 8;
/// Number of variables of an expression.
const VARIABLES: usize = 4;
/// An expression that has a [`Op::Walk`] is, as each expression of the walk, of at most that
/// many operations: they are counted with types, see `validate::check_walks`.
const WALK_OPS: usize = 72;
const WALK_INNER_OPS: usize = 56;

/// Check that the expressions of `rules` are well formed, and only read the first `head` bytes.
const fn check_rules(rules: &[Rule], head: usize) {
  let mut i = 0;
  while i < rules.len() {
    if let Some(ops) = rules[i].ops() {
      let sizes = matches!(rules[i], Rule::Size(_) | Rule::Len(_));
      let walks = matches!(rules[i], Rule::Walk(_));
      assert!(!walks || ops.len() <= WALK_OPS, "an expression that walks is too long");
      let walked = check_ops(ops, head, sizes, walks, 0);
      assert!(walked == walks as usize, "a `walk`, and only one, is in a rule that walks");
    }
    i += 1;
  }
}

/// Check an expression, in which the variables of `defined` are (a bit for each): returns the
/// number of [`Op::Walk`] that it has.
const fn check_ops(ops: &[Op], head: usize, sizes: bool, walks: bool, mut defined: u8) -> usize {
  let (mut depth, mut walked, mut i) = (0usize, 0, 0);
  while i < ops.len() {
    assert!(
      !(sizes && reads_value(&ops[i])),
      "the size of a value cannot be defined from the value itself"
    );
    match ops[i] {
      Op::Lit(_) | Op::Size => depth += 1,
      Op::At { size, .. } => assert!(size >= 1 && size <= 16 && depth >= 1, "malformed expression"),
      Op::Load { offset, size, .. } => {
        assert!(size >= 1 && size <= 16 && offset + size as usize <= head, "field out of bounds");
        depth += 1;
      }
      Op::Not => assert!(depth >= 1, "malformed expression"),
      Op::Select => {
        assert!(depth >= 3, "malformed expression");
        depth -= 2;
      }
      Op::Set(index) => {
        assert!(depth >= 1, "malformed expression");
        assert!((index as usize) < VARIABLES, "too many variables in an expression");
        (depth, defined) = (depth - 1, defined | 1 << index);
      }
      Op::Get(index) => {
        assert!((index as usize) < VARIABLES && defined & (1 << index) != 0, "no such variable");
        depth += 1;
      }
      Op::Let(mask) => {
        assert!(depth >= 1, "malformed expression");
        assert!(defined & mask == mask, "no such variable");
      }
      Op::Walk { var, cond, next } => {
        assert!(depth >= 1, "malformed expression");
        assert!(walks, "`walk` is for `assert`, and cannot be in another `walk`");
        walked += 1;
        assert!((var as usize) < VARIABLES, "too many variables in an expression");
        assert!(
          cond.len() <= WALK_INNER_OPS && next.len() <= WALK_INNER_OPS,
          "the expressions of a `walk` are too long"
        );
        check_ops(cond, head, sizes, false, defined | 1 << var);
        check_ops(next, head, sizes, false, defined | 1 << var);
      }
      _ => {
        assert!(depth >= 2, "malformed expression");
        depth -= 1;
      }
    }
    assert!(depth <= STACK, "expression is too deep");
    i += 1;
  }
  assert!(depth == 1, "malformed expression");
  walked
}

/// What evaluates an expression: a stack, and variables.
///
/// This is written so that, once inlined in a function where the expression is a constant, which
/// is how it is used, what remains is the arithmetic of the expression, without relying on much
/// of the optimizer: the stack is not an array but a variable for each of its slots.
#[derive(Copy, Clone)]
pub(crate) struct Machine {
  stack: (u128, u128, u128, u128, u128, u128, u128, u128),
  variables: (u128, u128, u128, u128),
  depth: usize,
  /// A bit for each slot whose value could not be computed (an overflow, a read out of bounds).
  failed: u8,
  /// The same, for each variable.
  unknown: u8,
}

impl Machine {
  pub(crate) const NEW: Self = Self {
    stack: (0, 0, 0, 0, 0, 0, 0, 0),
    variables: (0, 0, 0, 0),
    depth: 0,
    failed: 0,
    unknown: 0,
  };

  #[inline(always)]
  const fn get(&self, slot: usize) -> u128 {
    match slot {
      0 => self.stack.0,
      1 => self.stack.1,
      2 => self.stack.2,
      3 => self.stack.3,
      4 => self.stack.4,
      5 => self.stack.5,
      6 => self.stack.6,
      _ => self.stack.7,
    }
  }

  /// Set a slot to a value, or mark it as failed.
  #[inline(always)]
  const fn set(&mut self, slot: usize, value: Option<u128>) {
    match value {
      Some(value) => {
        self.failed &= !(1 << slot);
        match slot {
          0 => self.stack.0 = value,
          1 => self.stack.1 = value,
          2 => self.stack.2 = value,
          3 => self.stack.3 = value,
          4 => self.stack.4 = value,
          5 => self.stack.5 = value,
          6 => self.stack.6 = value,
          _ => self.stack.7 = value,
        }
      }
      None => self.failed |= 1 << slot,
    }
  }

  #[inline(always)]
  const fn has_failed(&self, slot: usize) -> bool {
    self.failed & (1 << slot) != 0
  }

  #[inline(always)]
  const fn slot(&self, slot: usize) -> Option<u128> {
    if self.has_failed(slot) { None } else { Some(self.get(slot)) }
  }

  #[inline(always)]
  const fn variable(&self, index: u8) -> Option<u128> {
    if self.unknown & (1 << index) != 0 {
      return None;
    }
    Some(match index {
      0 => self.variables.0,
      1 => self.variables.1,
      2 => self.variables.2,
      _ => self.variables.3,
    })
  }

  #[inline(always)]
  const fn set_variable(&mut self, index: u8, value: Option<u128>) {
    match value {
      Some(value) => {
        self.unknown &= !(1 << index);
        match index {
          0 => self.variables.0 = value,
          1 => self.variables.1 = value,
          2 => self.variables.2 = value,
          _ => self.variables.3 = value,
        }
      }
      None => self.unknown |= 1 << index,
    }
  }

  /// The value of the expression, once all of its operations are applied.
  #[inline(always)]
  pub(crate) const fn result(&self) -> Option<u128> {
    self.slot(0)
  }

  /// The value at the top of the stack.
  #[inline(always)]
  pub(crate) const fn top(&self) -> Option<u128> {
    self.slot(self.depth - 1)
  }

  /// Replace the value at the top of the stack.
  #[inline(always)]
  pub(crate) const fn replace(&mut self, value: Option<u128>) {
    self.set(self.depth - 1, value);
  }

  /// A machine to evaluate another expression, in which the variable `index` is `value` and the
  /// other ones are what they are here.
  #[inline(always)]
  pub(crate) const fn enter(&self, index: u8, value: u128) -> Self {
    let mut inner = Self { depth: 0, failed: 0, ..*self };
    inner.set_variable(index, Some(value));
    inner
  }

  /// Apply an operation of an expression over the struct at `base`, whose size is `size`.
  ///
  /// # Safety
  /// See [`eval`].
  // Not in debug builds, where this would only make every struct that has rules slow to build.
  #[cfg_attr(not(debug_assertions), inline(always))]
  pub(crate) const unsafe fn apply(&mut self, op: Op, base: *const u8, size: usize) {
    let depth = self.depth;
    match op {
      Op::Lit(value) => {
        self.set(depth, Some(value));
        self.depth += 1;
      }
      Op::Load { offset, size, big } => {
        let endian = if big { Endian::Big } else { Endian::Little };
        // SAFETY: `check_rules` made sure that the field is in the sized part of the struct.
        self.set(depth, Some(unsafe { load(base.add(offset), size as usize, endian) }));
        self.depth += 1;
      }
      Op::Size => {
        self.set(depth, Some(size as u128));
        self.depth += 1;
      }
      Op::Get(index) => {
        self.set(depth, self.variable(index));
        self.depth += 1;
      }
      Op::Set(index) => {
        self.set_variable(index, self.slot(depth - 1));
        self.depth -= 1;
      }
      Op::Let(mask) => {
        if self.unknown & mask != 0 {
          self.set(depth - 1, None);
        }
      }
      Op::Not => {
        if !self.has_failed(depth - 1) {
          self.set(depth - 1, Some((self.get(depth - 1) == 0) as u128));
        }
      }
      Op::At { size: width, big } => {
        let (offset, endian) =
          (self.get(depth - 1), if big { Endian::Big } else { Endian::Little });
        let inside = offset <= size as u128 && size - offset as usize >= width as usize;
        if inside && !self.has_failed(depth - 1) {
          // SAFETY: in the bounds of the value.
          let value = unsafe { load(base.add(offset as usize), width as usize, endian) };
          self.set(depth - 1, Some(value));
        } else {
          self.set(depth - 1, None);
        }
      }
      op @ (Op::Crc32 | Op::Sum16) => {
        let (start, end) = (self.get(depth - 2), self.get(depth - 1));
        let inside = start <= end && end <= size as u128;
        let computed = inside && !self.has_failed(depth - 2) && !self.has_failed(depth - 1);
        self.depth -= 1;
        if computed {
          // SAFETY: in the bounds of the value.
          let bytes = unsafe {
            core::slice::from_raw_parts(base.add(start as usize), (end - start) as usize)
          };
          let value =
            if matches!(op, Op::Crc32) { crc32(bytes) as u128 } else { sum16(bytes) as u128 };
          self.set(depth - 2, Some(value));
        } else {
          self.set(depth - 2, None);
        }
      }
      // As in Rust, the left side first: the right one only counts if the left one does not
      // decide, and may then have failed.
      op @ (Op::Both | Op::Either) => {
        let decides = matches!(op, Op::Either) as u128;
        let value = match self.slot(depth - 2) {
          Some(a) if (a != 0) as u128 == decides => Some(decides),
          Some(_) => match self.slot(depth - 1) {
            Some(b) => Some((b != 0) as u128),
            None => None,
          },
          None => None,
        };
        self.depth -= 1;
        self.set(depth - 2, value);
      }
      // The value that is not selected may have failed.
      Op::Select => {
        let value = match self.slot(depth - 3) {
          Some(0) => self.slot(depth - 1),
          Some(_) => self.slot(depth - 2),
          None => None,
        };
        self.depth -= 2;
        self.set(depth - 3, value);
      }
      // See `validate::check_walks`, which is what evaluates the expressions that walk.
      Op::Walk { .. } => self.set(depth - 1, None),
      op => {
        let computed = !self.has_failed(depth - 2) && !self.has_failed(depth - 1);
        let value = binary(op, self.get(depth - 2), self.get(depth - 1));
        self.depth -= 1;
        self.set(depth - 2, if computed { value } else { None });
      }
    }
  }
}

/// Evaluate a postfix expression over the struct at `base`.
///
/// The first operations are not in a loop: where the expression is a constant, each of them is
/// one, and what remains is its arithmetic (see [`Machine`]).
///
/// # Safety
/// `ops` must be an expression of a rule of the layout of a struct, and `base` must be valid for
/// reads of the sized fields of such a struct, initialized. `size` is the size of the struct:
/// unless the expression is the one of a `size` or `len` rule, `base` must be valid for reads of
/// that many initialized bytes.
// Not in debug builds, where this would only make every struct that has rules slow to build.
#[cfg_attr(not(debug_assertions), inline(always))]
pub(crate) const unsafe fn eval(ops: &[Op], base: *const u8, size: usize) -> Option<u128> {
  let mut machine = Machine::NEW;
  macro_rules! unrolled {
    ($($index:literal)+) => {$(
      if $index < ops.len() {
        // SAFETY: guaranteed by the caller.
        unsafe { machine.apply(ops[$index], base, size) };
      }
    )+};
  }
  unrolled!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23);
  let mut i = 24;
  while i < ops.len() {
    // SAFETY: guaranteed by the caller.
    unsafe { machine.apply(ops[i], base, size) };
    i += 1;
  }
  machine.result()
}

#[inline(always)]
const fn binary(op: Op, a: u128, b: u128) -> Option<u128> {
  match op {
    Op::Add => a.checked_add(b),
    Op::Sub => a.checked_sub(b),
    Op::Mul => a.checked_mul(b),
    Op::Div => a.checked_div(b),
    Op::Rem => a.checked_rem(b),
    Op::And => Some(a & b),
    Op::Or => Some(a | b),
    Op::Xor => Some(a ^ b),
    Op::Shl => {
      if b < 128 {
        Some(a << b)
      } else {
        None
      }
    }
    Op::Shr => {
      if b < 128 {
        Some(a >> b)
      } else {
        None
      }
    }
    Op::Eq => Some((a == b) as u128),
    Op::Ne => Some((a != b) as u128),
    Op::Lt => Some((a < b) as u128),
    Op::Le => Some((a <= b) as u128),
    Op::Gt => Some((a > b) as u128),
    Op::Ge => Some((a >= b) as u128),
    Op::Lit(_)
    | Op::Load { .. }
    | Op::Size
    | Op::Not
    | Op::At { .. }
    | Op::Crc32
    | Op::Sum16
    | Op::Both
    | Op::Either
    | Op::Select
    | Op::Set(_)
    | Op::Get(_)
    | Op::Let(_)
    | Op::Walk { .. } => None,
  }
}

/// The CRC-32 of Ethernet (and zlib, PNG...): the frame check sequence of a frame is the CRC of
/// what is before it, least significant byte first.
///
/// ```
/// assert_eq!(puns::crc32(b"123456789"), 0xcbf43926);
/// ```
#[must_use]
pub const fn crc32(bytes: &[u8]) -> u32 {
  const TABLE: [u32; 256] = {
    let (mut table, mut i) = ([0u32; 256], 0);
    while i < 256 {
      let (mut value, mut bit) = (i as u32, 0);
      while bit < 8 {
        value = if value & 1 != 0 { (value >> 1) ^ 0xedb8_8320 } else { value >> 1 };
        bit += 1;
      }
      table[i] = value;
      i += 1;
    }
    table
  };
  let (mut crc, mut i) = (!0u32, 0);
  while i < bytes.len() {
    crc = TABLE[((crc ^ bytes[i] as u32) & 0xff) as usize] ^ (crc >> 8);
    i += 1;
  }
  !crc
}

/// The ones' complement sum of the big-endian 16 bits words of `bytes`.
///
/// A last byte alone is the high byte of a word. The checksums of IP, UDP and TCP are the
/// complement of this sum: over data that holds its own checksum, it is `0xffff`.
///
/// ```
/// assert_eq!(puns::sum16(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]), 0xddf2);
/// ```
#[must_use]
pub const fn sum16(bytes: &[u8]) -> u16 {
  let (mut sum, mut i) = (0u64, 0);
  while i < bytes.len() {
    let low = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
    sum += ((bytes[i] as u64) << 8) | low as u64;
    // No overflow: the carries are folded as they come.
    sum = (sum & 0xffff) + (sum >> 16);
    i += 2;
  }
  sum as u16
}

/// Bounds of a range of valid values of an integer of the size of `T`, for `#[pun(valid(..))]`.
#[doc(hidden)]
#[must_use]
pub const fn range<T>(start: Option<u128>, end: Option<u128>, inclusive: bool) -> (u128, u128) {
  let start = match start {
    Some(start) => start,
    None => 0,
  };
  let end = match end {
    Some(end) if inclusive => end,
    Some(end) => {
      assert!(end > start, "empty range");
      end - 1
    }
    None => max_value(size_of::<T>()),
  };
  assert!(start <= end, "empty range");
  (start, end)
}

/// Sort and merge inclusive ranges; returns the ranges and their count.
#[doc(hidden)]
#[must_use]
pub const fn merge_ranges<const N: usize>(
  mut ranges: [(u128, u128); N],
) -> ([(u128, u128); N], usize) {
  let mut i = 1;
  while i < N {
    let mut j = i;
    while j > 0 && ranges[j - 1].0 > ranges[j].0 {
      ranges.swap(j - 1, j);
      j -= 1;
    }
    i += 1;
  }
  let (mut merged, mut len, mut i) = ([(0, 0); N], 0, 0);
  while i < N {
    if len > 0 && (ranges[i].0 <= merged[len - 1].1 || ranges[i].0 - merged[len - 1].1 == 1) {
      if ranges[i].1 > merged[len - 1].1 {
        merged[len - 1].1 = ranges[i].1;
      }
    } else {
      merged[len] = ranges[i];
      len += 1;
    }
    i += 1;
  }
  (merged, len)
}

/// The bits of a value of at most 16 bytes, as a native-endian unsigned integer.
///
/// # Safety
/// Every byte of `value` must be initialized.
#[doc(hidden)]
pub const unsafe fn bits_of<T>(value: &T) -> u128 {
  assert!(size_of::<T>() <= 16, "a fieldless enum cannot be larger than 16 bytes");
  // SAFETY: guaranteed by the caller.
  unsafe { load(core::ptr::from_ref::<T>(value).cast(), size_of::<T>(), Endian::NATIVE) }
}
