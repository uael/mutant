#![feature(const_mut_refs)]
#![feature(const_refs_to_cell)]
const VOID: u8 = 0;
const GOTO: u8 = 1;
const UNINIT: u8 = 2;
const INIT: u8 = 3;
const OR: u8 = 4;

const unsafe fn next(a: &mut *const u8, n: isize) {
  *a = a.offset(n);
}

const unsafe fn goto(a: &mut *const u8) {
  next(a, i32::from_le_bytes(*a.cast()) as isize)
}

/// Matches memory `m` against an encoded pattern `a`.
const unsafe fn matches(a: &mut *const u8, m: &mut *const u8, e: *const u8) -> bool {
  let mut res = true;
  loop {
    let mut tmp = *m;
    let inst = **a;
    next(a, 1);
    match inst {
      VOID => break res,
      _ if e.offset_from(*m) <= 0 => break res,
      GOTO => goto(a),
      UNINIT => next(m, 1),
      INIT => {
        let (start, end) = (**a, *a.wrapping_add(1));
        res &= **m >= start && **m <= end;
        next(a, 2);
        next(m, 1);
      }
      OR => res &= matches(a, &mut tmp, e) || matches(a, m, e),
      _ => unreachable!()
    }
  }
}

// intersects results:
const DISTINCT: u8 = 0; // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
const OVERLAP: u8 = 1; // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
const SUBSET: u8 = 3; // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)

/// Intersects `a` and `b` at offset `i` until offset `e`.
/// `e` is precomputed, `i` and `e` only serve as a termination condition.
const unsafe fn intersect(a: &mut *const u8, b: &mut *const u8, i: usize, e: &mut usize) -> u8 {
  todo!()
}

#[derive(Copy, Clone)]
pub struct ByteInterval {
  start: u8,
  end: u8,
}

impl ByteInterval {
  pub const ANY: Self = Self { start: 0x00, end: 0xff };
  pub const EMPTY: Self = Self { start: 0xff, end: 0x00 };
}

impl core::fmt::Debug for ByteInterval {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    if self.start > self.end { return write!(f, "∅"); }
    if self.start == self.end { return write!(f, "{:#02x}", self.start) }
    if self.start != 0 { write!(f, "{:#02x}", self.start)?; }
    write!(f, "..")?;
    if self.end != 0xff { write!(f, "={:#02x}", self.end)?; }
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
const fn compile<const N: usize, const M: usize>(start: &[u8; N], end: &[u8; N], lsb: bool)
  -> [[ByteInterval; N]; M]
{
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

  const fn offset<const N: usize>(off: usize, lsb: bool) -> usize {
    if lsb { N - off - 1 } else { off }
  }

  let mut res = [[ByteInterval::EMPTY; N]; M];
  let mut res_count = 0;

  // Compute start offset.
  let mut off = 0;
  while off < N && start[off] == end[off] {
    off += 1;
  }

  // All bytes are equals.
  if off == N {
    res[0] = range::<N>(N, start, lsb);
    return res;
  }

  // First sub-range:
  let mut sub = range::<N>(N, start, lsb);
  sub[offset::<N>(N - 1, lsb)].end = if off == N - 1 { end[N - 1] } else { 0xff };
  res[res_count] = sub;
  res_count += 1;

  // Only the last byte is different.
  if off + 1 >= N {
    return res;
  }

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

  // Middle range:
  sub = range::<N>(off, start, lsb);
  sub[offset::<N>(off, lsb)].start = start[off] + 1;
  sub[offset::<N>(off, lsb)].end = end[off] - 1;
  res[res_count] = sub;
  res_count += 1;

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
  sub[offset::<N>(N - 1, lsb)].start = if off == N - 1 { start[N - 1] } else { 0x00 };
  res[res_count] = sub;

  res
}

fn main() {
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0x44], false));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0xab], false));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0xab, 0xcd], false));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0xAB, 0xCD, 0xEF, 0xFE], false));

  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0x44], true));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0x33, 0xab], true));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0x11, 0x22, 0xab, 0xcd], true));
  println!("{:?}", compile::<4, 7>(&[0x11, 0x22, 0x33, 0x44], &[0xAB, 0xCD, 0xEF, 0xFE], true));

  println!("{:?}", compile::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0x33, 0x44], false));
  println!("{:?}", compile::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0x33, 0xab], false));
  println!("{:?}", compile::<3, 5>(&[0x22, 0x33, 0x44], &[0x22, 0xab, 0xcd], false));
  println!("{:?}", compile::<3, 5>(&[0x22, 0x33, 0x44], &[0xCD, 0xEF, 0xFE], false));

  println!("{:?}", compile::<2, 3>(&[0x11, 0x22], &[0xab, 0xcd], false));
  println!("{:?}", compile::<2, 3>(&[0x11, 0x22], &[0x11, 0xcd], false));

  println!("{:?}", compile::<1, 1>(&[0x11], &[0xab], false));
  println!("{:?}", compile::<1, 1>(&[0x11], &[0x11], false));
}