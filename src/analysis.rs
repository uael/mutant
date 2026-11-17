//! Compile-time analysis of a transmutation between two [`Layout`]s.
//!
//! The analysis answers, for a source layout `A` and a target layout `B`:
//!
//! 1. **Do they fit?** ([`Fit`]) How the size of a source maps to the size of a target, which is
//!    a matter of slice lengths when dynamically sized types are involved.
//! 2. **Is every source value a valid target value?** ([`Relation`]) Byte by byte: a target byte
//!    that must be initialized cannot be read from padding, and each constrained scalar of the
//!    target must accept whatever the source may put under it. Scalars of the target for which
//!    this cannot be proven are *marked*: they are exactly the ones a fallible transmutation has to
//!    check at run time.
//!
//! # How it scales
//!
//! Layouts are walked through a [`Cursor`] that yields *runs*: padding, unconstrained bytes or a
//! constrained scalar. Whole subtrees without constraints are a single run, whatever their size.
//!
//! Arrays and slices are periodic, so they are not walked element by element: when the target is
//! at the start of an element of a repetition and the source is inside a repetition as well, what
//! happens during the least common multiple of both strides happens again identically for as long
//! as both repetitions last. One period is analysed, the rest is skipped ([`find_skip`]). This is
//! also what makes the analysis of two slice tails, which never end, terminate.
//!
//! The cost of an analysis is therefore proportional to the number of fields of the types, not to
//! their size.

use crate::layout::{Kind, Layout, Spec, Uniform, Valid, max_value};

// -------------------------------------------------------------------------------------------------
// `Cursor`

/// Maximum nesting of arrays, slices and structs.
const MAX_DEPTH: usize = 32;

#[derive(Copy, Clone)]
pub(crate) enum RunKind {
  /// Bytes that may be uninitialized.
  Pad,
  /// Initialized bytes, any value.
  Any,
  /// A constrained scalar.
  Atom(Spec),
}

/// Consecutive bytes of the same nature: `start..end` are offsets from the root of the layout.
#[derive(Copy, Clone)]
pub(crate) struct Run {
  kind: RunKind,
  /// Only meaningful for atoms: other runs may start before `start`.
  start: usize,
  end: usize,
  /// Index of the constrained scalar, for atoms.
  leaf: usize,
}

#[derive(Copy, Clone)]
struct Frame {
  layout: &'static Layout,
  /// Offset of the layout from the root.
  base: usize,
  /// Index of the first constrained scalar of the layout.
  leaf: usize,
  /// For structs: fields before this one are behind the cursor.
  field: usize,
}

impl Frame {
  /// Offset one past the last byte of the layout, `usize::MAX` if it has no end.
  const fn end(&self) -> usize {
    if self.layout.is_dynamic() { usize::MAX } else { self.base + self.layout.size }
  }
}

/// A repetition the cursor is in: `stride` bytes repeat until `end`.
#[derive(Copy, Clone)]
struct Repeat {
  stride: usize,
  end: usize,
  /// The repetition is a run of identical bytes rather than an array or a slice.
  run: bool,
}

/// Forward-only iterator over the runs of a layout.
pub(crate) struct Cursor {
  stack: [Frame; MAX_DEPTH],
  depth: usize,
}

impl Cursor {
  pub(crate) const fn new(root: &'static Layout) -> Self {
    Self { stack: [Frame { layout: root, base: 0, leaf: 0, field: 0 }; MAX_DEPTH], depth: 1 }
  }

  const fn push(&mut self, layout: &'static Layout, base: usize, leaf: usize) {
    assert!(self.depth < MAX_DEPTH, "type is nested too deeply");
    self.stack[self.depth] = Frame { layout, base, leaf, field: 0 };
    self.depth += 1;
  }

  /// The run at `pos`, which must not be less than the previous `pos` given to the cursor.
  const fn run_at(&mut self, pos: usize) -> Run {
    loop {
      let frame = self.stack[self.depth - 1];
      let (layout, end) = (frame.layout, frame.end());
      if pos >= end {
        assert!(self.depth > 1, "internal error: analysis out of the bounds of a type");
        self.depth -= 1;
        continue;
      }
      match layout.uniform {
        Uniform::Pad => return Run { kind: RunKind::Pad, start: frame.base, end, leaf: 0 },
        Uniform::Any => return Run { kind: RunKind::Any, start: frame.base, end, leaf: 0 },
        Uniform::No => {}
      }
      match layout.kind {
        Kind::Scalar(spec) => {
          return Run { kind: RunKind::Atom(spec), start: frame.base, end, leaf: frame.leaf };
        }
        Kind::Array(elem, _) | Kind::Slice(elem) => {
          let index = (pos - frame.base) / elem.size;
          self.push(elem, frame.base + index * elem.size, frame.leaf);
        }
        Kind::Struct(fields) => {
          // Skip the fields that end before `pos`.
          let mut i = frame.field;
          while i < fields.len() {
            let field = &fields[i];
            let dynamic = field.layout.is_dynamic();
            if dynamic || frame.base + field.offset + field.layout.size > pos {
              break;
            }
            i += 1;
          }
          self.stack[self.depth - 1].field = i;
          if i == fields.len() {
            // Trailing padding.
            return Run { kind: RunKind::Pad, start: pos, end, leaf: 0 };
          }
          let start = frame.base + fields[i].offset;
          if pos < start {
            // Padding between two fields.
            return Run { kind: RunKind::Pad, start: pos, end: start, leaf: 0 };
          }
          self.push(fields[i].layout, start, frame.leaf + fields[i].leaf);
        }
      }
    }
  }

  /// The repetitions the cursor is in at `pos`, outermost first; `run` is the run at `pos`.
  ///
  /// With `aligned`, only repetitions of which `pos` is the start of an element are returned.
  const fn repeats(
    &self, pos: usize, run: &Run, aligned: bool, out: &mut [Repeat; MAX_DEPTH + 1],
  ) -> usize {
    let (mut len, mut i) = (0, 0);
    while i + 1 < self.depth {
      let frame = &self.stack[i];
      let (elem, end) = match frame.layout.kind {
        Kind::Array(elem, _) => (elem, frame.end()),
        Kind::Slice(elem) => (elem, usize::MAX),
        _ => {
          i += 1;
          continue;
        }
      };
      // The next frame is the element the cursor is in.
      if !aligned || self.stack[i + 1].base == pos {
        out[len] = Repeat { stride: elem.size, end, run: false };
        len += 1;
      }
      i += 1;
    }
    if !matches!(run.kind, RunKind::Atom(_)) {
      out[len] = Repeat { stride: 1, end: run.end, run: true };
      len += 1;
    }
    len
  }
}

// -------------------------------------------------------------------------------------------------
// `Marks`

/// A set of constrained scalars of a type, by index.
#[derive(Copy, Clone)]
pub(crate) struct Marks {
  bits: [u64; Self::CAPACITY / 64],
  /// Scalars whose index is not less than `CAPACITY` are all in the set.
  overflow: bool,
}

impl Marks {
  const CAPACITY: usize = 1024;

  pub(crate) const NONE: Self = Self { bits: [0; Self::CAPACITY / 64], overflow: false };
  pub(crate) const ALL: Self = Self { bits: [!0; Self::CAPACITY / 64], overflow: true };

  const fn set(&mut self, leaf: usize) {
    if leaf < Self::CAPACITY {
      self.bits[leaf / 64] |= 1 << (leaf % 64);
    } else {
      self.overflow = true;
    }
  }

  /// Whatever one of the `count` scalars from `first` is in the set.
  pub(crate) const fn any(&self, first: usize, count: usize) -> bool {
    let (mut i, end) = (first, first + count);
    while i < end {
      if i >= Self::CAPACITY {
        return self.overflow;
      }
      if i % 64 == 0 && end - i >= 64 {
        if self.bits[i / 64] != 0 {
          return true;
        }
        i += 64;
      } else {
        if self.bits[i / 64] & (1 << (i % 64)) != 0 {
          return true;
        }
        i += 1;
      }
    }
    false
  }
}

// -------------------------------------------------------------------------------------------------
// `Relation`

/// How the values a source may hold relate to the values a target accepts.
#[derive(Copy, Clone)]
pub(crate) struct Relation {
  /// Offset of a byte the target needs initialized that is padding in the source.
  pub(crate) uninit: Option<usize>,
  /// Offset of a scalar of the target that none of the source values is valid for.
  pub(crate) disjoint: Option<usize>,
  /// Offset of a scalar of the target that some of the source values are not valid for.
  pub(crate) partial: Option<usize>,
  /// Scalars of the target that may receive an invalid value.
  pub(crate) marks: Marks,
}

impl Relation {
  const NEW: Self = Self { uninit: None, disjoint: None, partial: None, marks: Marks::NONE };

  /// Every value of the source is a valid value of the target.
  pub(crate) const fn infallible(&self) -> bool {
    self.uninit.is_none() && self.disjoint.is_none() && self.partial.is_none()
  }
}

/// Relate the first `end` bytes (`usize::MAX`: all of them, however many) of `a` and `b`.
pub(crate) const fn relate(a: &'static Layout, b: &'static Layout, end: usize) -> Relation {
  let mut relation = Relation::NEW;
  let (mut a, mut b) = (Cursor::new(a), Cursor::new(b));
  relate_range(&mut a, &mut b, 0, end, &mut relation);
  relation
}

/// Returns `true` when the analysis is over because what follows repeats forever.
const fn relate_range(
  a: &mut Cursor, b: &mut Cursor, mut pos: usize, end: usize, out: &mut Relation,
) -> bool {
  while pos < end {
    let (run_a, run_b) = (a.run_at(pos), b.run_at(pos));
    match find_skip(a, b, pos, end, &run_a, &run_b) {
      Skip::No => {}
      Skip::To { period, next } => {
        relate_range(a, b, pos, pos + period, out);
        pos = next;
        continue;
      }
      Skip::Forever { period } => {
        relate_range(a, b, pos, pos + period, out);
        return true;
      }
    }
    let stop = min(run_b.end, end);
    match run_b.kind {
      // Padding accepts everything.
      RunKind::Pad => pos = stop,
      // Unconstrained bytes accept everything but padding.
      RunKind::Any => {
        if matches!(run_a.kind, RunKind::Pad) && out.uninit.is_none() {
          out.uninit = Some(pos);
        }
        pos = min(run_a.end, stop);
      }
      RunKind::Atom(spec) => {
        // Ranges of the analysis start and end between two elements of `b`.
        assert!(run_b.start == pos && run_b.end <= end);
        let (pieces, len) = gather(a, run_b.start, run_b.end);
        match atom_relation(&pieces, len, &spec) {
          None => {
            if out.uninit.is_none() {
              out.uninit = Some(pos);
            }
          }
          Some(Rel::Subset) => {}
          Some(rel) => {
            out.marks.set(run_b.leaf);
            if out.partial.is_none() {
              out.partial = Some(pos);
            }
            if matches!(rel, Rel::Disjoint) && out.disjoint.is_none() {
              out.disjoint = Some(pos);
            }
          }
        }
        pos = run_b.end;
      }
    }
  }
  false
}

enum Skip {
  No,
  /// What happens for the next `period` bytes repeats until `next`.
  To { period: usize, next: usize },
  /// What happens for the next `period` bytes repeats forever.
  Forever { period: usize },
}

/// Look for a period of both layouts at `pos`.
///
/// `b` must be at the start of an element of a repetition: the analysis is made of what happens
/// to each run of `b`, so a period must not cut one. `a` only has to be inside a repetition: what
/// lies under a run of `b` a period later is the same, even if runs of `a` are cut by the period.
const fn find_skip(
  a: &Cursor, b: &Cursor, pos: usize, end: usize, run_a: &Run, run_b: &Run,
) -> Skip {
  const NO: Repeat = Repeat { stride: 0, end: 0, run: false };
  let (mut repeats_a, mut repeats_b) = ([NO; MAX_DEPTH + 1], [NO; MAX_DEPTH + 1]);
  let len_b = b.repeats(pos, run_b, true, &mut repeats_b);
  if len_b == 0 {
    return Skip::No;
  }
  let len_a = a.repeats(pos, run_a, false, &mut repeats_a);
  // Outermost repetitions first: they skip the most.
  let mut i = 0;
  while i < len_b {
    let mut j = 0;
    while j < len_a {
      let (ra, rb) = (repeats_a[j], repeats_b[i]);
      j += 1;
      // Two runs are stepped over at once by `relate_range`.
      if ra.run && rb.run {
        continue;
      }
      let Some(period) = lcm(ra.stride, rb.stride) else { continue };
      let until = min(min(ra.end, rb.end), end);
      if until == usize::MAX {
        return Skip::Forever { period };
      }
      let count = (until - pos) / period;
      if count >= 2 {
        return Skip::To { period, next: pos + count * period };
      }
    }
    i += 1;
  }
  Skip::No
}

const fn min(a: usize, b: usize) -> usize {
  if a < b { a } else { b }
}

const fn gcd(a: usize, b: usize) -> usize {
  if b == 0 { a } else { gcd(b, a % b) }
}

const fn lcm(a: usize, b: usize) -> Option<usize> {
  (a / gcd(a, b)).checked_mul(b)
}

// -------------------------------------------------------------------------------------------------
// Relation of what lies under a constrained scalar with that scalar

/// Relation of a set of values `A` with a set of values `B`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Rel {
  /// `A ⊆ B`
  Subset,
  /// Neither.
  Overlap,
  /// `A ∩ B = ∅`
  Disjoint,
}

/// What `a` has under some bytes of a scalar of `b`.
#[derive(Copy, Clone)]
struct Piece {
  kind: RunKind,
  /// The bytes, as offsets in the scalar of `b`.
  from: usize,
  to: usize,
  /// For an atom of `a`: offset in that atom of its first byte under the scalar of `b`.
  skip: usize,
}

const fn gather(a: &mut Cursor, start: usize, end: usize) -> ([Piece; 16], usize) {
  let mut pieces = [Piece { kind: RunKind::Any, from: 0, to: 0, skip: 0 }; 16];
  let (mut len, mut pos) = (0, start);
  while pos < end {
    let run = a.run_at(pos);
    let to = min(run.end, end);
    let skip = if matches!(run.kind, RunKind::Atom(_)) { pos - run.start } else { 0 };
    pieces[len] = Piece { kind: run.kind, from: pos - start, to: to - start, skip };
    (len, pos) = (len + 1, to);
  }
  (pieces, len)
}

/// Relation of the values the `pieces` may hold with the values of `spec`, `None` if one of the
/// pieces may be uninitialized.
///
/// This is exact, whatever the alignment of the pieces and the byte orders involved (but for a
/// computation budget that is never reached in practice, past which it answers `Overlap`).
const fn atom_relation(pieces: &[Piece; 16], len: usize, spec: &Spec) -> Option<Rel> {
  let Valid::In(ranges) = spec.valid else { panic!("not an atom") };
  let (mut i, mut all_any) = (0, true);
  while i < len {
    match pieces[i].kind {
      RunKind::Pad => return None,
      RunKind::Any => {}
      RunKind::Atom(_) => all_any = false,
    }
    i += 1;
  }
  // The scalar is not full, and everything may come.
  if all_any {
    return Some(Rel::Overlap);
  }
  // The usual case: a scalar over a scalar of the same size and byte order.
  if len == 1 {
    if let RunKind::Atom(Spec { size, endian, valid: Valid::In(from) }) = pieces[0].kind {
      if pieces[0].skip == 0 && size == spec.size && (size == 1 || endian.equals(spec.endian)) {
        return Some(relate_ranges(from, ranges));
      }
    }
  }
  // The general case.
  let mut boxes = Boxes { pieces, len, spec, ranges, escapes: false, meets: false, budget: 1 << 14 };
  boxes.explore(0, &mut [0; 16], &mut [0xff; 16]);
  Some(match (boxes.escapes, boxes.meets) {
    (false, _) => Rel::Subset,
    (true, false) => Rel::Disjoint,
    (true, true) => Rel::Overlap,
  })
}

/// Relation of two sorted lists of disjoint and non-adjacent ranges.
const fn relate_ranges(a: &[(u128, u128)], b: &[(u128, u128)]) -> Rel {
  let (mut subset, mut meets, mut i) = (true, false, 0);
  while i < a.len() {
    let ((lo, hi), mut inside, mut j) = (a[i], false, 0);
    while j < b.len() {
      meets |= lo <= b[j].1 && b[j].0 <= hi;
      // The ranges of `b` are not adjacent: a range of `a` is in `b` if it is in one range of it.
      inside |= b[j].0 <= lo && hi <= b[j].1;
      j += 1;
    }
    subset &= inside;
    i += 1;
  }
  match (subset, meets) {
    (true, _) => Rel::Subset,
    (false, true) => Rel::Overlap,
    (false, false) => Rel::Disjoint,
  }
}

/// The values the pieces may hold are a union of *boxes*: sets of byte strings defined by a range
/// for each byte, which do not depend on any byte order. A range of integers splits into at most
/// `2 * size - 1` boxes ([`digit_box`]), and whatever a box has in common with a range of
/// integers of the target is found without enumerating anything ([`box_meets`]).
struct Boxes<'a> {
  pieces: &'a [Piece; 16],
  len: usize,
  spec: &'a Spec,
  ranges: &'a [(u128, u128)],
  /// Some values of the pieces are not valid for the scalar.
  escapes: bool,
  /// Some values of the pieces are valid for the scalar.
  meets: bool,
  budget: usize,
}

impl Boxes<'_> {
  /// For each box of the pieces from the `index`th, given a box (`lo..=hi` for each byte of the
  /// scalar, in memory order) of the pieces before.
  const fn explore(&mut self, index: usize, lo: &mut [u8; 16], hi: &mut [u8; 16]) {
    if self.escapes && self.meets {
      return;
    }
    if index == self.len {
      return self.check(lo, hi);
    }
    let piece = self.pieces[index];
    let RunKind::Atom(Spec { size, endian, valid: Valid::In(ranges) }) = piece.kind else {
      // Unconstrained bytes.
      let mut i = piece.from;
      while i < piece.to {
        (lo[i], hi[i]) = (0, 0xff);
        i += 1;
      }
      return self.explore(index + 1, lo, hi);
    };
    let mut r = 0;
    while r < ranges.len() {
      let (mut t, count) = (0, box_count(ranges[r].0, ranges[r].1, size));
      while t < count {
        if let Some((digits_lo, digits_hi)) = digit_box(ranges[r].0, ranges[r].1, size, t) {
          // Project the box on the bytes of the piece.
          let mut i = piece.from;
          while i < piece.to {
            let byte = piece.skip + (i - piece.from);
            let digit = if endian.is_little() { size - 1 - byte } else { byte };
            (lo[i], hi[i]) = (digits_lo[digit], digits_hi[digit]);
            i += 1;
          }
          self.explore(index + 1, lo, hi);
        }
        t += 1;
      }
      r += 1;
    }
  }

  /// Update `escapes` and `meets` with a box of the pieces.
  const fn check(&mut self, lo: &[u8; 16], hi: &[u8; 16]) {
    if self.budget == 0 {
      (self.escapes, self.meets) = (true, true);
      return;
    }
    self.budget -= 1;
    // Most significant digit of the scalar first.
    let (size, little) = (self.spec.size, self.spec.endian.is_little());
    let (mut digits_lo, mut digits_hi, mut i) = ([0; 16], [0; 16], 0);
    while i < size {
      let byte = if little { size - 1 - i } else { i };
      (digits_lo[i], digits_hi[i]) = (lo[byte], hi[byte]);
      i += 1;
    }
    // `gap` is the first value after the previous range.
    let (mut i, mut gap) = (0, Some(0));
    while i < self.ranges.len() {
      let (start, end) = self.ranges[i];
      self.meets = self.meets || box_meets(&digits_lo, &digits_hi, size, start, end);
      if let Some(gap) = gap {
        if gap < start {
          self.escapes = self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, start - 1);
        }
      }
      gap = if end < max_value(size) { Some(end + 1) } else { None };
      i += 1;
    }
    if let Some(gap) = gap {
      self.escapes =
        self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, max_value(size));
    }
  }
}

/// The `index`th digit, in base 256 and most significant first, of a `size` digits number.
const fn digit(value: u128, size: usize, index: usize) -> u8 {
  (value >> (8 * (size - 1 - index))) as u8
}

/// Index of the first digit that differs between `lo` and `hi`.
const fn first_difference(lo: u128, hi: u128, size: usize) -> usize {
  let mut i = 0;
  while i < size && digit(lo, size, i) == digit(hi, size, i) {
    i += 1;
  }
  i
}

/// Number of boxes `lo..=hi` is split into, see [`digit_box`].
const fn box_count(lo: u128, hi: u128, size: usize) -> usize {
  let p = first_difference(lo, hi, size);
  if p + 1 >= size { 1 } else { 1 + 2 * (size - 1 - p) }
}

/// The `index`th box of the split of `lo..=hi`: a range for each digit, most significant first.
/// Some of the boxes are empty (`None`).
///
/// With `p` the first digit that differs between `lo` and `hi`, the numbers of `lo..=hi` are:
/// - those whose digit `p` is strictly between the ones of `lo` and `hi` (first box),
/// - those that start like `lo` up to a digit `q > p`, which is greater (a box for each `q`),
/// - those that start like `hi` up to a digit `q > p`, which is lesser (a box for each `q`),
/// - `lo` and `hi`, which are put in the boxes of the last digit.
const fn digit_box(lo: u128, hi: u128, size: usize, index: usize) -> Option<([u8; 16], [u8; 16])> {
  let p = first_difference(lo, hi, size);
  let (mut box_lo, mut box_hi) = ([0; 16], [0xff; 16]);
  // The digits before `q` are the ones of `prefix`, the digits after are free.
  let (prefix, q, range);
  if p + 1 >= size {
    // `lo..=hi` is a box: only the last digit differs, if any.
    (prefix, q) = (lo, size - 1);
    range = (digit(lo, size, q), digit(hi, size, q));
  } else if index == 0 {
    (prefix, q) = (lo, p);
    if digit(hi, size, p) - digit(lo, size, p) < 2 {
      return None;
    }
    range = (digit(lo, size, p) + 1, digit(hi, size, p) - 1);
  } else if index <= size - 1 - p {
    (prefix, q) = (lo, p + index);
    let digit = digit(lo, size, q);
    if q == size - 1 {
      range = (digit, 0xff);
    } else if digit == 0xff {
      return None;
    } else {
      range = (digit + 1, 0xff);
    }
  } else {
    (prefix, q) = (hi, p + index - (size - 1 - p));
    let digit = digit(hi, size, q);
    if q == size - 1 {
      range = (0, digit);
    } else if digit == 0 {
      return None;
    } else {
      range = (0, digit - 1);
    }
  }
  let mut i = 0;
  while i < q {
    (box_lo[i], box_hi[i]) = (digit(prefix, size, i), digit(prefix, size, i));
    i += 1;
  }
  (box_lo[q], box_hi[q]) = range;
  Some((box_lo, box_hi))
}

/// Whatever a number of `start..=end` is in the box `lo..=hi` (a range for each digit, most
/// significant first).
const fn box_meets(lo: &[u8; 16], hi: &[u8; 16], size: usize, start: u128, end: u128) -> bool {
  // Look for the least number of the box that is not less than `start`: it starts like `start`
  // for as long as the box allows it.
  let mut i = 0;
  while i < size && lo[i] <= digit(start, size, i) && digit(start, size, i) <= hi[i] {
    i += 1;
  }
  // Whatever the digit at `i` is bumped (else it is, as the digits after it, the least one).
  let bump;
  if i == size {
    // `start` is in the box.
    return true;
  } else if digit(start, size, i) < lo[i] {
    // Any digit of the box is greater there.
    bump = false;
  } else {
    // Any digit of the box is lesser there: a previous digit has to be greater than in `start`.
    loop {
      if i == 0 {
        return false;
      }
      i -= 1;
      if digit(start, size, i) < hi[i] {
        break;
      }
    }
    bump = true;
  }
  let (mut least, mut k) = (0u128, 0);
  while k < size {
    let digit = match () {
      _ if k < i => digit(start, size, k),
      _ if k == i && bump => digit(start, size, k) + 1,
      _ => lo[k],
    };
    least = (least << 8) | digit as u128;
    k += 1;
  }
  least <= end
}

// -------------------------------------------------------------------------------------------------
// `Fit`

/// Whatever sources and targets have the same extent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Exact {
  Always,
  /// Depending on the slice length of the source.
  Sometimes,
  Never(&'static str),
}

/// How the extent of a source maps to the extent of a target.
///
/// The *extent* of a value is its size without the trailing padding of dynamically sized types:
/// the offset of its tail slice plus the size of that slice.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fit {
  pub(crate) exact: Exact,
  /// Whatever any source is at least as large as the smallest target.
  pub(crate) shrink: bool,
  /// Number of leading bytes to analyse, `usize::MAX` if there is no bound.
  pub(crate) end: usize,
}

pub(crate) const TOO_SMALL: &str = "the source is smaller than the target";
pub(crate) const TOO_LARGE: &str = "the source is larger than the target";
pub(crate) const UNEVEN: &str =
  "the size of the source cannot be reached with elements of the tail slice of the target";

pub(crate) const fn fit(a: &Layout, b: &Layout) -> Fit {
  let ((sa, ya), (sb, yb)) = ((a.size, a.stride()), (b.size, b.stride()));
  match (ya != 0, yb != 0) {
    // Both have a fixed size.
    (false, false) => Fit {
      exact: match () {
        _ if sa < sb => Exact::Never(TOO_SMALL),
        _ if sa > sb => Exact::Never(TOO_LARGE),
        _ => Exact::Always,
      },
      shrink: sa >= sb,
      end: sb,
    },
    // The target takes as many elements as the source has room for.
    (false, true) => {
      let elems = if sa >= sb { (sa - sb) / yb } else { 0 };
      Fit {
        exact: match () {
          _ if sa < sb => Exact::Never(TOO_SMALL),
          _ if (sa - sb) % yb != 0 => Exact::Never(UNEVEN),
          _ => Exact::Always,
        },
        shrink: sa >= sb,
        end: sb + elems * yb,
      }
    }
    // The source must have the very number of elements that makes it the size of the target.
    (true, false) => Fit {
      exact: match () {
        _ if sa > sb => Exact::Never(TOO_LARGE),
        _ if (sb - sa) % ya != 0 => Exact::Never(UNEVEN),
        _ => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: sb,
    },
    (true, true) => Fit {
      exact: match () {
        // No `sa + n * ya == sb + m * yb`.
        _ if sa.abs_diff(sb) % gcd(ya, yb) != 0 => Exact::Never(UNEVEN),
        // One for every `n`.
        _ if sa >= sb && (sa - sb) % yb == 0 && ya % yb == 0 => Exact::Always,
        _ => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: usize::MAX,
    },
  }
}

// -------------------------------------------------------------------------------------------------
// `Analysis`

#[derive(Copy, Clone)]
pub(crate) struct Analysis {
  pub(crate) fit: Fit,
  pub(crate) relation: Relation,
  pub(crate) aligned: bool,
}

pub(crate) const fn analyze(a: &'static Layout, b: &'static Layout) -> Analysis {
  let fit = fit(a, b);
  // No source fits, even truncated: there is nothing to relate.
  let hopeless = matches!(fit.exact, Exact::Never(_)) && !fit.shrink;
  Analysis {
    fit,
    relation: if hopeless { Relation::NEW } else { relate(a, b, fit.end) },
    aligned: b.align <= a.align,
  }
}

/// Whatever any value written through a `&mut` to the target `b` leaves a valid source `a`.
pub(crate) const fn mutable(a: &'static Layout, b: &'static Layout, fit: &Fit) -> bool {
  !matches!(fit.exact, Exact::Never(_)) && relate(b, a, fit.end).infallible()
}
