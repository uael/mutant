//! Value-level description of the memory layout and of the valid bit patterns of a type.
//!
//! Every [`Data`](crate::Data) type exposes a `const LAYOUT: Layout`: a small tree, built at
//! compile time, that tells for every byte of the type whether it is padding, an unconstrained
//! initialized byte, or part of a *scalar* with a restricted set of valid values.
//!
//! The whole static analysis of the crate is ordinary `const fn` code running over those values
//! (see [`analysis`](crate::analysis)); nothing is encoded in the type system.

use crate::ptr;
use core::mem;

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
  /// This is `valid`, but for fields that are restricted by `#[data(valid(..))]`: nothing
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
  pub const fn any<T>() -> Self {
    Self { size: size_of::<T>(), endian: Endian::NATIVE, valid: Valid::Any, known: Valid::Any }
  }

  /// A scalar the size of `T` that accepts everything, uninitialized bytes included.
  #[inline]
  pub const fn uninit<T>() -> Self {
    let (size, endian) = (size_of::<T>(), Endian::NATIVE);
    Self { size, endian, valid: Valid::Uninit, known: Valid::Uninit }
  }

  /// A scalar the size of `T` whose value, read as an unsigned integer in `endian` byte order,
  /// must be in one of `ranges`.
  ///
  /// `ranges` are inclusive and must be sorted, disjoint and non-adjacent; this is checked at
  /// compile time. See [`sorted_ranges`] to build them from a list of values.
  pub const fn ranges<T>(endian: Endian, ranges: &'static [(u128, u128)]) -> Self {
    Self { size: size_of::<T>(), endian, valid: Valid::Any, known: Valid::Any }
      .with_ranges(ranges)
  }

  /// Same as `self`, with other valid `ranges`, see [`Spec::ranges`].
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

  /// Whatever the scalar is only restricted by a `#[data(valid(..))]` that the type system does
  /// not enforce: it is then proven by restrictions of the same kind.
  #[inline]
  pub(crate) const fn is_soft(&self) -> bool {
    matches!(self.known, Valid::Any) && matches!(self.valid, Valid::In(_))
  }

  /// Same as `self`, read in another byte order.
  #[inline]
  pub const fn with_endian(self, endian: Endian) -> Self {
    Self { endian, ..self }
  }

  /// Whatever the scalar is an initialized integer of at most 16 bytes.
  #[doc(hidden)]
  pub const fn is_integer(&self) -> bool {
    self.size <= 16 && !matches!(self.known, Valid::Uninit) && !matches!(self.valid, Valid::Uninit)
  }

  /// The scalar at `ptr`, as an unsigned integer.
  ///
  /// # Safety
  /// `self` must be [an integer](Self::is_integer), and `ptr` must be valid for reads of one.
  #[doc(hidden)]
  #[inline(always)]
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
/// use transmutant::layout::sorted_ranges;
///
/// assert_eq!(sorted_ranges([7, 1, 0, 2]), ([(0, 2), (7, 7), (0, 0), (0, 0)], 2));
/// ```
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
  pub const fn new(offset: usize, layout: &'static Layout) -> Self {
    Self { offset, layout, index: 0, leaf: 0, restricted: false }
  }

  /// A scalar field that only accepts some of the values of its type, see
  /// [`Layout::restricted`].
  #[inline]
  pub const fn restricted(offset: usize, layout: &'static Layout) -> Self {
    assert!(matches!(layout.kind, Kind::Scalar(_)), "only scalar fields can be restricted");
    Self { offset, layout, index: 0, leaf: 0, restricted: true }
  }

  /// The specification of a restricted field.
  #[doc(hidden)]
  pub const fn restriction(&self) -> Option<Spec> {
    match self.layout.kind {
      Kind::Scalar(spec) if self.restricted => Some(spec),
      _ => None,
    }
  }

  #[doc(hidden)]
  pub const fn offset(&self) -> usize {
    self.offset
  }

  #[doc(hidden)]
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
pub const fn tail_offset(end: usize, align: usize, pack: usize) -> usize {
  match round_up(end, if align < pack { align } else { pack }) {
    Some(offset) => offset,
    None => panic!("struct is too large"),
  }
}

impl Layout {
  /// Layout of a scalar.
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
  pub const fn pattern<T: ?Sized>(probe: *const T, dfa: &'static Dfa) -> Self {
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
  pub const fn slice<T>(probe: *const [T], elem: &'static Layout) -> Self {
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
  pub const fn structure<T: ?Sized>(
    probe: *const T, fields: &'static [Field], rules: &'static [Rule],
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
        check_rules(rules, end, false);
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
        check_rules(rules, end, true);
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

  /// Number the fields of a struct (given in declaration order) and sort them by offset.
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
  pub const fn align(&self) -> usize {
    self.align
  }

  /// Size of the type if it is `Sized`, otherwise the offset of its tail slice.
  #[inline]
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
  pub const fn field_offset(&self, index: usize) -> usize {
    self.field(index).offset
  }

  #[doc(hidden)]
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
    }
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
    }
  }

  /// Whatever two layouts describe the same thing.
  pub(crate) const fn equals(&self, other: &Layout) -> bool {
    if self.size != other.size
      || self.align != other.align
      || self.tail != other.tail
      || self.unsized_ != other.unsized_
    {
      return false;
    }
    match (self.kind, other.kind) {
      (Kind::Scalar(a), Kind::Scalar(b)) => {
        a.size == b.size
          && match (a.valid, b.valid) {
            (Valid::Uninit, Valid::Uninit) | (Valid::Any, Valid::Any) => true,
            (Valid::In(x), Valid::In(y)) => {
              let mut i = 0;
              while i < x.len() && i < y.len() && x[i].0 == y[i].0 && x[i].1 == y[i].1 {
                i += 1;
              }
              a.endian.equals(b.endian) && x.len() == y.len() && i == x.len()
            }
            _ => false,
          }
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
      _ => false,
    }
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
/// division by zero, a read out of the value) makes the rule that holds the expression fail,
/// unless it is the side of a `&&` or of a `||` that the other side decides without:
/// `kind != 7 || be(40, 2) == 0` holds for a value that is not of kind 7, however short.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
  /// Push a constant.
  Lit(u128),
  /// Push the unsigned integer of `size` bytes at `offset` in the struct.
  Load { offset: usize, size: u8, big: bool },
  /// Push the size in bytes of the struct (not in `size` and `len` rules, which define it).
  Size,
  /// Replace an offset by the unsigned integer of `size` bytes that is there in the value.
  At { size: u8, big: bool },
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
}

impl Op {
  /// Push the value of the field of type `T` at `offset`: an integer, or a struct that only
  /// holds one.
  pub const fn load<T: crate::Data>(offset: usize) -> Self {
    let Some(spec) = T::LAYOUT.as_integer() else { panic!("not an integer field") };
    Op::Load { offset, size: spec.size as u8, big: !spec.endian.is_little() }
  }

  const fn equals(&self, other: &Op) -> bool {
    match (*self, *other) {
      (Op::Lit(a), Op::Lit(b)) => a == b,
      (Op::Load { offset: a, size: b, big: c }, Op::Load { offset: x, size: y, big: z }) => {
        a == x && b == y && c == z
      }
      (Op::At { size: a, big: b }, Op::At { size: x, big: y }) => a == x && b == y,
      (Op::Lit(_) | Op::Load { .. } | Op::At { .. }, _)
      | (_, Op::Lit(_) | Op::Load { .. } | Op::At { .. }) => false,
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
    }
  }
}

/// Something that values of a struct must satisfy, beyond the validity of their fields.
///
/// Rules are checked when a value is made from something else (`try_transmute*`, `unpack`), and
/// tell `unpack` how long a value is. They are not invariants that unsafe code may rely on: safe
/// code can write to the fields of a struct.
#[derive(Copy, Clone, Debug)]
pub enum Rule {
  /// The size in bytes of the value is the result of the expression.
  Size(&'static [Op]),
  /// The number of elements of the tail slice of the value is the result of the expression.
  Len(&'static [Op]),
  /// The expression is true.
  Assert(&'static [Op]),
  /// The tail of the value starts with a region of as many bytes as the result of the
  /// expression (`None`: what remains). Regions follow each other, in the order of the rules.
  Region(Option<&'static [Op]>),
}

/// The operations of an expression can read the whole value, not only its sized fields.
const fn reads_value(op: &Op) -> bool {
  matches!(op, Op::Size | Op::At { .. } | Op::Crc32 | Op::Sum16)
}

impl Rule {
  const fn ops(&self) -> Option<&'static [Op]> {
    match *self {
      Rule::Size(ops) | Rule::Len(ops) | Rule::Assert(ops) | Rule::Region(Some(ops)) => Some(ops),
      Rule::Region(None) => None,
    }
  }

  const fn equals(&self, other: &Rule) -> bool {
    let same_kind = matches!(
      (self, other),
      (Rule::Size(_), Rule::Size(_))
        | (Rule::Len(_), Rule::Len(_))
        | (Rule::Assert(_), Rule::Assert(_))
        | (Rule::Region(_), Rule::Region(_))
    );
    match (self.ops(), other.ops()) {
      (None, None) => same_kind,
      (Some(a), Some(b)) => {
        let mut i = 0;
        while i < a.len() && i < b.len() && a[i].equals(&b[i]) {
          i += 1;
        }
        same_kind && a.len() == b.len() && i == a.len()
      }
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

/// Check that the expressions of `rules` are well formed, and only read the first `head` bytes.
const fn check_rules(rules: &[Rule], head: usize, unsized_: bool) {
  let mut i = 0;
  while i < rules.len() {
    let _ = unsized_;
    if let Some(ops) = rules[i].ops() {
      let (mut depth, mut j) = (0usize, 0);
      while j < ops.len() {
        assert!(
          !(reads_value(&ops[j]) && matches!(rules[i], Rule::Size(_) | Rule::Len(_))),
          "the size of a value cannot be defined from the value itself"
        );
        match ops[j] {
          Op::Lit(_) | Op::Size => depth += 1,
          Op::At { size, .. } => assert!(size >= 1 && size <= 16 && depth >= 1, "malformed expression"),
          Op::Load { offset, size, .. } => {
            assert!(size >= 1 && size <= 16 && offset + size as usize <= head, "field out of bounds");
            depth += 1;
          }
          Op::Not => assert!(depth >= 1, "malformed expression"),
          _ => {
            assert!(depth >= 2, "malformed expression");
            depth -= 1;
          }
        }
        assert!(depth <= STACK, "expression is too deep");
        j += 1;
      }
      assert!(depth == 1, "malformed expression");
    }
    i += 1;
  }
}

/// Evaluate a postfix expression over the struct at `base`.
///
/// This is written so that, once inlined in a function where the expression is a constant, which
/// is how it is used, what remains is the arithmetic of the expression, without relying on much
/// of the optimizer: the first operations are not in a loop, and the stack is not an array but a
/// local variable for each of its slots.
///
/// # Safety
/// `ops` must be an expression of a rule of the layout of a struct, and `base` must be valid for
/// reads of the sized fields of such a struct, initialized. `size` is the size of the struct:
/// unless the expression is the one of a `size` or `len` rule, `base` must be valid for reads of
/// that many initialized bytes.
// Not in debug builds, where this would only make every struct that has rules slow to build.
#[cfg_attr(not(debug_assertions), inline(always))]
pub(crate) const unsafe fn eval(ops: &[Op], base: *const u8, size: usize) -> Option<u128> {
  let (mut s0, mut s1, mut s2, mut s3) = (0u128, 0u128, 0u128, 0u128);
  let (mut s4, mut s5, mut s6, mut s7) = (0u128, 0u128, 0u128, 0u128);
  // A bit for each slot whose value could not be computed (an overflow, a read out of bounds).
  let (mut depth, mut failed) = (0usize, 0u8);
  macro_rules! get {
    ($slot:expr) => {
      match $slot {
        0 => s0,
        1 => s1,
        2 => s2,
        3 => s3,
        4 => s4,
        5 => s5,
        6 => s6,
        _ => s7,
      }
    };
  }
  // Set a slot to a value, or mark it as failed.
  macro_rules! set {
    ($slot:expr, $value:expr) => {{
      let (slot, value): (usize, Option<u128>) = ($slot, $value);
      match value {
        Some(value) => {
          failed &= !(1 << slot);
          match slot {
            0 => s0 = value,
            1 => s1 = value,
            2 => s2 = value,
            3 => s3 = value,
            4 => s4 = value,
            5 => s5 = value,
            6 => s6 = value,
            _ => s7 = value,
          }
        }
        None => failed |= 1 << slot,
      }
    }};
  }
  macro_rules! has_failed {
    ($slot:expr) => {
      failed & (1 << $slot) != 0
    };
  }
  macro_rules! step {
    ($op:expr) => {
      match $op {
        Op::Lit(value) => {
          set!(depth, Some(value));
          depth += 1;
        }
        Op::Load { offset, size, big } => {
          let endian = if big { Endian::Big } else { Endian::Little };
          // SAFETY: `check_rules` made sure that the field is in the sized part of the struct.
          set!(depth, Some(unsafe { load(base.add(offset), size as usize, endian) }));
          depth += 1;
        }
        Op::Size => {
          set!(depth, Some(size as u128));
          depth += 1;
        }
        Op::Not => {
          if !has_failed!(depth - 1) {
            set!(depth - 1, Some((get!(depth - 1) == 0) as u128));
          }
        }
        Op::At { size: width, big } => {
          let (offset, endian) = (get!(depth - 1), if big { Endian::Big } else { Endian::Little });
          let inside = offset <= size as u128 && size - offset as usize >= width as usize;
          if inside && !has_failed!(depth - 1) {
            // SAFETY: in the bounds of the value.
            let value = unsafe { load(base.add(offset as usize), width as usize, endian) };
            set!(depth - 1, Some(value));
          } else {
            set!(depth - 1, None);
          }
        }
        op @ (Op::Crc32 | Op::Sum16) => {
          let (start, end) = (get!(depth - 2), get!(depth - 1));
          let inside = start <= end && end <= size as u128;
          let computed = inside && !has_failed!(depth - 2) && !has_failed!(depth - 1);
          depth -= 1;
          if computed {
            // SAFETY: in the bounds of the value.
            let bytes = unsafe {
              core::slice::from_raw_parts(base.add(start as usize), (end - start) as usize)
            };
            let value = if matches!(op, Op::Crc32) { crc32(bytes) as u128 } else { sum16(bytes) as u128 };
            set!(depth - 1, Some(value));
          } else {
            set!(depth - 1, None);
          }
        }
        // What decides of `&&` and `||` is enough: the other side may have failed.
        op @ (Op::Both | Op::Either) => {
          let (a, b) = (get!(depth - 2), get!(depth - 1));
          let (failed_a, failed_b) = (has_failed!(depth - 2), has_failed!(depth - 1));
          let decides = matches!(op, Op::Either) as u128;
          depth -= 1;
          if (!failed_a && (a != 0) as u128 == decides) || (!failed_b && (b != 0) as u128 == decides) {
            set!(depth - 1, Some(decides));
          } else if failed_a || failed_b {
            set!(depth - 1, None);
          } else {
            set!(depth - 1, Some(1 - decides));
          }
        }
        op => {
          let computed = !has_failed!(depth - 2) && !has_failed!(depth - 1);
          let value = binary(op, get!(depth - 2), get!(depth - 1));
          depth -= 1;
          set!(depth - 1, if computed { value } else { None });
        }
      }
    };
  }
  macro_rules! unrolled {
    ($($index:literal)+) => {$(
      if $index < ops.len() {
        step!(ops[$index]);
      }
    )+};
  }
  unrolled!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23);
  let mut i = 24;
  while i < ops.len() {
    step!(ops[i]);
    i += 1;
  }
  let _ = (s1, s2, s3, s4, s5, s6, s7, depth);
  if failed & 1 == 0 { Some(s0) } else { None }
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
    Op::Shl => if b < 128 { Some(a << b) } else { None },
    Op::Shr => if b < 128 { Some(a >> b) } else { None },
    Op::Eq => Some((a == b) as u128),
    Op::Ne => Some((a != b) as u128),
    Op::Lt => Some((a < b) as u128),
    Op::Le => Some((a <= b) as u128),
    Op::Gt => Some((a > b) as u128),
    Op::Ge => Some((a >= b) as u128),
    Op::Lit(_) | Op::Load { .. } | Op::Size | Op::Not => None,
    Op::At { .. } | Op::Crc32 | Op::Sum16 | Op::Both | Op::Either => None,
  }
}

/// The CRC-32 of Ethernet (and zlib, PNG...): the frame check sequence of a frame is the CRC of
/// what is before it, least significant byte first.
///
/// ```
/// assert_eq!(transmutant::crc32(b"123456789"), 0xcbf43926);
/// ```
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

/// The ones' complement sum of the big-endian 16 bits words of `bytes` (a last byte alone is the
/// high byte of a word): what the checksums of IP, UDP and TCP are the complement of. Over data
/// that holds its own checksum, the sum is `0xffff`.
///
/// ```
/// assert_eq!(transmutant::sum16(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]), 0xddf2);
/// ```
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

/// Bounds of a range of valid values of an integer of the size of `T`, for `#[data(valid(..))]`.
#[doc(hidden)]
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
  unsafe { load((value as *const T).cast(), size_of::<T>(), Endian::NATIVE) }
}
