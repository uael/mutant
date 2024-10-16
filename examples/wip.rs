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

fn main() {
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
