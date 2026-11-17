use core::num::NonZero;

use crate::{
  Pun, Scalar,
  analysis::{Exact, analyze, analyze_tail, mutable},
  layout::{Endian, Spec},
};

/// What the analysis says of a pun from `S` to `D`.
fn verdict<S: ?Sized + Pun, D: ?Sized + Pun>() -> String {
  let a = const { analyze(&S::LAYOUT, &D::LAYOUT) };
  let fit = match a.fit.exact {
    Exact::Always => String::new(),
    Exact::Sometimes => " (some lengths)".into(),
    Exact::Never(why) if a.fit.shrink => format!(" (truncated: {why})"),
    Exact::Never(why) => return format!("never: {why}"),
  };
  let relation = match () {
    () if a.relation.uninit.is_some() => {
      return format!("uninit at {}", a.relation.uninit.unwrap());
    }
    () if a.relation.disjoint.is_some() => {
      return format!("unreachable at {}", a.relation.disjoint.unwrap());
    }
    () if a.relation.infallible() => "infallible".to_string(),
    () => format!("fallible at {}", a.relation.partial.unwrap()),
  };
  let mutable = if const { mutable(&S::LAYOUT, &D::LAYOUT, &analyze(&S::LAYOUT, &D::LAYOUT).fit) } {
    ", mutable"
  } else {
    ""
  };
  format!("{relation}{mutable}{fit}")
}

macro_rules! scalar {
  ($name:ident($repr:ty) = $endian:ident $ranges:expr) => {
    #[derive(Copy, Clone)]
    #[repr(transparent)]
    struct $name($repr);
    // SAFETY: these types are only given to the analysis, never constructed: there is no value
    // of them that `SPEC` could misdescribe.
    unsafe impl Scalar for $name {
      const SPEC: Spec = Spec::ranges::<$repr>(Endian::$endian, &$ranges);
    }
  };
}

scalar!(Ascii(u8) = Little[(0, 0x7f)]);
scalar!(NzLe(u16) = Little[(1, 0xffff)]);
scalar!(NzBe(u16) = Big[(1, 0xffff)]);
scalar!(Le1234(u16) = Little[(0x1234, 0x1234)]);
scalar!(Be3412(u16) = Big[(0x3412, 0x3412)]);
scalar!(Be1234(u16) = Big[(0x1234, 0x1234)]);
scalar!(GeLe(u32) = Little[(0x100, 0xffff_ffff)]);
scalar!(GeBe(u32) = Big[(0x100, 0xffff_ffff)]);
scalar!(Enum3(u32) = Little [(0x0101_0101, 0x0101_0101), (0x0202_0202, 0x0202_0202), (0x0303_0303, 0x0303_0303)]);

#[derive(Pun)]
#[repr(C)]
#[pun(crate)]
struct Pair<A, B: ?Sized>(A, B);

#[derive(Pun)]
#[repr(C, packed)]
#[pun(crate)]
struct Packed<A, B>(A, B);

#[test]
fn scalars() {
  assert_eq!(verdict::<u8, u8>(), "infallible, mutable");
  assert_eq!(verdict::<bool, u8>(), "infallible");
  assert_eq!(verdict::<u8, bool>(), "fallible at 0, mutable");
  assert_eq!(verdict::<u32, char>(), "fallible at 0, mutable");
  assert_eq!(verdict::<char, u32>(), "infallible");
  assert_eq!(verdict::<bool, NonZero<u8>>(), "fallible at 0");
  assert_eq!(verdict::<Ascii, bool>(), "fallible at 0, mutable");
  assert_eq!(verdict::<u16, u8>(), "infallible (truncated: the source is larger than the target)");
  assert_eq!(verdict::<u8, u16>(), "never: the source is smaller than the target");
  assert_eq!(verdict::<Enum3, NonZero<u32>>(), "infallible");
  assert_eq!(verdict::<NonZero<u32>, Enum3>(), "fallible at 0, mutable");
}

#[test]
fn padding() {
  // `Pair<u8, u16>` has a padding byte at 1.
  assert_eq!(verdict::<Pair<u8, u16>, [u8; 4]>(), "uninit at 1");
  assert_eq!(verdict::<[u8; 4], Pair<u8, u16>>(), "infallible");
  assert_eq!(verdict::<Pair<u8, u16>, Pair<bool, u16>>(), "fallible at 0, mutable");
  assert_eq!(verdict::<Pair<bool, u16>, Pair<u8, u16>>(), "infallible");
  assert_eq!(verdict::<Pair<u8, u16>, Pair<u8, i16>>(), "infallible, mutable");
  // A target without padding cannot read the padding of the source.
  assert_eq!(verdict::<Pair<u8, u16>, Pair<u8, [u8; 2]>>(), "uninit at 1");
  assert_eq!(verdict::<Pair<u8, u16>, Packed<u8, Packed<u8, u16>>>(), "uninit at 1");
  // ... and a `&mut` to a target with padding could de-initialize bytes of the source.
  assert_eq!(verdict::<[u8; 4], Pair<u8, u16>>(), "infallible");
}

#[test]
fn misaligned_and_mixed_endian() {
  // Whatever the byte order, a non-zero `u16` is a non-zero `u16`.
  assert_eq!(verdict::<NzLe, NzBe>(), "infallible, mutable");
  assert_eq!(verdict::<Le1234, Be3412>(), "infallible, mutable");
  assert_eq!(verdict::<Le1234, NzBe>(), "infallible");
  // ... but being at least 0x100 depends on it.
  assert_eq!(verdict::<GeLe, GeBe>(), "fallible at 0");
  assert_eq!(verdict::<GeLe, GeLe>(), "infallible, mutable");
  // The first byte of a little-endian `u32 >= 0x100` can be anything, the others cannot be all 0.
  assert_eq!(verdict::<GeLe, Pair<u8, [u8; 3]>>(), "infallible");
  assert_eq!(verdict::<GeLe, [bool; 4]>(), "fallible at 0");
  assert_eq!(verdict::<[Le1234; 2], [u8; 4]>(), "infallible");
  // In memory: `34 12 34 12`.
  assert_eq!(verdict::<[Le1234; 2], Packed<u8, Packed<Be1234, u8>>>(), "infallible");
  assert_eq!(verdict::<[Le1234; 2], Packed<u8, Packed<Le1234, u8>>>(), "unreachable at 1");
  assert_eq!(verdict::<[u8; 2], NzLe>(), "fallible at 0, mutable");
  assert_eq!(verdict::<Pair<u8, NonZero<u8>>, NzLe>(), "infallible");
  assert_eq!(verdict::<Pair<NonZero<u8>, u8>, NzBe>(), "infallible");
}

#[test]
fn huge_arrays_cost_nothing() {
  // As large as the target allows (the largest array is `15 * N` bytes).
  const N: usize = if usize::BITS >= 64 { 1 << 40 } else { 1 << 24 };
  assert_eq!(verdict::<[u8; N], [u8; N]>(), "infallible, mutable");
  assert_eq!(verdict::<[bool; N], [u8; N]>(), "infallible");
  assert_eq!(verdict::<[u8; N], [bool; N]>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[[bool; 4]; N], [u32; N]>(), "infallible");
  assert_eq!(verdict::<[Pair<u8, u16>; N], [u32; N]>(), "uninit at 1");
  assert_eq!(verdict::<[Pair<bool, [u8; 3]>; N], [Pair<u8, [bool; 3]>; N]>(), "fallible at 1");
  assert_eq!(
    verdict::<[[u8; 3]; N], [[bool; 5]; N]>(),
    "never: the source is smaller than the target"
  );
  assert_eq!(verdict::<[[Ascii; 5]; 3 * N], [[bool; 3]; 5 * N]>(), "fallible at 0, mutable");
  // Out of phase: 2 bytes elements against 2 bytes elements, one byte later.
  assert_eq!(verdict::<[NzLe; N], Packed<u8, Packed<[NzLe; N - 1], u8>>>(), "fallible at 1");
  assert_eq!(verdict::<[Le1234; N], Packed<u8, Packed<[Be1234; N - 1], u8>>>(), "infallible");
}

#[test]
fn unsized_types() {
  assert_eq!(verdict::<[u8], [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<[bool], [u8]>(), "infallible");
  assert_eq!(verdict::<[u8], [bool]>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[u32], [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<[u8], [u32]>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<[[u8; 3]], [[u8; 2]]>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<[u8; 8], [u16]>(), "infallible, mutable");
  assert_eq!(
    verdict::<[u8; 7], [u16]>(),
    "infallible (truncated: the size of the source cannot be reached with elements of the tail slice of the target)"
  );
  assert_eq!(verdict::<[u8], u32>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<Pair<u16, [u16]>, [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<Pair<u16, [Pair<u8, bool>]>, Pair<u8, [bool]>>(), "fallible at 1, mutable");
  // A source without any element is too small.
  assert_eq!(
    verdict::<Pair<bool, [Pair<bool, bool>]>, Pair<[bool; 3], [u8]>>(),
    "infallible (some lengths)"
  );
  // `Pair<u32, [u8]>` has trailing padding, but none in what a `[u8]` can see of it.
  assert_eq!(verdict::<Pair<u32, [u8]>, [u8]>(), "infallible, mutable");
  assert_eq!(<Pair<u32, [u8]> as Pun>::LAYOUT.size_of_val(1), Some(8));
  assert_eq!(<Pair<u32, [u8]> as Pun>::LAYOUT.size_of_val(5), Some(12));
}

/// What the analysis says of the last bytes of `S` as a `D`.
fn tail<S: ?Sized + Pun, D: ?Sized + Pun>() -> String {
  let a = const { analyze_tail(&S::LAYOUT, &D::LAYOUT) };
  let shift = a.shift;
  if let (Exact::Never(why), false) = (a.fit.exact, a.fit.shrink) {
    return format!("never: {why}");
  }
  let relation = match () {
    () if a.relation.uninit.is_some() => {
      format!("uninit at {}", a.relation.uninit.unwrap() - shift)
    }
    () if a.relation.disjoint.is_some() => {
      format!("unreachable at {}", a.relation.disjoint.unwrap() - shift)
    }
    () if a.relation.infallible() => "infallible".to_string(),
    () => format!("fallible at {}", a.relation.partial.unwrap() - shift),
  };
  let aligned = if a.aligned { "" } else { ", misaligned" };
  format!("{relation} from {shift}{aligned}")
}

#[test]
fn tails() {
  assert_eq!(tail::<[u8; 4], [u8; 2]>(), "infallible from 2");
  assert_eq!(tail::<[u8; 4], [u8; 4]>(), "infallible from 0");
  assert_eq!(tail::<[bool; 3], u16>(), "infallible from 1, misaligned");
  assert_eq!(tail::<[u8; 3], [bool; 2]>(), "fallible at 0 from 1");
  assert_eq!(tail::<Pair<bool, u8>, bool>(), "fallible at 0 from 1");
  assert_eq!(tail::<Pair<u8, bool>, bool>(), "infallible from 1");
  // `Pair<u8, u16>` has a padding byte at 1, `Pair<u16, u8>` at 3.
  assert_eq!(tail::<Pair<u8, u16>, u16>(), "infallible from 2");
  assert_eq!(tail::<Pair<u8, u16>, [u8; 3]>(), "uninit at 0 from 1");
  assert_eq!(tail::<Pair<u16, u8>, u8>(), "uninit at 0 from 3");
  assert_eq!(tail::<Pair<u16, u8>, Pair<u16, u8>>(), "infallible from 0");
  // Scalars cut by the shift: the first byte of a little-endian `u32 >= 0x100` can be anything,
  // the others cannot be all 0, but the last two can.
  assert_eq!(tail::<GeLe, [u8; 3]>(), "infallible from 1");
  assert_eq!(tail::<GeLe, Packed<u8, NzLe>>(), "fallible at 1 from 1");
  assert_eq!(tail::<GeLe, Packed<u8, [u8; 2]>>(), "infallible from 1");
  assert_eq!(tail::<Pair<u8, GeBe>, GeBe>(), "infallible from 4");
  assert_eq!(tail::<[Le1234; 2], Packed<Be3412, u8>>(), "unreachable at 0 from 1");
  assert_eq!(tail::<[Le1234; 2], Packed<Be1234, u8>>(), "infallible from 1");
  // Sizes.
  assert_eq!(tail::<u8, u16>(), "never: the source is smaller than the target");
  assert_eq!(
    tail::<[u8], u8>(),
    "never: the source is unsized: where its last bytes start depends on its length, which is \
     only known at run time.\nUse `from_tail` to take a value from the end of bytes"
  );
  assert_eq!(
    tail::<[u8; 4], [u8]>(),
    "never: the target is unsized: its extent cannot be found from the end of the source"
  );
}

#[test]
fn huge_tails_cost_nothing() {
  const N: usize = if usize::BITS >= 64 { 1 << 40 } else { 1 << 24 };
  assert_eq!(tail::<[u8; N], [u8; N]>(), "infallible from 0");
  assert_eq!(tail::<[u8; N], [bool; 7]>(), format!("fallible at 0 from {}", N - 7));
  assert_eq!(tail::<[bool; N], [u16; 3]>(), format!("infallible from {}, misaligned", N - 6));
  // In memory, `34 12 34 12 ..`; from 1, `12 34 12 34 .. 12`: out of phase, every period.
  assert_eq!(tail::<[Le1234; N], Packed<[Be1234; N - 1], u8>>(), "infallible from 1");
  assert_eq!(tail::<[Le1234; N], Packed<[Le1234; N - 1], u8>>(), "unreachable at 0 from 1");
  assert_eq!(tail::<[NzLe; N], [NzLe; N - 1]>(), "infallible from 2");
  assert_eq!(tail::<[NzLe; N], Packed<u8, [NzLe; N - 1]>>(), "infallible from 1");
  assert_eq!(tail::<[NzLe; N], Packed<[NzLe; N - 1], u8>>(), "fallible at 0 from 1");
  // A padding byte every 4, from 1.
  assert_eq!(tail::<[Pair<u8, u16>; N], [u16; 2 * N - 1]>(), "uninit at 3 from 2");
  assert_eq!(tail::<[Pair<u8, u16>; N], Pair<u16, [Pair<u8, u16>; N - 1]>>(), "infallible from 2");
}
