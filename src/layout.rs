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
  pub(crate) valid: Valid,
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
    Self { size: size_of::<T>(), endian: Endian::NATIVE, valid: Valid::Any }
  }

  /// A scalar the size of `T` that accepts everything, uninitialized bytes included.
  #[inline]
  pub const fn uninit<T>() -> Self {
    Self { size: size_of::<T>(), endian: Endian::NATIVE, valid: Valid::Uninit }
  }

  /// A scalar the size of `T` whose value, read as an unsigned integer in `endian` byte order,
  /// must be in one of `ranges`.
  ///
  /// `ranges` are inclusive and must be sorted, disjoint and non-adjacent; this is checked at
  /// compile time. See [`sorted_ranges`] to build them from a list of values.
  pub const fn ranges<T>(endian: Endian, ranges: &'static [(u128, u128)]) -> Self {
    let size = size_of::<T>();
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
    Self { size, endian, valid: if full { Valid::Any } else { Valid::In(ranges) } }
  }

  /// Same as `self`, read in another byte order.
  #[inline]
  pub const fn with_endian(self, endian: Endian) -> Self {
    Self { endian, ..self }
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
/// use mutant::layout::sorted_ranges;
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
  Struct(&'static [Field]),
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
}

impl Field {
  #[inline]
  pub const fn new(offset: usize, layout: &'static Layout) -> Self {
    Self { offset, layout, index: 0, leaf: 0 }
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
  /// Number of constrained scalars (array and slice elements are counted once).
  pub(crate) leaves: usize,
  pub(crate) uniform: Uniform,
}

/// What [`validate`](crate::validate) has to do for a type.
#[derive(Copy, Clone)]
pub(crate) enum Shape {
  Scalar(Spec),
  Array(usize),
  Slice,
  Struct,
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
    }
  }

  /// Layout of a type the size and alignment of `T` whose bytes are all padding.
  pub const fn padding<T>() -> Self {
    Self {
      kind: Kind::Struct(&[]),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 0,
      uniform: Uniform::Pad,
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
    }
  }

  /// Layout of the struct `T`.
  ///
  /// - `probe` is `T::PROBE`,
  /// - `fields` are the fields of `T` as returned by [`Layout::sort_fields`].
  ///
  /// The size and alignment of `T` come from the compiler. If the last field is unsized, so is
  /// `T`, and the way the compiler sizes `T` is checked against the model used by this crate.
  pub const fn structure<T: ?Sized>(probe: *const T, fields: &'static [Field]) -> Self {
    let (mut i, mut end, mut leaves) = (0, 0, 0);
    let (mut all_any, mut all_pad) = (true, true);
    let mut tail: Option<&Field> = None;
    while i < fields.len() {
      let field = &fields[i];
      assert!(tail.is_none(), "only the last field of a struct can be unsized");
      leaves += field.layout.leaves;
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
        Self { kind: Kind::Struct(fields), size, align, tail: 0, unsized_: false, leaves, uniform }
      }
      Some(field) => {
        // SAFETY: the tail of `T` is a slice (the tail of its last field is).
        unsafe { ptr::verify::<T>(probe) };
        // SAFETY: the metadata of `probe` is a small slice length.
        let align = unsafe { mem::align_of_val_raw(probe) };
        let this = Self {
          kind: Kind::Struct(fields),
          size: field.offset + field.layout.size,
          align,
          tail: field.layout.tail,
          unsized_: true,
          leaves,
          uniform: Uniform::No,
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
    let Kind::Struct(fields) = self.kind else { panic!("not a struct") };
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
      Kind::Struct(_) => Shape::Struct,
    }
  }

  /// Whatever the all-zeros bit pattern is valid.
  pub(crate) const fn zeroable(&self) -> bool {
    match self.kind {
      Kind::Scalar(spec) => spec.contains(0),
      Kind::Array(elem, len) => len == 0 || elem.zeroable(),
      Kind::Slice(elem) => elem.zeroable(),
      Kind::Struct(fields) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.zeroable() {
            return false;
          }
          i += 1;
        }
        true
      }
    }
  }
}
