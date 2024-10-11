use crate::num::LiteralUint;
use crate::{
  endian::Endianness,
  num,
  num::{Native, NativeOf, Numeric},
  ptr, Default, Literal,
};
use core::marker::PhantomData;
use core::num::NonZero;

#[derive(Copy, Clone)]
pub struct ByteInterval {
  start: u8,
  end: u8,
}

// impl `ByteInterval`
const _: () = {
  use core::fmt;

  impl ByteInterval {
    pub const ANY: Self = Self { start: 0, end: 0xff };
    pub const EMPTY: Self = Self { start: 0xff, end: 0 };

    pub(crate) const fn len(&self) -> usize {
      if self.start > self.end {
        return 0;
      }
      1usize + (self.end - self.start) as usize
    }

    pub(crate) const fn intersect(&self, o: &Self) -> usize {
      let start = if self.start < o.start {
        if self.end < o.start {
          return 0;
        }
        o.start
      } else if o.end < self.start {
        return 0;
      } else {
        self.start
      };
      1usize + (if self.end < o.end { self.end } else { o.end } - start) as usize
    }
  }

  impl fmt::Debug for ByteInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      if self.len() == 0 {
        return write!(f, "∅");
      }
      if self.start == self.end {
        return write!(f, "{:#02x}", self.start);
      }
      if self.start != 0 {
        write!(f, "{:#02x}", self.start)?;
      }
      write!(f, "..")?;
      if self.end != 0xff {
        write!(f, "={:#02x}", self.end)?;
      }
      Ok(())
    }
  }
};

#[derive(Debug, Copy, Clone)]
pub enum BitsPattern {
  Uninit,
  Init(&'static [ByteInterval]),
  Any(&'static [BitsPattern]),
}

// impl `BitsPattern`
const _: () = {
  impl BitsPattern {
    const fn reachable(&self) -> bool {
      match self {
        BitsPattern::Uninit => true,
        BitsPattern::Init(x) => {
          let mut i = 0;
          while i < x.len() && x[i].len() > 0 {
            i += 1
          }
          i == x.len()
        }
        BitsPattern::Any(_) => true,
      }
    }

    #[cfg(test)]
    pub fn matches<T: Copy>(&self, memory: T) -> bool {
      use BitsPattern::*;
      match self {
        Uninit => true,
        Any(x) => x.iter().any(|p| p.matches(memory)),
        Init(x) => {
          let mem = unsafe {
            core::slice::from_raw_parts(core::ptr::from_ref(&memory).cast::<u8>(), size_of::<T>())
          };
          x.iter().zip(mem).all(|(&ByteInterval { start, end }, m)| start <= *m && *m <= end)
        }
      }
    }
  }
};

pub enum Operator<T> {
  True,
  Equ(T),
  RangeFrom(T),
  RangeTo(T),
  Range(T, T),
  Or,
}

pub trait Satisfy<T> {
  type Lhs: Satisfy<T>;
  type Rhs: Satisfy<T>;
  const OP: Operator<T>;
}

#[derive(Copy, Clone)]
pub struct Equ<T, Val>(PhantomData<(T, Val)>);

#[derive(Copy, Clone)]
pub struct RangeFrom<T, Val>(PhantomData<(T, Val)>);

#[derive(Copy, Clone)]
pub struct RangeTo<T, Val>(PhantomData<(T, Val)>);

#[derive(Copy, Clone)]
pub struct Range<T, Start, End>(PhantomData<(T, Start, End)>);

#[derive(Copy, Clone)]
pub struct Or<Lhs, Rhs>(PhantomData<(Lhs, Rhs)>);

#[inline]
pub const fn is_true<O: Copy, T: Satisfy<O>>() -> bool {
  matches!(T::OP, Operator::True)
}

#[inline]
pub const fn eval<O: Native, T: Satisfy<O>>(val: O) -> bool {
  match T::OP {
    Operator::True => true,
    Operator::RangeFrom(start) => num::is_ge(val, start),
    Operator::RangeTo(end) => num::is_le(val, end),
    Operator::Range(start, end) => num::is_ge(val, start) && num::is_le(val, end),
    Operator::Equ(rhs) => num::is_eq(val, rhs),
    Operator::Or => eval::<O, T::Lhs>(val) || eval::<O, T::Rhs>(val),
  }
}

// impl `Satisfy`
const _: () = {
  impl<O> Satisfy<O> for () {
    type Lhs = ();
    type Rhs = ();
    const OP: Operator<O> = Operator::True;
  }

  impl<T, Val: Literal<T>> Satisfy<T> for Equ<T, Val> {
    type Lhs = ();
    type Rhs = ();
    const OP: Operator<T> = Operator::Equ(Val::LITERAL);
  }

  impl<T, Val: Literal<T>> Satisfy<T> for RangeFrom<T, Val> {
    type Lhs = ();
    type Rhs = ();
    const OP: Operator<T> = Operator::RangeFrom(Val::LITERAL);
  }

  impl<T, Val: Literal<T>> Satisfy<T> for RangeTo<T, Val> {
    type Lhs = ();
    type Rhs = ();
    const OP: Operator<T> = Operator::RangeTo(Val::LITERAL);
  }

  impl<T: Native, Start: Literal<T>, End: Literal<T>> Satisfy<T> for Range<T, Start, End> {
    type Lhs = ();
    type Rhs = ();
    const OP: Operator<T> = {
      assert!(num::is_le(Start::LITERAL, End::LITERAL));
      Operator::Range(Start::LITERAL, End::LITERAL)
    };
  }

  impl<O, Lhs: Satisfy<O>, Rhs: Satisfy<O>> Satisfy<O> for Or<Lhs, Rhs> {
    type Lhs = Lhs;
    type Rhs = Rhs;
    const OP: Operator<O> = Operator::Or;
  }
};

pub trait Raw: 'static + Copy + Default {
  type Repr: Numeric;
  type Satisfy: Satisfy<NativeOf<Self::Repr>>;
  const RAW_DEFAULT: Self;
}

#[inline]
pub const fn size_of_term<T: Raw>() -> usize {
  const { size_of::<T::Repr>() }
}

#[inline]
pub const unsafe fn satisfy_term<F: Raw>(ptr: *const u8) -> bool {
  if const { is_true::<NativeOf<F::Repr>, F::Satisfy>() } {
    return true;
  }
  let ptr = ptr.cast::<F::Repr>();
  let val = num::cast(unsafe { ptr.read_unaligned() });
  eval::<NativeOf<F::Repr>, F::Satisfy>(val)
}

// impl `Raw`
const _: () = {
  unsafe impl<T: Raw> ptr::Pointee for T {
    type Metadata = ();
  }

  impl<T: Numeric> Raw for T {
    type Repr = T;
    type Satisfy = ();
    const RAW_DEFAULT: Self = T::ZERO;
  }

  impl<T: Numeric, const VALUE: u128> Raw for LiteralUint<T, VALUE> {
    type Repr = T;
    type Satisfy = Equ<T::Native, LiteralUint<T::Native, VALUE>>;
    const RAW_DEFAULT: Self = Self::new();
  }

  impl Raw for bool {
    type Repr = u8;
    type Satisfy = RangeTo<u8, LiteralUint<u8, 1>>;
    const RAW_DEFAULT: Self = false;
  }

  impl Raw for char {
    type Repr = u32;
    type Satisfy = Or<
      RangeTo<u32, LiteralUint<u32, { 0xD800 - 1 }>>,
      Range<u32, LiteralUint<u32, { 0xDFFF + 1 }>, LiteralUint<u32, { 0x110000 - 1 }>>,
    >;
    const RAW_DEFAULT: Self = '\0';
  }

  macro_rules! impl_non_zero {
    { $($T:ident)+ } => {$(
      impl Raw for NonZero<$T> {
        type Repr = $T;
        type Satisfy = RangeFrom<$T, LiteralUint<$T, 1>>;
        const RAW_DEFAULT: Self = match NonZero::new(1) {
          Some(val) => val,
          _ => unreachable!()
        };
      }

      impl Raw for Option<NonZero<$T>> {
        type Repr = $T;
        type Satisfy = ();
        const RAW_DEFAULT: Self = None;
      }
    )+};
  }

  impl_non_zero! { u8 u16 u32 u64 u128 }
};

#[repr(transparent)]
pub struct Sat<T, F>(T, PhantomData<F>);

// impl `Sat`
const _: () = {
  impl<T: Numeric, F: Satisfy<T::Native>> Copy for Sat<T, F> {}

  impl<T: Numeric, F: Satisfy<T::Native>> Clone for Sat<T, F> {
    fn clone(&self) -> Self {
      *self
    }
  }

  impl<T: Numeric, F: Satisfy<T::Native> + 'static> Raw for Sat<T, F> {
    type Repr = T;
    type Satisfy = F;
    const RAW_DEFAULT: Self = Sat(<T as Default>::DEFAULT, PhantomData);
  }
};

pub enum LayoutKind {
  Raw,
  And(usize, usize),
  Repeat(usize, usize),
  Wildcard(usize),
  #[cfg(test)]
  Uninit,
}

pub trait Layout {
  type Lhs: Layout;
  type Rhs: Layout;
  type Raw: Raw;
  const KIND: LayoutKind;
}

#[inline]
pub const unsafe fn satisfy<T: Layout>(ptr: *const u8, metadata: usize) -> bool {
  use LayoutKind::*;
  match const { T::KIND } {
    Raw => satisfy_term::<T::Raw>(ptr),
    And(l, r) => {
      satisfy::<T::Lhs>(ptr.wrapping_add(l), metadata)
        && satisfy::<T::Rhs>(ptr.wrapping_add(r), metadata)
    }
    Repeat(_, 0) | Wildcard(0) => true,
    Repeat(_, s) | Wildcard(s) => {
      let (mut i, n) = (0, if let Repeat(n, _) = const { T::KIND } { n } else { metadata });
      while i < n {
        if !satisfy::<T::Lhs>(ptr.wrapping_add(i * s), 0) {
          return false;
        }
        i += 1;
      }
      true
    }
    #[cfg(test)]
    Uninit => true,
  }
}

#[inline]
pub const fn wildcard_size<T: Layout>() -> usize {
  use LayoutKind::*;
  const {
    match T::KIND {
      Raw | Repeat(_, _) => 0,
      And(_, _) => wildcard_size::<T::Lhs>() + wildcard_size::<T::Rhs>(),
      Wildcard(s) => s,
      #[cfg(test)]
      Uninit => 0,
    }
  }
}

#[inline]
pub const unsafe fn initialize_to_default<T: Layout>(ptr: *mut u8) {
  use LayoutKind::*;
  match const { T::KIND } {
    Raw => *ptr.cast() = T::Raw::RAW_DEFAULT,
    And(l, r) => {
      initialize_to_default::<T::Lhs>(ptr.wrapping_add(l));
      initialize_to_default::<T::Rhs>(ptr.wrapping_add(r));
    }
    Repeat(n, s) => {
      let mut i = 0;
      while i < n {
        initialize_to_default::<T::Lhs>(ptr.wrapping_add(i * s));
        i += 1;
      }
    }
    Wildcard(_) => {}
    #[cfg(test)]
    Uninit => {}
  }
}

/// Compile an inclusive range from `start` to `end`, where `start` and `end` are numbers
/// encoded in big-endian. `start` bound must be greater or equals than `end` bound, this will
/// not be checked within this function.
///
/// # Parameters
/// - `start`: Start bound encoded in big-endian.
/// - `end`: End bound encoded in big-endian.
/// - `lsb`: Whatever the returned sub-ranges should be in little-endian, big-endian otherwise.
///
/// # Returns
/// All the possible byte sub-ranges of the input range.
/// Empty ranges are set to `[ByteInterval::EMPTY; N]`.
const fn compile_range<const N: usize, const M: usize>(
  start: &[u8; N], end: &[u8; N], lsb: bool,
) -> [[ByteInterval; N]; M] {
  const fn offset<const N: usize>(off: usize, lsb: bool) -> usize {
    if lsb {
      N - off - 1
    } else {
      off
    }
  }

  const fn range<const N: usize>(off: usize, fixed: &[u8; N], lsb: bool) -> [ByteInterval; N] {
    let mut sub = [ByteInterval::ANY; N];
    // Fill empty sub-range with fixed bytes.
    let mut idx = 0;
    while idx < off {
      sub[offset::<N>(idx, lsb)] = ByteInterval { start: fixed[idx], end: fixed[idx] };
      idx += 1;
    }
    sub
  }

  const fn all<const N: usize>(mut idx: usize, val: u8, bytes: &[u8; N]) -> bool {
    while idx < N && bytes[idx] == val {
      idx += 1;
    }
    idx == N
  }

  let mut res = [[ByteInterval::EMPTY; N]; M];
  let mut res_count = 0;

  // Compute start offset.
  let mut off = 0;
  while off < N && start[off] == end[off] {
    off += 1;
  }

  // All bytes are equals.
  if off + 1 >= N {
    res[0] = range::<N>(N, start, lsb);
    if off < N {
      res[0][offset::<N>(N - 1, lsb)].end = end[N - 1];
    }
    return res;
  }

  let middle_end = if all::<N>(off, 0xff, end) { 0xff } else { end[off] - 1 };
  let middle_start = if all::<N>(off, 0x00, start) {
    0x00
  } else {
    // First sub-range:
    let mut sub = range::<N>(N, start, lsb);
    sub[offset::<N>(N - 1, lsb)].end = 0xff;
    res[res_count] = sub;
    res_count += 1;

    // Start bound middle ranges:
    let mut idx = off + 1;
    while idx < N - 1 {
      sub = range::<N>(idx, start, lsb);
      if let Some(s) = start[idx].checked_add(1) {
        sub[offset::<N>(idx, lsb)].start = s;
        res[res_count] = sub;
        res_count += 1;
      };
      idx += 1;
    }

    start[off] + 1
  };

  // Middle range:
  let mut sub = range::<N>(off, start, lsb);
  sub[offset::<N>(off, lsb)].start = middle_start;
  sub[offset::<N>(off, lsb)].end = middle_end;
  res[res_count] = sub;
  res_count += 1;

  if middle_end != 0xff {
    // End bound middle ranges:
    let mut idx = off + 1;
    while idx < N - 1 {
      sub = range::<N>(idx, end, lsb);
      if let Some(s) = end[idx].checked_sub(1) {
        sub[offset::<N>(idx, lsb)].end = s;
        res[res_count] = sub;
        res_count += 1;
      };
      idx += 1;
    }

    // Last sub-range:
    sub = range::<N>(N, end, lsb);
    sub[offset::<N>(N - 1, lsb)].start = 0x00;
    res[res_count] = sub;
  }

  res
}

/// Compile `T` term type into `BitsPattern`
#[inline]
const fn compile<T: Raw, const N: usize, const M: usize>() -> BitsPattern {
  const fn compile_impl<O: Native, T: Satisfy<O>, E: Endianness, const N: usize, const M: usize>(
  ) -> BitsPattern {
    /// Compile both `lhs` and `rhs` sides of the expression.
    /// NOTE: Function indirection is required as it avoid a "cycle detected" compilation error.
    const fn operands<O: Native, T: Satisfy<O>, E: Endianness, const N: usize, const M: usize>(
    ) -> &'static [BitsPattern] {
      &const { [compile_impl::<O, T::Lhs, E, N, M>(), compile_impl::<O, T::Rhs, E, N, M>()] }
    }

    use BitsPattern::*;
    use Operator::*;

    match T::OP {
      True => Init(&[ByteInterval::ANY; N]),
      Or => Any(operands::<O, T, E, N, M>()),
      Equ(_) => Init(
        &const {
          let val = if let Equ(x) = T::OP { x } else { num::cast(0u8) };
          let bytes = match E::LITERAL.is_little() {
            true => num::to_le_bytes(val),
            _ => num::to_be_bytes(val),
          };
          // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
          let bytes: &[u8; N] = unsafe { core::mem::transmute(&bytes) };
          let mut intervals = [ByteInterval::ANY; N];
          let mut i = 0;
          while i < N {
            intervals[i] = ByteInterval { start: bytes[i], end: bytes[i] };
            i += 1;
          }
          intervals
        },
      ),
      RangeFrom(_) | RangeTo(_) | Range(_, _) => {
        let patterns = &const {
          let intervals = &const {
            use core::mem::transmute;
            use num::{cast, to_be_bytes, Maximum, Minimum};
            let (s, e) = match T::OP {
              RangeFrom(s) => (s, <O as Maximum>::MAX),
              RangeTo(e) => (<O as Minimum>::MIN, e),
              Range(s, e) => (s, e),
              _ => (cast(()), cast(())),
            };
            let (s, e) = (to_be_bytes(s), to_be_bytes(e));
            // SAFETY: `bytes` is a valid memory region of `[u8; N]`.
            let (s, e): (&[u8; N], &[u8; N]) = unsafe { (transmute(&s), transmute(&e)) };
            compile_range::<N, M>(s, e, E::LITERAL.is_little())
          };
          let mut patterns = [Uninit; M];
          let mut i = 0;
          while i < M && Init(&intervals[i]).reachable() {
            patterns[i] = Init(&intervals[i]);
            i += 1;
          }
          patterns
        };
        let mut i = 0;
        while i < M && !matches!(patterns[i], Uninit) {
          i += 1
        }
        Any(patterns.split_at(i).0)
      }
    }
  }

  const { compile_impl::<NativeOf<T::Repr>, T::Satisfy, <T::Repr as Numeric>::Endian, N, M>() }
}

pub(crate) const fn offset<T: Layout>(off: usize) -> &'static BitsPattern {
  use LayoutKind::*;
  match T::KIND {
    #[cfg(test)]
    Uninit => &BitsPattern::Uninit,
    Raw if off == 0 => {
      &const {
        macro_rules! compile_dispatch {
          ($($size:literal)+) => {
            match size_of::<T::Raw>() {
              0 => BitsPattern::Init(&[]),
              $($size => compile::<T::Raw, $size, { 2 * $size - 1 }>(),)+
              _ => unreachable!(),
            }
          };
        }
        compile_dispatch!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16)
      }
    }
    Raw => &BitsPattern::Uninit,
    Repeat(n, s) if off < n * s => offset::<T::Lhs>(off % s),
    Repeat(_, _) | Wildcard(0) => &BitsPattern::Uninit,
    Wildcard(s) => offset::<T::Lhs>(off % s),
    And(l, r) => {
      let l = if off >= l { offset::<T::Lhs>(off - l) } else { &BitsPattern::Uninit };
      if matches!(l, &BitsPattern::Uninit) && off >= r {
        offset::<T::Rhs>(off - r)
      } else {
        l
      }
    }
  }
}

#[cfg(test)]
pub struct Uninit;

#[cfg(test)]
impl Layout for Uninit {
  type Lhs = ();
  type Rhs = ();
  type Raw = ();
  const KIND: LayoutKind = LayoutKind::Uninit;
}

// impl `Layout`
const _: () = {
  impl<T: Raw> Layout for T {
    type Lhs = T;
    type Rhs = T;
    type Raw = T;
    const KIND: LayoutKind = LayoutKind::Raw;
  }

  impl<_0Off: Literal<usize>, _0: Layout> Layout for ((_0Off, _0),) {
    type Lhs = _0;
    type Rhs = ();
    type Raw = ();
    const KIND: LayoutKind = LayoutKind::And(_0Off::LITERAL, 0);
  }

  impl<_0Off: Literal<usize>, _0: Layout, _1Off: Literal<usize>, _1: Layout> Layout
    for ((_0Off, _0), (_1Off, _1))
  {
    type Lhs = _0;
    type Rhs = _1;
    type Raw = ();
    const KIND: LayoutKind = LayoutKind::And(_0Off::LITERAL, _1Off::LITERAL);
  }

  macro_rules! impl_and {
    // Match at least one pair
    { ($TOff:ident $T:ident) ($ROff:ident $R:ident) } => {};
    { ($TOff:ident $T:ident) $( ($ROff:ident $R:ident) )* } => {
      impl_and! { $( ($ROff $R) )* }
      impl<$TOff: Literal<usize>, $T: Layout, $($ROff: Literal<usize>, $R: Layout ),*>
        Layout for (($TOff, $T), $( ($ROff, $R) ),*)
      {
        type Lhs = $T;
        type Rhs = ($( ($ROff, $R) ),*);
        type Raw = ();
        const KIND: LayoutKind = LayoutKind::And($TOff::LITERAL, 0);
      }
    };
  }

  impl_and! {
    (_0Off _0) (_1Off _1) (_2Off _2) (_3Off _3)
    (_4Off _4) (_5Off _5) (_6Off _6) (_7Off _7)
    (_8Off _8) (_9Off _9) (_10Off _10) (_11Off _11)
    (_12Off _12) (_13Off _13) (_14Off _14) (_15Off _15)
    (_16Off _16) (_17Off _17) (_18Off _18) (_19Off _19)
    (_20Off _20) (_21Off _21) (_22Off _22) (_23Off _23)
    (_24Off _24) (_25Off _25) (_26Off _26) (_27Off _27)
    (_28Off _28) (_29Off _29) (_30Off _30) (_31Off _31)
  }
};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum Intersection {
  Disjoint, // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
  Overlap,  // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
  Subset,   // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)
}

pub(crate) const fn intersect<A: Layout, B: Layout>(end: usize) -> Intersection {
  const ALL: u8 = 0b11;
  const SOME: u8 = 0b01;

  const fn step<A: Layout, B: Layout>(
    mut off: usize, mut a: BitsPattern, mut b: BitsPattern, end: &mut usize,
  ) -> u8 {
    use BitsPattern::*;
    let mut result = ALL;
    loop {
      match (a, b) {
        // terminal conditions:
        (Uninit, Uninit) => {
          break {
            *end = off + 1;
            result
          }
        }
        (Init([]), Init([])) => {
          break {
            *end = off;
            result
          }
        }
        // overlapping patterns:
        (Init([]), _) => a = *offset::<A>(off),
        (_, Init([])) => b = *offset::<B>(off),
        // uninitialized against initialized:
        (Init([_, na @ ..]), Uninit) => {
          off += 1;
          a = Init(na);
          b = *offset::<B>(off);
        }
        (Uninit, Init([_, rb @ ..])) => {
          off += 1;
          a = *offset::<A>(off);
          b = Init(rb);
        }
        (Any(a), _) => {
          break result & {
            let mut result = step::<A, B>(off, a[0], b, end);
            let mut i = 1;
            while i < a.len() {
              result = match (result, step::<A, B>(off, a[i], b, end)) {
                (ALL, ALL) => ALL,
                (0, 0) => 0,
                _ => SOME,
              };
              i += 1;
            }
            result
          }
        }
        (_, Any(b)) => {
          break result & {
            let mut result = step::<A, B>(off, a, b[0], end);
            let mut i = 1;
            while i < b.len() {
              result |= step::<A, B>(off, a, b[i], end);
              i += 1;
            }
            result
          }
        }
        // both initialized:
        (Init([la, ra @ ..]), Init([lb, rb @ ..])) => match (la.len(), la.intersect(lb)) {
          (_, 0) => break 0,
          (l, r) => {
            result &= if l == r { ALL } else { SOME };
            off += 1;
            a = Init(ra);
            b = Init(rb);
          }
        },
      }
    }
  }

  let (mut off, mut result) = (0, ALL);
  while result != 0 && off < end {
    result &= step::<A, B>(off, *offset::<A>(off), *offset::<B>(off), &mut off);
  }

  match result {
    SOME => Intersection::Overlap,
    ALL => Intersection::Subset,
    _ => Intersection::Disjoint,
  }
}
