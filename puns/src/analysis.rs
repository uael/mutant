//! Compile-time analysis of a pun between two [`Layout`]s.
//!
//! The analysis answers, for a source layout `A` and a target layout `B`:
//!
//! 1. **Do they fit?** ([`Fit`]) How the size of a source maps to the size of a target, which is
//!    a matter of slice lengths when dynamically sized types are involved.
//! 2. **Is every source value a valid target value?** ([`Relation`]) Byte by byte: a target byte
//!    that must be initialized cannot be read from padding, and each constrained scalar of the
//!    target must accept whatever the source may put under it. Scalars of the target for which
//!    this cannot be proven are *marked*: they are exactly the ones a checked pun has to
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

use crate::layout::{Dfa, Endian, Kind, Layout, Pointer, Spec, Tags, Uniform, Valid, max_value};

// -------------------------------------------------------------------------------------------------
// `Cursor`

/// Maximum nesting of arrays, slices and structs.
const MAX_DEPTH: usize = 32;

#[derive(Copy, Clone)]
pub enum RunKind {
  /// Bytes that may be uninitialized.
  Pad,
  /// Initialized bytes, any value.
  Any,
  /// A constrained scalar.
  Atom(Spec),
  /// Bytes, until the end, that an automaton accepts.
  Pattern(&'static Dfa),
  /// A self-relative pointer. As a source, its bytes are initialized and nothing more is made of
  /// them; as a target, only a pointer proves it.
  Pointer(Pointer),
}

impl RunKind {
  /// Whatever a source may hold uninitialized bytes there.
  const fn may_be_uninit(&self) -> bool {
    match self {
      RunKind::Pad => true,
      RunKind::Atom(spec) => spec.may_be_uninit(),
      RunKind::Any | RunKind::Pattern(_) | RunKind::Pointer(_) => false,
    }
  }
}

/// Consecutive bytes of the same nature: `start..end` are offsets from the root of the layout.
#[derive(Copy, Clone)]
pub struct Run {
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
  /// What tells an array or a slice from another: its depth in the layout and its offset.
  id: (usize, usize),
}

/// Forward-only iterator over the runs of a layout.
#[derive(Copy, Clone)]
pub struct Cursor {
  stack: [Frame; MAX_DEPTH],
  depth: usize,
  /// The layout is the one of the target: a byte that is only initialized in some of the
  /// variants of an enum is then to be initialized, rather than known not to be.
  target: bool,
  /// What of the target may be checked at run time, see [`Checked`].
  checked: Checked,
  /// What the source is known to hold under some bytes of the target, whatever its own layout
  /// says there: the tag of an enum, when a variant of the target is related to a variant of the
  /// source, or to a source whose bytes there are one of the tags of that variant.
  known: Known,
}

/// The most ranges that [`Known`] holds: past them, nothing is known.
const KNOWN: usize = 8;

/// Values that the source is known to hold at `start..end`, as an integer of `endian` byte order:
/// the tag of an enum. Nothing is known if there are no ranges.
#[derive(Copy, Clone)]
struct Known {
  start: usize,
  end: usize,
  endian: Endian,
  ranges: [(u128, u128); KNOWN],
  len: usize,
}

impl Known {
  const NONE: Self =
    Self { start: 0, end: 0, endian: Endian::Little, ranges: [(0, 0); KNOWN], len: 0 };

  /// The values of both `a` and `b`, at `start..start + size`; nothing if they are too many.
  const fn both(start: usize, size: usize, endian: Endian, a: &Tags, b: &Tags) -> Self {
    let mut known = Self { start, end: start + size, endian, ..Self::NONE };
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
      let ((lo_a, hi_a), (lo_b, hi_b)) = (a.get(i), b.get(j));
      let (lo, hi) = (if lo_a > lo_b { lo_a } else { lo_b }, if hi_a < hi_b { hi_a } else { hi_b });
      if lo <= hi {
        if known.len == KNOWN {
          return Self::NONE;
        }
        known.ranges[known.len] = (lo, hi);
        known.len += 1;
      }
      if hi_a < hi_b {
        i += 1;
      } else {
        j += 1;
      }
    }
    known
  }

  /// Whatever the byte at `pos` holds a known value.
  const fn covers(&self, pos: usize) -> bool {
    self.len > 0 && self.start <= pos && pos < self.end
  }

  /// The values known of exactly `start..end`, read in `endian` byte order, if any.
  const fn of(&self, start: usize, end: usize, endian: Endian) -> Option<&[(u128, u128)]> {
    let same =
      self.start == start && self.end == end && (end - start == 1 || self.endian.equals(endian));
    if self.len > 0 && same { Some(self.ranges.split_at(self.len).0) } else { None }
  }
}

/// Which rules of a target may be checked at run time: the rules that read the bytes of their
/// struct (`le`, `crc32`...) read its padding, which must then be initialized.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Checked {
  /// None: this is not a target (the reverse relation of a `&mut`).
  No,
  /// The ones that [`mark_rules`] marks: the source does not prove them.
  Marked,
  /// All of them: the target is the variant of an enum, which is validated as a whole.
  All,
}

impl Cursor {
  /// A cursor over `root`, whose first byte is at `base`: positions are offsets from the start of
  /// the source, wherever the target is in it.
  pub(crate) const fn new(
    root: &'static Layout,
    base: usize,
    target: bool,
    checked: Checked,
  ) -> Self {
    let frame = Frame { layout: root, base, leaf: 0, field: 0 };
    Self { stack: [frame; MAX_DEPTH], depth: 1, target, checked, known: Known::NONE }
  }

  /// Whatever the padding at the top of the stack is read by a rule that may be checked: one of
  /// a struct that holds it, unless `source` proves it, as [`mark_rules`] tells.
  const fn padding_is_read(&self, source: &'static Layout) -> bool {
    let (mut i, mut marked) = (0, matches!(self.checked, Checked::All));
    if matches!(self.checked, Checked::No) {
      return false;
    }
    while i < self.depth {
      let frame = &self.stack[i];
      if !marked && covered(source, frame.base, frame.layout) {
        // Nothing in it is checked.
        return false;
      }
      if let Kind::Array(..) | Kind::Slice(_) = frame.layout.kind {
        // The elements share their rules: proven all at once, or checked everywhere.
        marked = true;
      }
      if frame.layout.rules_read_bytes() {
        return true;
      }
      i += 1;
    }
    false
  }

  /// The enum that the last run is of, if any, with its offset and its leaf.
  const fn enumeration(&self) -> Option<(&'static Layout, usize, usize)> {
    let frame = &self.stack[self.depth - 1];
    match frame.layout.kind {
      Kind::Enum(_) => Some((frame.layout, frame.base, frame.leaf)),
      _ => None,
    }
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
        Kind::Pattern(dfa) => {
          return Run { kind: RunKind::Pattern(dfa), start: frame.base, end, leaf: frame.leaf };
        }
        Kind::Pointer(pointer) => {
          let kind = RunKind::Pointer(pointer);
          return Run { kind, start: frame.base, end, leaf: frame.leaf };
        }
        Kind::Enum(variants) => {
          let Kind::Scalar(spec) = variants.tag.layout.kind else { panic!() };
          let (offset, tag) = (pos - frame.base, variants.tag.offset);
          if offset >= tag && offset < tag + spec.size {
            let kind = if spec.is_constrained() { RunKind::Atom(spec) } else { RunKind::Any };
            let (start, end) = (frame.base + tag, frame.base + tag + spec.size);
            return Run { kind, start, end, leaf: frame.leaf };
          }
          // What follows, for as long as it is known to be initialized or not.
          let checked = !matches!(self.checked, Checked::No);
          let (init, stop) = layout.segment(offset, !self.target, checked);
          let kind = if init { RunKind::Any } else { RunKind::Pad };
          return Run { kind, start: pos, end: frame.base + stop, leaf: 0 };
        }
        Kind::Struct(fields, _) => {
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
    &self,
    pos: usize,
    run: &Run,
    aligned: bool,
    out: &mut [Repeat; MAX_DEPTH + 1],
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
        out[len] = Repeat { stride: elem.size, end, run: false, id: (i, frame.base) };
        len += 1;
      }
      i += 1;
    }
    if matches!(run.kind, RunKind::Pad | RunKind::Any) {
      out[len] = Repeat { stride: 1, end: run.end, run: true, id: (usize::MAX, 0) };
      len += 1;
    }
    len
  }
}

// -------------------------------------------------------------------------------------------------
// `Marks`

/// A set of constrained scalars of a type, by index.
#[derive(Copy, Clone)]
pub struct Marks {
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
pub struct Relation {
  /// Offset of a byte the target needs initialized that is padding in the source.
  pub(crate) uninit: Option<usize>,
  /// Offset of a scalar of the target that none of the source values is valid for.
  pub(crate) disjoint: Option<usize>,
  /// Offset of a scalar of the target that some of the source values are not valid for.
  pub(crate) partial: Option<usize>,
  /// Scalars of the target that may receive an invalid value.
  pub(crate) marks: Marks,
  /// Offset of a pointer of the target that is not, in the source, a pointer whose target can be
  /// read as its own. No check at run time makes up for it: what a pointer guarantees is not a
  /// property of its bytes.
  pub(crate) pointer: Option<usize>,
}

impl Relation {
  const NEW: Self =
    Self { uninit: None, disjoint: None, partial: None, marks: Marks::NONE, pointer: None };

  const fn pointer(&mut self, pos: usize) {
    if self.pointer.is_none() {
      self.pointer = Some(pos);
    }
  }

  /// The thing at `leaf`, which is at `pos`, has to be checked at run time.
  const fn mark(&mut self, leaf: usize, pos: usize) {
    self.marks.set(leaf);
    if self.partial.is_none() {
      self.partial = Some(pos);
    }
  }

  const fn uninit(&mut self, pos: usize) {
    if self.uninit.is_none() {
      self.uninit = Some(pos);
    }
  }

  /// Every value of the source is a valid value of the target.
  pub(crate) const fn infallible(&self) -> bool {
    self.uninit.is_none()
      && self.disjoint.is_none()
      && self.partial.is_none()
      && self.pointer.is_none()
  }
}

/// Relate the first `end` bytes (`usize::MAX`: all of them, however many) of `a` and `b`.
///
/// `checked` tells which rules of `b` may be checked at run time: the bytes that they read must
/// be initialized.
pub const fn relate(
  a: &'static Layout,
  b: &'static Layout,
  end: usize,
  checked: Checked,
) -> Relation {
  relate_at(a, 0, b, 0, 0, end, checked)
}

/// Relate the bytes `start..end` (`end` may be `usize::MAX`) of `a`, which starts at `base_a`, and
/// of `b`, which starts at `base_b`: offsets are counted from wherever the outermost of the two
/// starts, and so are the offsets of the [`Relation`].
///
/// This is how a target is related to the end of a source (`b` at `size(a) - size(b)`), and the
/// part of a source that a `&mut` covers to that target (`a` there). Nothing else changes with
/// where they are: the analysis is made of what each run of `b` has under it, periods are found
/// from the strides of both (what lies under a run of `b` a period later is the same, whatever
/// the phase of the repetitions of `a` and `b`), and an enum is proven only by an enum at the
/// very same offset.
///
/// With `checked` other than [`Checked::No`], `a` must start at `0`: [`Cursor::padding_is_read`]
/// looks for what `a` has at the offsets of `b`.
pub const fn relate_at(
  a: &'static Layout,
  base_a: usize,
  b: &'static Layout,
  base_b: usize,
  start: usize,
  end: usize,
  checked: Checked,
) -> Relation {
  assert!(base_a == 0 || matches!(checked, Checked::No), "internal error: shifted source");
  let mut relation = Relation::NEW;
  // A type without values is no value of any other, whatever its bytes (it may have none).
  if !b.is_inhabited() && a.is_inhabited() {
    relation.disjoint = Some(start);
    return relation;
  }
  let mut a = Cursor::new(a, base_a, false, Checked::No);
  let mut b = Cursor::new(b, base_b, true, checked);
  relate_range(&mut a, &mut b, start, end, &mut relation);
  relation
}

/// [`relate`], knowing what the source holds at some bytes (see [`Known`]).
const fn relate_known(
  a: &'static Layout,
  b: &'static Layout,
  end: usize,
  checked: Checked,
  known: &Known,
) -> Relation {
  let mut relation = Relation::NEW;
  if !b.is_inhabited() && a.is_inhabited() {
    relation.disjoint = Some(0);
    return relation;
  }
  let mut a = Cursor::new(a, 0, false, Checked::No);
  let mut b = Cursor::new(b, 0, true, checked);
  b.known = *known;
  relate_range(&mut a, &mut b, 0, end, &mut relation);
  relation
}

/// Returns `true` when the analysis is over because what follows repeats forever.
const fn relate_range(
  a: &mut Cursor,
  b: &mut Cursor,
  mut pos: usize,
  end: usize,
  out: &mut Relation,
) -> bool {
  while pos < end {
    let (run_a, run_b) = (a.run_at(pos), b.run_at(pos));
    // Patterns go on until the end.
    if let RunKind::Pattern(dfa) = run_b.kind {
      pattern_target(a, dfa, run_b.leaf, pos, end, out);
      return true;
    }
    if let RunKind::Pattern(_) = run_a.kind {
      // The bytes of a pattern are initialized: enough for a target without constraint.
      let free = matches!(run_b.kind, RunKind::Pad | RunKind::Any) && run_b.end >= end;
      if !free || b.enumeration().is_some() {
        // Else, give up on proving anything: everything is checked at run time.
        out.marks = Marks::ALL;
        out.partial = Some(pos);
        // But a pointer, which no check makes: one that the target holds is not proven here.
        if b.stack[0].layout.has_pointers() {
          out.pointer(pos);
        }
      }
      return true;
    }
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
    // What an enum requires of its fields depends on its tag: it is proven as a whole, by an
    // enum, or not at all.
    if let Some((enum_b, base_b, leaf)) = b.enumeration() {
      if let Some((enum_a, base_a, _)) = a.enumeration()
        && base_a == pos
        && base_b == pos
        && relate_enums(enum_a, enum_b, pos, leaf, out)
      {
        pos += enum_b.size;
        continue;
      }
      // A union: one of its fields.
      if base_b == pos && pos + enum_b.size <= end && relate_union_from(a, enum_b, pos, leaf, out) {
        pos += enum_b.size;
        continue;
      }
      // From something else, variant by variant: the ones whose tags the source can hold there.
      if base_b == pos
        && pos + enum_b.size <= end
        && !enum_b.has_pointers()
        && relate_enum_from(a, enum_b, pos, leaf, &b.known, out)
      {
        pos += enum_b.size;
        continue;
      }
      // Else, what follows only tells whatever the enum can be checked at run time: which a
      // pointer in one of its variants cannot.
      out.mark(leaf, pos);
      if enum_b.has_pointers() {
        out.pointer(pos);
      }
    }
    let stop = min(run_b.end, end);
    match run_b.kind {
      // Padding that a rule reads accepts everything but padding.
      RunKind::Pad if b.padding_is_read(a.stack[0].layout) => {
        if run_a.kind.may_be_uninit() && !b.known.covers(pos) {
          out.uninit(pos);
        }
        pos = min(run_a.end, stop);
      }
      // Padding accepts everything.
      RunKind::Pad => pos = stop,
      // Unconstrained bytes accept everything but padding.
      RunKind::Any => {
        if run_a.kind.may_be_uninit() && !b.known.covers(pos) {
          out.uninit(pos);
        }
        pos = min(run_a.end, stop);
      }
      // Only some bytes of the scalar are related, and what the others hold is not known: `b` is
      // the source of a `&mut` that covers a part of it.
      RunKind::Atom(_) if run_b.start != pos || run_b.end > end => {
        out.mark(run_b.leaf, pos);
        pos = stop;
      }
      RunKind::Atom(spec) => {
        // The tag, whose values may be known.
        let relation = if let Some(known) = b.known.of(run_b.start, run_b.end, spec.endian) {
          let Valid::In(ranges) = spec.valid else { panic!("not an atom") };
          Some(relate_ranges(known, ranges))
        } else {
          let (pieces, len) = gather(a, run_b.start, run_b.end);
          atom_relation(&pieces, len, &spec)
        };
        match relation {
          None => out.uninit(pos),
          Some(Rel::Subset) => {}
          Some(rel) => {
            out.mark(run_b.leaf, pos);
            if matches!(rel, Rel::Disjoint) && out.disjoint.is_none() {
              out.disjoint = Some(pos);
            }
          }
        }
        pos = run_b.end;
      }
      // Only a pointer proves a pointer: one at the same place, whose target can be read as the
      // target of this one. Its bytes do not: whatever else the source has there is refused,
      // and so is a part of a pointer.
      RunKind::Pointer(to) => {
        match run_a.kind {
          RunKind::Pointer(from)
            if run_a.start == pos && run_b.start == pos && run_b.end <= end =>
          {
            match from.reads_as(&to) {
              Some(true) => {}
              // What is not a pointer (`None`) is not read as one: checked at run time.
              Some(false) => out.mark(run_b.leaf, pos),
              None => out.pointer(pos),
            }
          }
          _ => out.pointer(pos),
        }
        pos = stop;
      }
      RunKind::Pattern(_) => unreachable!(),
    }
  }
  false
}

/// Relate the enum `a` with the enum `b`, both at `pos`, variant by variant: each variant of `a`
/// with the ones of `b` that its tags are tags of, knowing that the tag is one of both.
///
/// Returns `false`, having done nothing, if their tags are not at the same place.
const fn relate_enums(
  a: &'static Layout,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  out: &mut Relation,
) -> bool {
  if a.equals(b) {
    return true;
  }
  let (Kind::Enum(enum_a), Kind::Enum(enum_b)) = (a.kind, b.kind) else { panic!() };
  // A union is not told by a tag: one only proves the same union.
  if enum_a.union || enum_b.union {
    return false;
  }
  let (Kind::Scalar(tag_a), Kind::Scalar(tag_b)) = (enum_a.tag.layout.kind, enum_b.tag.layout.kind)
  else {
    panic!()
  };
  if a.size < b.size
    || enum_a.tag.offset != enum_b.tag.offset
    || tag_a.size != tag_b.size
    || !tag_a.endian.equals(tag_b.endian)
  {
    return false;
  }
  // Whatever some variants of `a` are, and some are not, valid for `b`.
  let (mut some, mut all, mut i) = (false, true, 0);
  let (tag, size) = (enum_b.tag.offset, tag_b.size);
  while i < enum_a.variants.len() {
    let from = &enum_a.variants[i];
    i += 1;
    // A variant that no value is.
    if !from.is_present() {
      continue;
    }
    // Proven if all of its tags are of one variant of `b`, which it proves.
    let (tags, mut proven, mut j) = (from.tags(), false, 0);
    while j < enum_b.variants.len() {
      let to = &enum_b.variants[j];
      j += 1;
      if !tags.meets(&to.tags()) {
        continue;
      }
      // What the tag holds in `to`: the tags of both.
      let known = Known::both(tag, size, tag_b.endian, &tags, &to.tags());
      let mut relation = relate_known(from.layout, to.layout, b.size, Checked::All, &known);
      mark_rules(from.layout, to.layout, 0, 0, &mut relation);
      if let Some(offset) = relation.uninit {
        out.uninit(pos + offset);
      }
      if let Some(offset) = relation.pointer {
        out.pointer(pos + offset);
      }
      some |= relation.disjoint.is_none();
      proven |= relation.infallible() && tags.within(&to.tags());
    }
    // Else some of its tags are not of `b`, or of several variants, or the fields of one are
    // not proven.
    all &= proven;
  }
  if !all {
    out.mark(leaf, pos);
  }
  if !some && out.disjoint.is_none() {
    out.disjoint = Some(pos);
  }
  true
}

/// Relate what the source `a` holds at `pos` with the enum `b` there, which an enum of the source
/// did not prove: the tag of `b` is related as a scalar, and each variant whose tags the source
/// may hold there with what the source holds over the bytes of the enum, knowing that the tag is
/// one of those (a niche of the variant is then proven by them).
///
/// Proven if every value that the source may hold at the tag is the tag of a variant (that has
/// values), each of which is proven; else the enum is marked, and checked as a whole at run time.
/// What a variant requires to be initialized must be, if the source may be that variant.
///
/// Returns `false`, having done nothing, if the tag cannot be told apart in the source.
const fn relate_enum_from(
  a: &Cursor,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  known: &Known,
  out: &mut Relation,
) -> bool {
  let Kind::Enum(layout) = b.kind else { panic!() };
  let Kind::Scalar(tag) = layout.tag.layout.kind else { panic!() };
  if layout.union {
    return false;
  }
  let start = pos + layout.tag.offset;
  // What the source holds at the tag.
  let Some(values) = tag_values(a, start, &tag, known) else {
    // Padding where the tag is: nothing is ever this enum.
    out.uninit(start);
    return true;
  };
  let (mut some, mut all, mut i) = (false, true, 0);
  while i < layout.variants.len() {
    let variant = &layout.variants[i];
    i += 1;
    if !variant.is_present() || !values.meets(&variant.tags()) {
      continue;
    }
    let known = Known::both(start, tag.size, tag.endian, &values, &variant.tags());
    let mut relation = Relation::NEW;
    let (mut from, mut to) = (*a, Cursor::new(variant.layout, pos, true, Checked::All));
    to.known = known;
    relate_range(&mut from, &mut to, pos, pos + b.size, &mut relation);
    mark_rules(a.stack[0].layout, variant.layout, pos, 0, &mut relation);
    if let Some(offset) = relation.uninit {
      out.uninit(offset);
    }
    if let Some(offset) = relation.pointer {
      out.pointer(offset);
    }
    some |= relation.disjoint.is_none();
    all &= relation.infallible();
  }
  if !all || !all_tags(&values, layout.variants) {
    out.mark(leaf, pos);
  }
  if !some && out.disjoint.is_none() {
    out.disjoint = Some(pos);
  }
  true
}

/// Relate what the source `a` holds at `pos` with the union `b` there, which the same union of
/// the source did not prove: proven if the source proves one of its fields; else checked, field
/// by field at run time, if none of them would read a byte that may be padding in the source;
/// else refused.
///
/// Returns `false`, having done nothing, if `b` is not a union.
const fn relate_union_from(
  a: &Cursor,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  out: &mut Relation,
) -> bool {
  let Kind::Enum(layout) = b.kind else { panic!() };
  if !layout.union {
    return false;
  }
  let (mut proven, mut some, mut uninit, mut i) = (false, false, None, 0);
  while i < layout.variants.len() {
    let field = &layout.variants[i];
    i += 1;
    if !field.is_present() {
      continue;
    }
    let mut relation = Relation::NEW;
    let (mut from, mut to) = (*a, Cursor::new(field.layout, pos, true, Checked::All));
    relate_range(&mut from, &mut to, pos, pos + b.size, &mut relation);
    mark_rules(a.stack[0].layout, field.layout, pos, 0, &mut relation);
    if relation.uninit.is_none() {
      proven |= relation.infallible();
      some |= relation.disjoint.is_none();
    } else if uninit.is_none() {
      // Its check would read padding: it cannot be checked.
      uninit = relation.uninit;
    }
  }
  if proven {
    return true;
  }
  if let Some(offset) = uninit {
    out.uninit(offset);
  } else {
    out.mark(leaf, pos);
    if !some && out.disjoint.is_none() {
      out.disjoint = Some(pos);
    }
  }
  true
}

/// The values that the source `a` may hold at `start`, where a target has the scalar `tag`, as an
/// integer of its byte order: what is `known` there (the tag of an enum that holds this one in a
/// niche), else what a scalar of the source of the same size and order is known to be, else any;
/// `None` if some of these bytes may be uninitialized.
const fn tag_values(a: &Cursor, start: usize, tag: &Spec, known: &Known) -> Option<Tags> {
  if let Some(ranges) = known.of(start, start + tag.size, tag.endian) {
    // Their hull, which holds them.
    return Some(Tags::range(ranges[0].0, ranges[ranges.len() - 1].1));
  }
  let mut from = *a;
  let (pieces, len) = gather(&mut from, start, start + tag.size);
  let mut i = 0;
  while i < len {
    if pieces[i].kind.may_be_uninit() {
      return None;
    }
    i += 1;
  }
  if len == 1
    && let RunKind::Atom(Spec { size, endian, known: Valid::In(ranges), .. }) = pieces[0].kind
    && pieces[0].skip == 0
    && size == tag.size
    && (size == 1 || endian.equals(tag.endian))
  {
    return Some(Tags::of(ranges));
  }
  Some(Tags::range(0, max_value(tag.size)))
}

/// Whatever every value of `values` is a tag of one of `variants` that has values.
const fn all_tags(values: &Tags, variants: &[crate::layout::Variant]) -> bool {
  let mut r = 0;
  while r < values.len() {
    let (mut next, hi) = values.get(r);
    r += 1;
    loop {
      // The range of the tags of a variant that holds `next`, if any.
      let (mut found, mut i) = (None, 0);
      while i < variants.len() && found.is_none() {
        let tags = variants[i].tags();
        let mut j = 0;
        while variants[i].is_present() && j < tags.len() {
          let (lo, up) = tags.get(j);
          if lo <= next && next <= up {
            found = Some(up);
          }
          j += 1;
        }
        i += 1;
      }
      match found {
        None => return false,
        Some(up) if up >= hi => break,
        Some(up) => next = up + 1,
      }
    }
  }
  true
}

const NO: Repeat = Repeat { stride: 0, end: 0, run: false, id: (0, 0) };

enum Skip {
  No,
  /// What happens for the next `period` bytes repeats until `next`.
  To {
    period: usize,
    next: usize,
  },
  /// What happens for the next `period` bytes repeats forever.
  Forever {
    period: usize,
  },
}

/// Look for a period of both layouts at `pos`.
///
/// `b` must be at the start of an element of a repetition: the analysis is made of what happens
/// to each run of `b`, so a period must not cut one. `a` only has to be inside a repetition: what
/// lies under a run of `b` a period later is the same, even if runs of `a` are cut by the period.
const fn find_skip(
  a: &Cursor,
  b: &Cursor,
  pos: usize,
  end: usize,
  run_a: &Run,
  run_b: &Run,
) -> Skip {
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
pub enum Rel {
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
    let mut run = a.run_at(pos);
    // The bytes of a pointer are initialized, and that is all that is made of them here.
    if let RunKind::Pointer(_) = run.kind {
      run.kind = RunKind::Any;
    }
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
  // What is known of the pieces: a restriction that the type system does not enforce only
  // proves a restriction of the same kind.
  let (mut known, soft) = (*pieces, spec.is_soft());
  let (mut i, mut all_any) = (0, true);
  while i < len {
    match known[i].kind {
      RunKind::Pad => return None,
      // Whatever a pattern accepts is initialized, and that is all that is made of it here.
      RunKind::Any | RunKind::Pattern(_) | RunKind::Pointer(_) => {}
      // Required to be some values, but a source may hold anything there, padding included.
      RunKind::Atom(from) if from.may_be_uninit() => return None,
      RunKind::Atom(from) => match if soft { from.valid } else { from.known } {
        valid @ Valid::In(_) => {
          known[i].kind = RunKind::Atom(Spec { valid, ..from });
          all_any = false;
        }
        _ => known[i].kind = RunKind::Any,
      },
    }
    i += 1;
  }
  let pieces = &known;
  // The scalar is not full, and everything may come.
  if all_any {
    return Some(Rel::Overlap);
  }
  // The usual case: a scalar over a scalar of the same size and byte order.
  if len == 1
    && let RunKind::Atom(Spec { size, endian, valid: Valid::In(from), .. }) = pieces[0].kind
    && pieces[0].skip == 0
    && size == spec.size
    && (size == 1 || endian.equals(spec.endian))
  {
    return Some(relate_ranges(from, ranges));
  }
  // The general case.
  let mut boxes =
    Boxes { pieces, len, spec, ranges, escapes: false, meets: false, budget: 1 << 14 };
  boxes.explore(0, &mut [0; 16], &mut [0xff; 16]);
  Some(match (boxes.escapes, boxes.meets) {
    (false, _) => Rel::Subset,
    (true, false) => Rel::Disjoint,
    (true, true) => Rel::Overlap,
  })
}

/// Relation of two sorted lists of disjoint and non-adjacent ranges.
pub const fn relate_ranges(a: &[(u128, u128)], b: &[(u128, u128)]) -> Rel {
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
    let RunKind::Atom(Spec { size, endian, valid: Valid::In(ranges), .. }) = piece.kind else {
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
      if let Some(gap) = gap
        && gap < start
      {
        self.escapes = self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, start - 1);
      }
      gap = if end < max_value(size) { Some(end + 1) } else { None };
      i += 1;
    }
    if let Some(gap) = gap {
      self.escapes = self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, max_value(size));
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

  let bump = if i == size {
    // `start` is in the box.
    return true;
  } else if digit(start, size, i) < lo[i] {
    // Any digit of the box is greater there.
    false
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
    true
  };
  let (mut least, mut k) = (0u128, 0);
  while k < size {
    let digit = match () {
      () if k < i => digit(start, size, k),
      () if k == i && bump => digit(start, size, k) + 1,
      () => lo[k],
    };
    least = (least << 8) | digit as u128;
    k += 1;
  }
  least <= end
}

// -------------------------------------------------------------------------------------------------
// Patterns

/// Relate what `a` has from `pos` with the pattern `dfa`, which is the thing at `leaf` of the
/// target: the automaton is run over the runs of `a`, with a set of states.
///
/// Arrays and slices of `a` are skipped as soon as an element leaves the set of states as it
/// found it.
const fn pattern_target(
  a: &mut Cursor,
  dfa: &'static Dfa,
  leaf: usize,
  mut pos: usize,
  end: usize,
  out: &mut Relation,
) {
  // The automaton is in one of the states of `set`.
  let mut set = 1u64;
  // The `set` at the start of the previous element of the repetitions of `a`.
  let mut memo = [((0, 0), 0u64); MAX_DEPTH + 1];
  let mut memo_len = 0;
  let mut budget = 1usize << 12;
  'runs: while pos < end {
    if budget == 0 {
      return out.mark(leaf, pos);
    }
    budget -= 1;
    let run = a.run_at(pos);
    let mut repeats = [NO; MAX_DEPTH + 1];
    let (len, mut i) = (a.repeats(pos, &run, true, &mut repeats), 0);
    while i < len {
      let repeat = repeats[i];
      i += 1;
      if repeat.run {
        continue;
      }
      // A slice can end before any of its elements.
      if repeat.end == usize::MAX && !dfa.accepts(set) {
        return out.mark(leaf, pos);
      }
      let mut j = 0;
      while j < memo_len && !(memo[j].0.0 == repeat.id.0 && memo[j].0.1 == repeat.id.1) {
        j += 1;
      }
      if j < memo_len && memo[j].1 == set {
        // An element changes nothing: neither will the next ones.
        pos = min(repeat.end, end);
        continue 'runs;
      }
      memo[j] = (repeat.id, set);
      if j == memo_len {
        memo_len += 1;
      }
    }
    let stop = min(run.end, end);
    match run.kind {
      RunKind::Pad => return out.uninit(pos),
      RunKind::Any | RunKind::Pointer(_) => {
        // Any byte, `stop - pos` times, or until it changes nothing.
        let mut steps = 0;
        while steps < stop - pos {
          let (next, total) = dfa.step(set, 0, 0xff);
          // The source can end anywhere if nothing bounds the analysis.
          if !total || steps > 256 || (end == usize::MAX && !dfa.accepts(next)) {
            return out.mark(leaf, pos);
          }
          if next == set {
            break;
          }
          (set, steps) = (next, steps + 1);
        }
      }
      RunKind::Atom(spec) if spec.may_be_uninit() => return out.uninit(pos),
      RunKind::Atom(spec) => {
        // Each box of the scalar goes through the automaton on its own. Only what the type
        // system enforces is known of it.
        let ranges: &[(u128, u128)] = match spec.known {
          Valid::In(ranges) => ranges,
          _ => &[(0, max_value(spec.size))],
        };
        let (mut result, mut r) = (0, 0);
        while r < ranges.len() {
          let (mut t, count) = (0, box_count(ranges[r].0, ranges[r].1, spec.size));
          while t < count {
            if let Some((lo, hi)) = digit_box(ranges[r].0, ranges[r].1, spec.size, t) {
              let (mut states, mut byte) = (set, pos - run.start);
              while byte < stop - run.start {
                let digit = if spec.endian.is_little() { spec.size - 1 - byte } else { byte };
                let (next, total) = dfa.step(states, lo[digit], hi[digit]);
                if !total {
                  return out.mark(leaf, pos);
                }
                (states, byte) = (next, byte + 1);
              }
              result |= states;
            }
            t += 1;
          }
          r += 1;
        }
        set = result;
      }
      RunKind::Pattern(source) => {
        // The source may be anywhere in its own pattern if it started before.
        let from = if run.start == pos { 1 } else { source.reachable() };
        if !pattern_product(source, from, dfa, set) {
          out.mark(leaf, pos);
        }
        return;
      }
    }
    pos = stop;
  }
  // The source ends here.
  if end != usize::MAX && !dfa.accepts(set) {
    out.mark(leaf, pos);
  }
}

/// Whatever every string that the automaton `a` accepts from one of the states `from_a` is
/// accepted by the automaton `b` from any of the states `from_b`.
const fn pattern_product(a: &Dfa, from_a: u64, b: &Dfa, from_b: u64) -> bool {
  // For each state of `a`, the states of `b` it can be paired with.
  let (mut seen, mut done) = ([0u64; 64], [0u64; 64]);
  let mut state = 0;
  while state < a.states.len() {
    if from_a & (1 << state) != 0 {
      seen[state] = from_b;
    }
    state += 1;
  }
  loop {
    let (mut changed, mut state) = (false, 0);
    while state < a.states.len() {
      let todo = seen[state] & !done[state];
      if todo != 0 {
        (done[state], changed) = (done[state] | todo, true);
        if a.states[state].accept && !b.accepts(todo) {
          return false;
        }
        let (edges, mut i) = (a.states[state].edges, 0);
        while i < edges.len() {
          let (next, total) = b.step(todo, edges[i].0, edges[i].1);
          if !total {
            return false;
          }
          seen[edges[i].2 as usize] |= next;
          i += 1;
        }
      }
      state += 1;
    }
    if !changed {
      return true;
    }
  }
}

// -------------------------------------------------------------------------------------------------
// Rules

/// Mark the structs of `target`, which is at `base` in the target and whose first leaf is `leaf`,
/// whose rules the `source` does not prove.
///
/// A rule is not a property of bytes: it is only proven by the same rule, in the same struct, at
/// the same place of the source.
const fn mark_rules(
  source: &'static Layout,
  target: &'static Layout,
  base: usize,
  leaf: usize,
  out: &mut Relation,
) {
  if !target.has_rules || covered(source, base, target) {
    return;
  }
  match target.kind {
    Kind::Struct(fields, rules) => {
      if !rules.is_empty() {
        out.mark(leaf + target.leaves - 1, base);
      }
      let mut i = 0;
      while i < fields.len() {
        mark_rules(source, fields[i].layout, base + fields[i].offset, leaf + fields[i].leaf, out);
        i += 1;
      }
    }
    // Elements share their leaves: either they are all proven at once (`covered`), or none is.
    Kind::Array(elem, _) | Kind::Slice(elem) => mark_all_rules(elem, base, leaf, out),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => {}
  }
}

const fn mark_all_rules(target: &'static Layout, base: usize, leaf: usize, out: &mut Relation) {
  if !target.has_rules {
    return;
  }
  match target.kind {
    Kind::Struct(fields, rules) => {
      if !rules.is_empty() {
        out.mark(leaf + target.leaves - 1, base);
      }
      let mut i = 0;
      while i < fields.len() {
        mark_all_rules(fields[i].layout, base + fields[i].offset, leaf + fields[i].leaf, out);
        i += 1;
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => mark_all_rules(elem, base, leaf, out),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => {}
  }
}

/// Whatever `source` has, at `offset`, the very thing that `target` is.
const fn covered(source: &'static Layout, offset: usize, target: &'static Layout) -> bool {
  if offset == 0 {
    if source.equals(target) {
      return true;
    }
    // As many of the same elements, or more.
    let same = match (source.kind, target.kind) {
      (Kind::Array(a, n), Kind::Array(b, m)) => n >= m && a.equals(b),
      (Kind::Slice(a), Kind::Slice(b) | Kind::Array(b, _)) => a.equals(b),
      _ => false,
    };
    if same {
      return true;
    }
  }
  match source.kind {
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        let field = &fields[i];
        let inside = field.layout.is_dynamic() || offset < field.offset + field.layout.size;
        if offset >= field.offset && inside {
          return covered(field.layout, offset - field.offset, target);
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, len) => {
      elem.size > 0 && offset / elem.size < len && covered(elem, offset % elem.size, target)
    }
    Kind::Slice(elem) => elem.size > 0 && covered(elem, offset % elem.size, target),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => false,
  }
}

// -------------------------------------------------------------------------------------------------
// `Fit`

/// Whatever sources and targets have the same extent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Exact {
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
pub struct Fit {
  pub(crate) exact: Exact,
  /// Whatever any source is at least as large as the smallest target.
  pub(crate) shrink: bool,
  /// Number of leading bytes to analyse, `usize::MAX` if there is no bound.
  pub(crate) end: usize,
}

pub const TOO_SMALL: &str = "the source is smaller than the target";
pub const TOO_LARGE: &str = "the source is larger than the target";
pub const UNEVEN: &str =
  "the size of the source cannot be reached with elements of the tail slice of the target";

pub const fn fit(a: &Layout, b: &Layout) -> Fit {
  let ((sa, ya), (sb, yb)) = ((a.size, a.stride()), (b.size, b.stride()));
  match (ya != 0, yb != 0) {
    // Both have a fixed size.
    (false, false) => Fit {
      exact: match () {
        () if sa < sb => Exact::Never(TOO_SMALL),
        () if sa > sb => Exact::Never(TOO_LARGE),
        () => Exact::Always,
      },
      shrink: sa >= sb,
      end: sb,
    },
    // The target takes as many elements as the source has room for.
    (false, true) => {
      let elems = if sa >= sb { (sa - sb) / yb } else { 0 };
      Fit {
        exact: match () {
          () if sa < sb => Exact::Never(TOO_SMALL),
          () if (sa - sb) % yb != 0 => Exact::Never(UNEVEN),
          () => Exact::Always,
        },
        shrink: sa >= sb,
        end: sb + elems * yb,
      }
    }
    // The source must have the very number of elements that makes it the size of the target.
    (true, false) => Fit {
      exact: match () {
        () if sa > sb => Exact::Never(TOO_LARGE),
        () if (sb - sa) % ya != 0 => Exact::Never(UNEVEN),
        () => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: sb,
    },
    (true, true) => Fit {
      exact: match () {
        // No `sa + n * ya == sb + m * yb`.
        () if sa.abs_diff(sb) % gcd(ya, yb) != 0 => Exact::Never(UNEVEN),
        // One for every `n`.
        () if sa >= sb && (sa - sb) % yb == 0 && ya % yb == 0 => Exact::Always,
        () => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: usize::MAX,
    },
  }
}

// -------------------------------------------------------------------------------------------------
// `Analysis`

#[derive(Copy, Clone)]
pub struct Analysis {
  pub(crate) fit: Fit,
  /// Its offsets are from the start of the source: the target is at `shift`.
  pub(crate) relation: Relation,
  /// The target is aligned wherever the source is.
  pub(crate) aligned: bool,
  /// Offset of the target in the source: `0`, or for the last bytes of the source (`tail`), the
  /// size of the source minus the size of the target.
  pub(crate) shift: usize,
}

pub const fn analyze(a: &'static Layout, b: &'static Layout) -> Analysis {
  let fit = fit(a, b);
  // No source fits, even truncated: there is nothing to relate.
  let hopeless = matches!(fit.exact, Exact::Never(_)) && !fit.shrink;
  let mut relation = if hopeless { Relation::NEW } else { relate(a, b, fit.end, Checked::Marked) };
  if !hopeless {
    mark_rules(a, b, 0, 0, &mut relation);
  }
  Analysis { fit, relation, aligned: b.align <= a.align, shift: 0 }
}

/// The source is unsized: where its last bytes start depends on its length.
pub const TAIL_UNSIZED_SOURCE: &str = "the source is unsized: where its last bytes start depends \
                                       on its length, which is only known at run time.\nUse \
                                       `from_tail` to take a value from the end of bytes";
/// The target is unsized: how many bytes it takes from the end cannot be told.
pub const TAIL_UNSIZED_TARGET: &str =
  "the target is unsized: its extent cannot be found from the end of the source";

/// Relate the last bytes of `a` with `b`: the target at `shift = size(a) - size(b)` in the
/// source. Both must be sized (else the analysis says so, with nothing to relate).
///
/// What is checked is what [`analyze`] checks of the first bytes: padding under a byte that the
/// target needs, scalars, patterns, enums (an enum only proves an enum at the same offset), and
/// rules (only proven by the same rule at the same offset: [`mark_rules`] from `shift`).
pub const fn analyze_tail(a: &'static Layout, b: &'static Layout) -> Analysis {
  if a.is_unsized() || b.is_unsized() || a.size < b.size {
    let exact = Exact::Never(match () {
      () if a.is_unsized() => TAIL_UNSIZED_SOURCE,
      () if b.is_unsized() => TAIL_UNSIZED_TARGET,
      () => TOO_SMALL,
    });
    let fit = Fit { exact, shrink: false, end: 0 };
    return Analysis { fit, relation: Relation::NEW, aligned: false, shift: 0 };
  }
  let shift = a.size - b.size;
  let exact = if shift == 0 { Exact::Always } else { Exact::Never(TOO_LARGE) };
  let fit = Fit { exact, shrink: true, end: b.size };
  let mut relation = relate_at(a, 0, b, shift, shift, a.size, Checked::Marked);
  mark_rules(a, b, shift, 0, &mut relation);
  // The source is at a multiple of its alignment: the target is at a multiple of its own if both
  // the alignment of the source and the shift are.
  let aligned = b.align <= a.align && shift.is_multiple_of(b.align);
  Analysis { fit, relation, aligned, shift }
}

/// Whatever any value written through a `&mut` to the target `b` leaves a valid source `a`.
///
/// Never with a self-relative pointer on either side: through a `&mut`, safe code could write
/// over it, swap it with another or move it elsewhere, where it would aim at something else.
pub const fn mutable(a: &'static Layout, b: &'static Layout, fit: &Fit) -> bool {
  !a.has_pointers()
    && !b.has_pointers()
    && !matches!(fit.exact, Exact::Never(_))
    && relate(b, a, fit.end, Checked::No).infallible()
}

pub const POINTER_MUT: &str = "the source or the target holds a self-relative pointer, which a \
                               `&mut` would let safe code overwrite, swap or move: a pointer is \
                               only read.\nEdit the value where it is, in its region, with what \
                               made the pointer (the handles of `nearest::Region::edit`)";

pub const MUTABLE: &str = "some values of the target are not valid for the source (or are padding \
                           where the source is not), and could be written through the `&mut`";
pub const PART_RULE: &str = "a rule of the source (a size, a length, an assertion, a region) \
                             holds over bytes that the `&mut` covers, and could stop holding when \
                             they are written";
pub const PART_ENUM: &str = "an enum of the source is only partly covered by the `&mut`: what its \
                             tag says of the bytes on one side depends on the other";
pub const PART_PATTERN: &str = "a string of the source is only partly covered by the `&mut`: what \
                                is written must be valid with what follows it";

/// Whatever any value written through a `&mut` to the target `b`, which covers the bytes of `a`
/// from `shift` (as many as `fit` tells), leaves a valid source `a`: why not, if it does not.
///
/// What is written is any value of `b`, so `b` must be included in what `a` accepts over those
/// bytes (the reverse relation, as [`mutable`] requires of the whole). And nothing of `a` outside
/// those bytes may depend on them: a rule of a struct that holds over them, an enum of which they
/// are only a part (its tag on one side, fields on the other), or a string that goes on after
/// them. This is conservative: such a source is refused even if no value of `b` would break it.
pub const fn mutable_part(
  a: &'static Layout,
  b: &'static Layout,
  fit: &Fit,
  shift: usize,
) -> Option<&'static str> {
  if !fit.shrink {
    // Refused by the analysis of the reference already.
    return Some(MUTABLE);
  }
  // Wherever the pointer is: a part that does not cover it is refused as well.
  if a.has_pointers() || b.has_pointers() {
    return Some(POINTER_MUT);
  }
  let end = shift.saturating_add(fit.end);
  if let Some(why) = cut(a, 0, shift, end) {
    return Some(why);
  }
  // A target that takes as many elements as fit may end anywhere in the source: within an enum
  // or a string, which `cut` cannot see without an end.
  if end == usize::MAX && !matches!(fit.exact, Exact::Always) && is_tagged(a) {
    return Some(PART_ENUM);
  }
  if relate_at(b, shift, a, 0, shift, end, Checked::No).infallible() { None } else { Some(MUTABLE) }
}

/// Why something of `layout`, which is at `base`, depends on the bytes `lo..hi` (`hi` may be
/// `usize::MAX`) but does not lie within them, if something does: see [`mutable_part`].
const fn cut(layout: &'static Layout, base: usize, lo: usize, hi: usize) -> Option<&'static str> {
  let end = if layout.is_dynamic() { usize::MAX } else { base + layout.size };
  if end <= lo || base >= hi || base == end {
    return None;
  }
  match layout.kind {
    // No `&mut` covers a pointer, see `mutable_part`.
    Kind::Scalar(_) | Kind::Pointer(_) => None,
    Kind::Struct(fields, rules) => {
      if !rules.is_empty() {
        return Some(PART_RULE);
      }
      let mut i = 0;
      while i < fields.len() {
        if let Some(why) = cut(fields[i].layout, base + fields[i].offset, lo, hi) {
          return Some(why);
        }
        i += 1;
      }
      None
    }
    Kind::Enum(enumeration) => {
      if base < lo || end > hi {
        return Some(PART_ENUM);
      }
      // Within the bytes: a variant is written whole, but some may have rules.
      let mut i = 0;
      while i < enumeration.variants.len() {
        if has_rules(enumeration.variants[i].layout) {
          return Some(PART_RULE);
        }
        i += 1;
      }
      None
    }
    Kind::Pattern(_) => {
      if base < lo || hi != usize::MAX {
        Some(PART_PATTERN)
      } else {
        None
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => {
      if elem.size == 0 {
        return None;
      }
      if has_rules(elem) {
        // Some element is covered, if only in part: its rules hold over what is covered.
        return Some(PART_RULE);
      }
      if !is_tagged(elem) {
        return None;
      }
      // Only the elements at either end of the bytes may be cut.
      let count = match layout.kind {
        Kind::Array(_, count) => count,
        _ => usize::MAX,
      };
      let first = if lo > base { (lo - base) / elem.size } else { 0 };
      if first < count
        && let Some(why) = cut(elem, base + first * elem.size, lo, hi)
      {
        return Some(why);
      }
      if hi != usize::MAX {
        let last = (hi - 1 - base) / elem.size;
        if last < count
          && last != first
          && let Some(why) = cut(elem, base + last * elem.size, lo, hi)
        {
          return Some(why);
        }
      }
      None
    }
  }
}

/// Whatever a struct of `layout` has rules, the variants of its enums included.
const fn has_rules(layout: &'static Layout) -> bool {
  match layout.kind {
    _ if layout.has_rules => true,
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if has_rules(fields[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => has_rules(elem),
    Kind::Enum(enumeration) => {
      let mut i = 0;
      while i < enumeration.variants.len() {
        if has_rules(enumeration.variants[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Pointer(_) => false,
  }
}

/// Whatever `layout` has an enum with fields or a pattern: bytes whose validity depends on other
/// bytes.
const fn is_tagged(layout: &'static Layout) -> bool {
  match layout.kind {
    Kind::Enum(_) | Kind::Pattern(_) => true,
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if is_tagged(fields[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => is_tagged(elem),
    Kind::Scalar(_) | Kind::Pointer(_) => false,
  }
}
