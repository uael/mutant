//! What the values of an expression of a rule may be, known at compile time, so that what fits a
//! register (64 bits, or 32 on a target of 32 bits) is computed in it: with the same results and
//! the same failures as on `u128`, which the expression means (see "Narrowing" in `DESIGN.md`).
//!
//! Each value of an expression is known to lie in a [`Range`], from the widths of what it reads
//! and the operations that make it. For each operation, a [`How`] says in what width it is
//! computed, whatever it may fail (an addition that cannot overflow is not tested), whatever the
//! halves of a `u128` are computed apart (what is added to the low half does not carry), and what
//! its result fits: the evaluator (`layout::Machine`) truncates every value to that width, which
//! changes nothing of it and tells the optimizer what it is where it could not know, in the loop
//! of a walk. A walk is known by a fixed point of what its steps make of its state.

use crate::{
  layout::{self, MAX_OPS, Op, STACK, VARIABLES, Varint},
  steps::Entry,
};

/// The largest size of a value in bytes, which Rust allows no value to exceed: what an offset, a
/// length or the end of a range that a rule reads at is at most, once it is read.
const SIZE_MAX: u128 = isize::MAX as u128;

/// What a value may be: `hi << 64 | lo`, for any `hi` in `hi.0..=hi.1` and any `lo` in
/// `lo.0..=lo.1`.
///
/// Two halves rather than one interval: the state of a walk may be two numbers in a `u128`, one in
/// each half (see "Walks" in `DESIGN.md`), and what is added to its low half is computed there
/// alone where it is known not to carry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Range {
  hi: (u64, u64),
  lo: (u64, u64),
}

/// What the variables of an expression may be.
pub type Vars = [Range; VARIABLES];

const fn min64(a: u64, b: u64) -> u64 {
  if a < b { a } else { b }
}

const fn max64(a: u64, b: u64) -> u64 {
  if a > b { a } else { b }
}

const fn min128(a: u128, b: u128) -> u128 {
  if a < b { a } else { b }
}

/// Every bit up to the highest one of `x`.
const fn ones(x: u64) -> u64 {
  if x == 0 { 0 } else { u64::MAX >> x.leading_zeros() }
}

impl Range {
  /// Any value.
  pub const ANY: Range = Range { hi: (0, u64::MAX), lo: (0, u64::MAX) };

  /// The values `min..=max`.
  pub const fn new(min: u128, max: u128) -> Range {
    let (a, b) = ((min >> 64) as u64, (max >> 64) as u64);
    if a == b {
      Range { hi: (a, a), lo: (min as u64, max as u64) }
    } else {
      Range { hi: (a, b), lo: (0, u64::MAX) }
    }
  }

  /// The value `value`.
  pub const fn exact(value: u128) -> Range {
    Range::new(value, value)
  }

  /// The values of an unsigned integer of `bytes` bytes.
  const fn bytes(bytes: u8) -> Range {
    if bytes >= 16 { Range::ANY } else { Range::new(0, (1 << (8 * bytes as u32)) - 1) }
  }

  /// A value that is not more than any.
  pub const fn min(self) -> u128 {
    (self.hi.0 as u128) << 64 | self.lo.0 as u128
  }

  /// A value that is not less than any.
  pub const fn max(self) -> u128 {
    (self.hi.1 as u128) << 64 | self.lo.1 as u128
  }

  /// Whatever `value` is one of these.
  #[cfg(test)]
  pub const fn holds(self, value: u128) -> bool {
    let (hi, lo) = ((value >> 64) as u64, value as u64);
    self.hi.0 <= hi && hi <= self.hi.1 && self.lo.0 <= lo && lo <= self.lo.1
  }

  /// The halves, from tests: `hi << 64 | lo` for `hi` in `hi.0..=hi.1` and `lo` in `lo.0..=lo.1`.
  #[cfg(test)]
  pub const fn halves(hi: (u64, u64), lo: (u64, u64)) -> Range {
    Range { hi, lo }
  }

  /// What is either.
  const fn join(self, other: Range) -> Range {
    Range {
      hi: (min64(self.hi.0, other.hi.0), max64(self.hi.1, other.hi.1)),
      lo: (min64(self.lo.0, other.lo.0), max64(self.lo.1, other.lo.1)),
    }
  }

  const fn same(self, other: Range) -> bool {
    self.hi.0 == other.hi.0
      && self.hi.1 == other.hi.1
      && self.lo.0 == other.lo.0
      && self.lo.1 == other.lo.1
  }

  /// What `self` was found to be after it was taken to `to`: each bound that moved goes as far as
  /// it can, so that a fixed point is reached after a few steps.
  const fn widen(self, to: Range) -> Range {
    Range {
      hi: (
        if to.hi.0 < self.hi.0 { 0 } else { self.hi.0 },
        if to.hi.1 > self.hi.1 { u64::MAX } else { self.hi.1 },
      ),
      lo: (
        if to.lo.0 < self.lo.0 { 0 } else { self.lo.0 },
        if to.lo.1 > self.lo.1 { u64::MAX } else { self.lo.1 },
      ),
    }
  }

  /// What is also at most `max` (`self` if nothing is: what follows is then never computed).
  const fn at_most(self, max: u128) -> Range {
    if self.min() > max {
      return self;
    }
    // Both are true of the value: so is what they share, which holds `self.min()`.
    let cut = Range::new(self.min(), min128(self.max(), max));
    Range {
      hi: (max64(self.hi.0, cut.hi.0), min64(self.hi.1, cut.hi.1)),
      lo: (max64(self.lo.0, cut.lo.0), min64(self.lo.1, cut.lo.1)),
    }
  }

  /// The width in bits of what the value is computed in: the narrowest of `u32`, `u64` and
  /// `u128` that holds every value, but not narrower than a register ([`REGISTER`]).
  pub const fn fits(self) -> u8 {
    let bits = self.bits();
    if bits < REGISTER { REGISTER } else { bits }
  }

  /// The width in bits of the narrowest of `u32`, `u64` and `u128` that holds every value.
  pub const fn bits(self) -> u8 {
    let max = self.max();
    if max <= u32::MAX as u128 {
      32
    } else if max <= u64::MAX as u128 {
      64
    } else {
      128
    }
  }
}

/// How an operation of an expression is computed, from what its operands may be.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct How {
  /// The width in bits (32, 64 or 128) of what the result may be, to which it is truncated: so
  /// that the optimizer knows it.
  pub result: u8,
  /// The width in bits (32, 64 or 128) in which a binary operation is computed: its operands and
  /// its result all fit it, and so does the amount of a shift.
  pub width: u8,
  /// Whatever the operation may fail: an addition, a subtraction, a product or a shift that
  /// cannot is computed without the test.
  pub fails: bool,
  /// An addition or a subtraction of 128 bits whose low halves do not carry or borrow, and whose
  /// high halves do not overflow: each half is computed alone.
  pub halves: bool,
}

impl How {
  /// On `u128`, every test made: what an expression means, and how it was computed before.
  pub const WIDE: How = How { result: 128, width: 128, fails: true, halves: false };
}

/// `value`, of which the analysis knows that it fits `bits` bits: so that the optimizer knows it.
#[inline(always)]
pub const fn fit(value: u128, bits: u8) -> u128 {
  match bits {
    32 => value as u32 as u128,
    64 => value as u64 as u128,
    _ => value,
  }
}

/// `op` of `a` and `b`, computed as `how` says: what [`layout::binary`] makes of them on `u128`,
/// failures included, where the analysis made `how` for what they may be.
#[inline(always)]
pub const fn binary(op: Op, how: How, a: u128, b: u128) -> Option<u128> {
  match how.width {
    32 => match narrow32(op, how.fails, a as u32, b as u32) {
      Some(value) => Some(value as u128),
      None => None,
    },
    64 => match narrow64(op, how.fails, a as u64, b as u64) {
      Some(value) => Some(value as u128),
      None => None,
    },
    _ => wide(op, how, a, b),
  }
}

/// On `u128`, without the tests that cannot fail.
#[inline(always)]
const fn wide(op: Op, how: How, a: u128, b: u128) -> Option<u128> {
  match op {
    Op::Add if !how.fails => Some(if how.halves {
      let hi = ((a >> 64) as u64).wrapping_add((b >> 64) as u64);
      (hi as u128) << 64 | (a as u64).wrapping_add(b as u64) as u128
    } else {
      a.wrapping_add(b)
    }),
    Op::Sub if !how.fails => Some(if how.halves {
      let hi = ((a >> 64) as u64).wrapping_sub((b >> 64) as u64);
      (hi as u128) << 64 | (a as u64).wrapping_sub(b as u64) as u128
    } else {
      a.wrapping_sub(b)
    }),
    Op::Mul if !how.fails => Some(a.wrapping_mul(b)),
    // The amount is less than 128.
    Op::Shl if !how.fails => Some(a.wrapping_shl(b as u32)),
    Op::Shr if !how.fails => Some(a.wrapping_shr(b as u32)),
    _ => layout::binary(op, a, b),
  }
}

/// `narrow!(name: T)`: a binary operation computed on `T`, of operands and a result that fit it,
/// and of a shift by less than its width. A sum, a product or a shift then does not overflow it;
/// a difference, a quotient or a remainder fails where it would on `u128`.
macro_rules! narrow {
  ($($name:ident: $t:ty)+) => {$(
    #[inline(always)]
    const fn $name(op: Op, fails: bool, a: $t, b: $t) -> Option<$t> {
      Some(match op {
        Op::Add => a.wrapping_add(b),
        Op::Sub if fails => match a.checked_sub(b) {
          Some(value) => value,
          None => return None,
        },
        Op::Sub => a.wrapping_sub(b),
        Op::Mul => a.wrapping_mul(b),
        Op::Div => match a.checked_div(b) {
          Some(value) => value,
          None => return None,
        },
        Op::Rem => match a.checked_rem(b) {
          Some(value) => value,
          None => return None,
        },
        Op::And => a & b,
        Op::Or => a | b,
        Op::Xor => a ^ b,
        Op::Shl => a.wrapping_shl(b as u32),
        Op::Shr => a.wrapping_shr(b as u32),
        Op::Eq => (a == b) as $t,
        Op::Ne => (a != b) as $t,
        Op::Lt => (a < b) as $t,
        Op::Le => (a <= b) as $t,
        Op::Gt => (a > b) as $t,
        Op::Ge => (a >= b) as $t,
        _ => return None,
      })
    }
  )+};
}

narrow! {
  narrow32: u32
  narrow64: u64
}

/// What `op` makes of values of `a` and of `b` where it does not fail, whatever it may fail, and
/// whatever it can be computed on halves that do not carry or borrow (an addition or a
/// subtraction).
pub const fn binary_range(op: Op, a: Range, b: Range) -> (Range, bool, bool) {
  match op {
    Op::Add => {
      if let (Some(lo), Some(hi)) = (a.lo.1.checked_add(b.lo.1), a.hi.1.checked_add(b.hi.1)) {
        let range = Range { hi: (a.hi.0 + b.hi.0, hi), lo: (a.lo.0 + b.lo.0, lo) };
        return (range, false, true);
      }
      match a.max().checked_add(b.max()) {
        Some(max) => (Range::new(a.min() + b.min(), max), false, false),
        None => (Range::new(a.min().saturating_add(b.min()), u128::MAX), true, false),
      }
    }
    Op::Sub => {
      if a.lo.0 >= b.lo.1 && a.hi.0 >= b.hi.1 {
        let range =
          Range { hi: (a.hi.0 - b.hi.1, a.hi.1 - b.hi.0), lo: (a.lo.0 - b.lo.1, a.lo.1 - b.lo.0) };
        return (range, false, true);
      }
      let range = Range::new(a.min().saturating_sub(b.max()), a.max().saturating_sub(b.min()));
      (range, a.min() < b.max(), false)
    }
    Op::Mul => match a.max().checked_mul(b.max()) {
      Some(max) => (Range::new(a.min() * b.min(), max), false, false),
      None => (Range::new(a.min().saturating_mul(b.min()), u128::MAX), true, false),
    },
    Op::Div => {
      let least = if b.min() == 0 { 1 } else { b.min() };
      // By zero only: a failure, after which nothing is computed.
      let range = match a.min().checked_div(b.max()) {
        Some(min) => Range::new(min, a.max() / least),
        None => Range::exact(0),
      };
      (range, b.min() == 0, false)
    }
    Op::Rem => {
      let range =
        if b.max() == 0 { Range::exact(0) } else { Range::new(0, min128(a.max(), b.max() - 1)) };
      (range, b.min() == 0, false)
    }
    // Bitwise: each half on its own.
    Op::And => {
      (Range { hi: (0, min64(a.hi.1, b.hi.1)), lo: (0, min64(a.lo.1, b.lo.1)) }, false, false)
    }
    Op::Or | Op::Xor => {
      let range =
        Range { hi: (0, ones(max64(a.hi.1, b.hi.1))), lo: (0, ones(max64(a.lo.1, b.lo.1))) };
      (range, false, false)
    }
    Op::Shl => {
      let fails = b.max() >= 128;
      // The low half becomes the high one.
      if b.min() == 64 && b.max() == 64 {
        return (Range { hi: a.lo, lo: (0, 0) }, false, false);
      }
      // No bit is shifted out.
      let range = if b.max() < 128 && a.max().leading_zeros() as u128 >= b.max() {
        Range::new(a.min() << b.min(), a.max() << b.max())
      } else {
        Range::ANY
      };
      (range, fails, false)
    }
    Op::Shr => {
      if b.min() == 64 && b.max() == 64 {
        return (Range { hi: (0, 0), lo: a.hi }, false, false);
      }
      let (least, most) = (min128(b.min(), 127), min128(b.max(), 127));
      (Range::new(a.min() >> most, a.max() >> least), b.max() >= 128, false)
    }
    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => (Range::new(0, 1), false, false),
    _ => (Range::ANY, true, false),
  }
}

/// The width in bits of a register of the target, from that of a pointer: what is narrower is
/// computed in a register all the same. On 64 bits, a sum of 32 bits that is the size of the value
/// (`40 + payload_length`) is not known to be the same as the sum of 64 bits that a `size` rule
/// compared with the size (it may wrap), and the bounds of what it reads are tested again.
const REGISTER: u8 = if usize::BITS >= 64 { 64 } else { 32 };

/// How a binary operation `op` of values of `a` and of `b` is computed.
pub const fn binary_how(op: Op, a: Range, b: Range) -> (How, Range) {
  binary_in(op, a, b, REGISTER)
}

/// How a binary operation `op` of values of `a` and of `b` is computed, where a register has
/// `register` bits.
pub const fn binary_in(op: Op, a: Range, b: Range, register: u8) -> (How, Range) {
  let (range, fails, halves) = binary_range(op, a, b);
  let mut width = register;
  if a.bits() > width {
    width = a.bits();
  }
  if b.bits() > width {
    width = b.bits();
  }
  if range.bits() > width {
    width = range.bits();
  }
  // A shift by its width or more is not what it is on `u128`.
  if matches!(op, Op::Shl | Op::Shr) && b.max() >= width as u128 {
    width = 128;
  }
  let result = if range.bits() < register { register } else { range.bits() };
  (How { result, width, fails, halves: halves && width == 128 }, range)
}

/// How each operation of an expression is computed, what the expression may be, and what its
/// variables may be at the walk where the analysis stopped.
pub struct Plan {
  pub how: [How; MAX_OPS],
  pub result: Range,
  pub entry: Vars,
}

/// No variable: a value of the stack that is not one.
const NONE: u8 = u8::MAX;

/// What the variables were before a part (the right side of a `&&`, a branch of an `if`), which
/// they still are after it if it was not computed.
#[derive(Copy, Clone)]
struct Part {
  /// The index of the operation that ends it.
  end: usize,
  vars: Vars,
}

const fn join_vars(a: Vars, b: Vars) -> Vars {
  let (mut vars, mut i) = (a, 0);
  while i < VARIABLES {
    vars[i] = a[i].join(b[i]);
    i += 1;
  }
  vars
}

/// The plan of an expression that starts as `entry` says.
pub const fn plan(ops: &[Op], entry: &Entry) -> Plan {
  analyze(ops, entry.vars, usize::MAX, entry.walks)
}

/// Where the expressions of the walk at the index `at` of an expression that starts as `entry`
/// say start.
pub const fn walk_entry(ops: &[Op], entry: &Entry, at: usize) -> Entry {
  Entry { vars: analyze(ops, entry.vars, at, entry.walks).entry, walks: entry.walks + 1 }
}

/// Walks nested deeper than this have a state that may be anything: a fixed point is found by
/// analysing what follows a step a few times, and the steps of the walks it holds as many times
/// each.
const FIXED_WALKS: usize = 2;

/// What the state of a walk may be: what it starts as, and what any number of steps make of it,
/// a step being `next`, where the variables are `vars` and the state is the variable `var`.
const fn fixed_point(next: &[Op], vars: Vars, var: u8, start: Range, walks: usize) -> Range {
  if walks >= FIXED_WALKS {
    return Range::ANY;
  }
  let (mut state, mut round) = (start, 0);
  loop {
    let mut inner = vars;
    inner[var as usize] = state;
    let after = state.join(analyze(next, inner, usize::MAX, walks + 1).result);
    if after.same(state) {
      return state;
    }
    // Twice as it is, then each bound that moves goes as far as it can: at most four more times.
    state = if round < 2 { after } else { state.widen(after) };
    round += 1;
  }
}

/// What every value of the expression `ops` may be where it is computed, from what its variables
/// are where it starts (`vars`) and what it reads; with `stop`, up to the walk at that index,
/// where the expressions of the walk start as `Plan::entry` says. `walks` is the number of walks
/// that the expression is in.
///
/// The operations are taken in order, as the evaluator applies them where it skips none: a value
/// that a part pushes is what the part makes, or a placeholder that nothing reads (the branch of
/// an `if` that is not taken, which [`Op::Select`] leaves), or the value of the left side of a
/// `&&` or `||` that decided. A read of the value that succeeds (`be(at, 2)`) tells that its
/// offset is in the value: a variable that was read at is then at most the size of a value.
const fn analyze(ops: &[Op], vars: Vars, stop: usize, walks: usize) -> Plan {
  let mut plan = Plan { how: [How::WIDE; MAX_OPS], result: Range::ANY, entry: vars };
  let mut vars = vars;
  // The values of the stack, and the variable that each is, if it was pushed by `Get`.
  let (mut stack, mut tags, mut depth) = ([Range::ANY; STACK], [NONE; STACK], 0usize);
  let (mut parts, mut open) = ([Part { end: usize::MAX, vars }; STACK], 0usize);
  let mut i = 0;
  while i < ops.len() && i < MAX_OPS {
    let op = ops[i];
    // The end of a part: what follows may come after it or after what was before it.
    let mut before = vars;
    if open > 0 && parts[open - 1].end == i {
      open -= 1;
      before = parts[open].vars;
      // The `else` branch is computed only when the first one is not: from what was before it.
      if !matches!(op, Op::Else(_)) {
        vars = join_vars(vars, before);
      }
    }
    let (pops, value) = match op {
      Op::Lit(value) => (0, Range::exact(value)),
      Op::Load { size, .. } => (0, Range::bytes(size)),
      Op::Size => (0, Range::new(0, SIZE_MAX)),
      Op::Get(index) => {
        stack[depth] = vars[index as usize];
        tags[depth] = index;
        plan.how[i].result = vars[index as usize].fits();
        depth += 1;
        i += 1;
        continue;
      }
      Op::Set(index) => {
        depth -= 1;
        vars[index as usize] = stack[depth];
        // What the stack holds of it is what it was.
        let mut k = 0;
        while k < depth {
          if tags[k] == index {
            tags[k] = NONE;
          }
          k += 1;
        }
        i += 1;
        continue;
      }
      Op::Not => (1, Range::new(0, 1)),
      Op::At { size, .. } => {
        refine(&mut vars, tags[depth - 1], SIZE_MAX - size as u128);
        (1, Range::bytes(size))
      }
      Op::Varint { form, length } => {
        refine(&mut vars, tags[depth - 1], SIZE_MAX);
        let value = match (form, length) {
          (Varint::Quic, false) => Range::new(0, (1 << 62) - 1),
          (Varint::Quic, true) => Range::new(1, 8),
          (_, false) => Range::ANY,
          (_, true) => Range::new(1, SIZE_MAX),
        };
        (1, value)
      }
      Op::Crc32 | Op::Sum16 | Op::Crc(_) | Op::CrcZeros(_) | Op::Checksum(_) | Op::Matches(_) => {
        let operands = if matches!(op, Op::CrcZeros(_)) { 4 } else { 2 };
        let mut k = depth - operands;
        while k < depth {
          refine(&mut vars, tags[k], SIZE_MAX);
          k += 1;
        }
        let value = match op {
          Op::Crc32 | Op::Checksum(layout::Checksum::Adler32) => Range::bytes(4),
          Op::Sum16 | Op::Checksum(layout::Checksum::Fletcher16) => Range::bytes(2),
          Op::Crc(_) | Op::CrcZeros(_) => Range::bytes(8),
          Op::Matches(_) => Range::new(0, 1),
          // A sum of bytes.
          _ => Range::new(0, 255 * SIZE_MAX),
        };
        (operands, value)
      }
      // What decides is the value of the `&&` or the `||`, which its end makes `0..=1` as well.
      Op::Shortcut { len, .. } => {
        parts[open] = Part { end: i + len as usize, vars };
        open += 1;
        (1, stack[depth - 1])
      }
      Op::Then(len) => {
        parts[open] = Part { end: i + len as usize, vars };
        open += 1;
        i += 1;
        continue;
      }
      // What the first branch left is joined at the `Select`, with what the second leaves.
      Op::Else(len) => {
        parts[open] = Part { end: i + len as usize + 1, vars };
        open += 1;
        vars = before;
        i += 1;
        continue;
      }
      Op::Both | Op::Either => (2, Range::new(0, 1)),
      Op::Select => (3, stack[depth - 2].join(stack[depth - 1])),
      Op::Walk { var, next, .. } => {
        let state = fixed_point(next, vars, var, stack[depth - 1], walks);
        if i == stop {
          plan.entry = vars;
          plan.entry[var as usize] = state;
          return plan;
        }
        (1, state)
      }
      op => {
        let (how, value) = binary_how(op, stack[depth - 2], stack[depth - 1]);
        plan.how[i] = how;
        (2, value)
      }
    };
    depth -= pops;
    stack[depth] = value;
    tags[depth] = NONE;
    plan.how[i].result = value.fits();
    depth += 1;
    i += 1;
  }
  plan.result = stack[0];
  plan
}

/// What the variable `index` (if a variable, else [`NONE`]) is known to be once it was read at,
/// in the value: at most `max`.
const fn refine(vars: &mut Vars, index: u8, max: u128) {
  if index != NONE {
    vars[index as usize] = vars[index as usize].at_most(max);
  }
}

/// The plan of the expression `X`, made once for it.
pub struct Planned<X>(core::marker::PhantomData<X>);

impl<X: crate::steps::Ops> Planned<X> {
  pub const PLAN: Plan = plan(X::OPS, &X::ENTRY);
}
