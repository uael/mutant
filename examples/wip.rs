#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]
use crate::MaybeUninit::Uninit;
use core::marker::PhantomData;

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct Unaligned<T>(T);

const END: u8 = 0;
const VOID: u8 = 1;
const UNINIT: u8 = 2;
const INIT: u8 = 3;
const COUNT: u8 = 4;
const NEXT: u8 = 5;
const GOTO: u8 = 6;
const ALT_BEGIN: u8 = 7;
const ALT_END: u8 = 8;
const ALT: u8 = 9;

#[repr(C, u8)]
#[derive(Debug, Copy, Clone)]
enum Inst {
  End = END,
  Void = VOID,
  Uninit = UNINIT,
  Init(ByteInterval) = INIT,
  Count { counter: Unaligned<u16> } = COUNT,
  Next { relative: Unaligned<i16> } = NEXT,
  Goto { relative: Unaligned<i16> } = GOTO,
  AltBegin = ALT_BEGIN,
  AltEnd = ALT_END,
  Alt = ALT,
}

const TEST: &'static [u8] = &[COUNT, 5, 0, UNINIT, NEXT, 2, 0, INIT, 42, 42, VOID];

trait Side {
  type Flip: Side<Flip = Self>;
}

struct A;
struct B;

impl Side for A {
  type Flip = B;
}

impl Side for B {
  type Flip = A;
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Intersection {
  Disjoint = 0b0, // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
  Overlap = 0b1,  // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
  Subset = 0b11,  // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)
}

impl Intersection {
  const fn and(self, rhs: Self) -> Self {
    use core::mem::transmute;
    unsafe { transmute(transmute::<_, u8>(self) & transmute::<_, u8>(rhs)) }
  }

  const fn or(self, rhs: Self) -> Self {
    use core::mem::transmute;
    unsafe { transmute(transmute::<_, u8>(self) | transmute::<_, u8>(rhs)) }
  }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum MaybeUninit<T> {
  Uninit,
  Init(T),
}

#[repr(C)]
struct Branch<S: Side> {
  state: State<S::Flip>,
  result: Intersection,
  current: Option<Intersection>,
}

struct State<S: Side> {
  counters: *mut u8,
  branches: *mut u8,
  pattern: *const u8,
  result: Intersection,
  _s: PhantomData<S>,
}

impl<S: Side> State<S> {
  const unsafe fn new(pattern: *const u8, counters: *mut u8, branches: *mut u8) -> Self {
    *counters.cast::<usize>() = 0;
    *branches.cast::<usize>() = 0;
    Self { counters, branches, result: Intersection::Subset, pattern, _s: PhantomData }
  }

  const unsafe fn next(&mut self, rhs: &mut State<S::Flip>) -> Option<MaybeUninit<ByteInterval>> {
    use Inst::*;
    loop {
      let offset = match *self.pattern.cast::<Inst>() {
        // end: stop iteration.
        End => break None,
        // void: continue to next pattern.
        Void => 1,
        // uninit:
        // - advance to next pattern.
        // - return `Uninit`.
        Uninit => {
          self.pattern = self.pattern.wrapping_add(1);
          break Some(MaybeUninit::Uninit);
        }
        // init:
        // - advance to next pattern.
        // - return `Init(interval)`.
        Init(interval) => {
          self.pattern = self.pattern.wrapping_add(3);
          break Some(MaybeUninit::Init(interval));
        }
        // count:
        // - push `counter` to the counter stack.
        // - continue to next pattern.
        Count { counter } => {
          // push `counter` to the counter stack.
          let len_ptr = self.counters.cast::<usize>();
          let ptr = len_ptr.wrapping_add(1).cast::<u16>();
          *ptr.wrapping_add(*len_ptr) = counter.0;
          *len_ptr += 1;
          // continue to next pattern.
          3
        }
        // next:
        // - decrement counter stack top counter by 1.
        // - when counter is 0: rewind by `relative`.
        // - otherwise: pop stack & continue to next pattern.
        Next { relative } => {
          // decrement counter stack top counter by 1.
          let len_ptr = self.counters.cast::<usize>();
          let ptr = len_ptr.wrapping_add(1).cast::<u16>();
          *ptr.wrapping_add(*len_ptr - 1) -= 1;
          // when counter is 0:
          if *ptr.wrapping_add(*len_ptr - 1) > 0 {
            // rewind by `relative`.
            -relative.0 as isize
          } else {
            // otherwise: pop stack & continue to next pattern.
            *len_ptr -= 1;
            3
          }
        }
        // goto: rewind by `relative`.
        Goto { relative } => -relative.0 as isize,
        // branch begin:
        AltBegin => {
          let state = rhs.save();
          let len_ptr = self.branches.cast::<usize>();
          let ptr = len_ptr.wrapping_add(1).cast::<Branch<S>>();
          *ptr.wrapping_add(*len_ptr) = Branch { state, result: self.result, current: None };
          *len_ptr += 1;
          self.result = Intersection::Subset;
          1
        }
        AltEnd => {
          let len_ptr = self.branches.cast::<usize>();
          let ptr = len_ptr.wrapping_add(1).cast::<Branch<S>>();
          *len_ptr -= 1;
          let Branch { state, result, current } = core::ptr::read(ptr.wrapping_add(*len_ptr));
          *rhs = state;
          self.result = result.and(match current {
            Some(x) => x,
            _ => result,
          });
          1
        }
        Alt => {
          let len_ptr = self.branches.cast::<usize>();
          let ptr = len_ptr.wrapping_add(1).cast::<Branch<S>>();
          let mut branch = &mut *ptr.wrapping_add(*len_ptr - 1);
          *rhs = core::ptr::read(&branch.state);
          branch.current = Some(match branch.current {
            Some(x) => x.or(self.result),
            _ => self.result,
          });
          self.result = Intersection::Subset;
          1
        }
      };
      // advance to next pattern.
      self.pattern = self.pattern.offset(offset);
    }
  }

  const unsafe fn save(&mut self) -> Self {
    const unsafe fn copy(src: *const u8, dst: *mut u8, mut n: usize) -> *mut u8 {
      while n > 0 {
        n -= 1;
        *dst.wrapping_add(n) = *src.wrapping_add(n);
      }
      dst
    }

    let Self { counters, branches, .. } = *self;
    let state = Self { ..*self };
    // duplicate counters:
    let size = size_of::<usize>() + *counters.cast::<usize>() * size_of::<u16>();
    self.counters = copy(counters, counters.wrapping_byte_add(size), size);
    // duplicate branches:
    let size = size_of::<usize>() + *branches.cast::<usize>() * size_of::<Branch<S>>();
    self.branches = copy(branches, branches.wrapping_byte_add(size), size);
    state
  }
}

const unsafe fn intersect(a: *const u8, b: *const u8) -> (Intersection, Intersection) {
  let (mut w, mut x, mut y, mut z) = ([0usize; 512], [0usize; 8192], [0usize; 512], [0usize; 8192]);
  let (mut a, mut b) = (
    State::<A>::new(a, w.as_mut_ptr().cast(), x.as_mut_ptr().cast()),
    State::<B>::new(b, y.as_mut_ptr().cast(), z.as_mut_ptr().cast()),
  );
  loop {
    use MaybeUninit::*;
    match (a.next(&mut b), b.next(&mut a)) {
      (None, _) | (_, None) => break (a.result, b.result),
      // uninit.
      (Some(Init(_)), Some(Uninit)) | (Some(Uninit), Some(Init(_))) => todo!(),
      (Some(Uninit), Some(Uninit)) => {}
      // init
      (Some(Init(l)), Some(Init(r))) => {
        let i = l.intersects(&r);
        a.result = a.result.and(match (l.len(), i) {
          (_, 0) => Intersection::Disjoint,
          (l, r) if l != r => Intersection::Overlap,
          (_, _) => Intersection::Subset,
        });
        b.result = b.result.and(match (r.len(), i) {
          (_, 0) => Intersection::Disjoint,
          (l, r) if l != r => Intersection::Overlap,
          (_, _) => Intersection::Subset,
        });
      }
    }
  }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct ByteInterval {
  start: u8,
  end: u8,
}

impl ByteInterval {
  pub const ANY: Self = Self { start: 0x00, end: 0xff };
  pub const EMPTY: Self = Self { start: 0xff, end: 0x00 };

  pub(crate) const fn len(&self) -> usize {
    1usize + (self.end - self.start) as usize
  }

  pub(crate) const fn intersects(&self, o: &Self) -> usize {
    macro_rules! l {
      ($v:expr) => {
        if $v <= 127 {
          1u128 << $v
        } else {
          0u128
        }
      };
    }
    macro_rules! h {
      ($v:expr) => {
        if $v <= 127 {
          0u128
        } else {
          1u128 << ($v - 128)
        }
      };
    }

    // `(a, b)` and `(c, d)` are the byte intervals.
    let ((mut a, b), (mut c, d)) = ((self.start, self.end), (o.start, o.end));

    // Compute low and high bits for an u256 bitset covering the `self` byte intervals.
    let mut e = (l!(b), h!(b));
    while a < b {
      e.0 |= l!(a);
      e.1 |= h!(a);
      a += 1;
    }

    // Compute low and high bits for an u256 bitset covering the `o` byte intervals.
    let mut f = (l!(d), h!(d));
    while c < d {
      f.0 |= l!(c);
      f.1 |= h!(c);
      c += 1;
    }

    // Return the number of intersecting values.
    (e.0 & f.0).count_ones() as usize + (e.1 & f.1).count_ones() as usize
  }
}

impl core::fmt::Debug for ByteInterval {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    if self.start > self.end {
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

fn main() {
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0x44], false)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0xab], false)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0xab, 0xcd], false)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0xAB, 0xCD, 0xEF, 0xFE], false)
  );

  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0x44], true)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0xab], true)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0xab, 0xcd], true)
  );
  println!(
    "{:?}",
    compile_range::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0xAB, 0xCD, 0xEF, 0xFE], true)
  );

  println!("{:?}", compile_range::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0x33, 0x44], false));
  println!("{:?}", compile_range::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0x33, 0xab], false));
  println!("{:?}", compile_range::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0xab, 0xcd], false));
  println!("{:?}", compile_range::<3, 5>(&[0x22, 0x33, 0x44], &[0xCD, 0xEF, 0xFE], false));
  println!("{:?}", compile_range::<3, 5>(&[0x00, 0x00, 0x00], &[0xCD, 0xEF, 0xFE], false));
  println!("{:?}", compile_range::<3, 5>(&[0x01, 0x00, 0x00], &[0xCD, 0xEF, 0xFE], false));
  println!("{:?}", compile_range::<3, 5>(&[0x22, 0x33, 0x44], &[0xff, 0xff, 0xff], false));

  println!("{:?}", compile_range::<2, 3>(&[0x11, 0x22], &[0xab, 0xcd], false));
  println!("{:?}", compile_range::<2, 3>(&[0x11, 0x22], &[0x11, 0xcd], false));
  println!("{:?}", compile_range::<2, 3>(&[0x00, 0x22], &[0xff, 0xcd], false));

  println!("{:?}", compile_range::<1, 1>(&[0x11], &[0xab], false));
  println!("{:?}", compile_range::<1, 1>(&[0x11], &[0x11], false));

  unsafe {
    // println!("{:?}", intersect([INIT, 0x00, 0xff, END].as_ptr(), [INIT, 0x00, 0xff, END].as_ptr()));
    println!("{:?}", {
      intersect([INIT, 0x00, 0x01, END].as_ptr(), [INIT, 0x00, 0xff, END].as_ptr())
    });
    println!("{:?}", {
      intersect([INIT, 0x00, 0x01, END].as_ptr(), [INIT, 0x00, 0xff, END].as_ptr())
    });
    println!("{:?}", {
      intersect(
        [ALT_BEGIN, INIT, 0x00, 0x00, ALT, INIT, 0x00, 0x01, ALT_END, END].as_ptr(),
        [INIT, 0x00, 0xff, END].as_ptr(),
      )
    });
  }
}
