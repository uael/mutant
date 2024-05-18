mod tmp;

use core::num::NonZeroU8;
use core::num::NonZeroUsize;
use core::marker::PhantomData;

/// Unwrap a `Result` and return the error if it is `Err`.
#[rustfmt::skip]
macro_rules! bail {
  ($e:expr $(,)?) => (match $e { Ok(r) => r, Err(e) => return Err(e) });
  ($e:expr, $x:ident => $map:expr $(,)?) => (match $e { Ok(r) => r, Err($x) => return Err($map) });
}

/// Check a condition and return an error if it is `false`.
#[rustfmt::skip]
macro_rules! check { ($c:expr, $e:expr $(,)?) => { if (!$c) { return Err($e); } } }

/// Same as iter.fold() but for const fns.
#[rustfmt::skip]
macro_rules! fold {
  (..$len:expr, $init:expr, |$i:tt, $acc:tt| $x:expr) => {{
    let (mut $i, mut $acc) = (0, $init);
    loop {
      if $i >= $len { break $acc; }
      ($acc, $i) = ($x, $i + 1);
    }
  }};
  ($iter:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $iter.len(), $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    fold!($iter, $iter.len(), $init, |$i, $acc, $it| $x)
  }};
  ($iter:expr, $len:expr, $init:expr, |$acc:tt, $it:tt| $x:expr) => (
    fold!($iter, $len, $init, |__i, $acc, $it| $x)
  );
  ($iter:expr, $len:expr, $init:expr, |$i:tt, $acc:tt, $it:tt| $x:expr) => {{
    let (mut $i, mut $acc) = (0, $init);
    loop {
      if $i >= $len { break $acc; }
      let $it = &$iter[$i];
      ($acc, $i) = ($x, $i + 1);
    }
  }};
}

/// Unwrap an `Option` and panic if it is `None`.
#[rustfmt::skip]
macro_rules! unwrap {
  ($e:expr, $none:expr $(,)?) => (match $e { Some(r) => r, None => $none });
  ($e:expr $(,)?) => (unwrap!($e, unreachable!()));
}

#[derive(Debug, Copy, Clone)]
struct Interval {
  start: u8,
  end: u8,
}

#[derive(Debug, Copy, Clone)]
enum Byte {
  Initialized(Interval),
  Uninitialized,
}

#[derive(Debug, Copy, Clone)]
enum MaybeOob<T> {
  InBounds(T),
  OutOfBounds,
}

#[derive(Debug, Copy, Clone)]
enum Intersection {
  Empty,
  Complete,
  Incomplete,
}

/// Enumerates metadata constraints for transmutations involving dynamically sized types (DSTs).
///
/// # Internals
/// DSTs metadata represents the length of it's [dst slice](#dst-slice), such as `.1` in
/// `(u16, [u16])`. These checks [validate](MetadataChecks::validate) such metadata to ensure safe
/// and sound transmutations.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MetadataChecks {
  Is(usize),                             // Is equals to `n`
  IsAtLeast(usize),                      // Is at least `n`
  IsMultipleOf(usize),                   // Is a multiple of `m`.
  IsAtLeastThenMultipleOf(usize, usize), // Is at least `n` and a multiple of `m`.
  IsEmpty,                               // Implies no data, i.e. should be zero.
}

enum Tag {
  TVoid, TAny, TAll, TJoin,
  TByte { byte: Byte, mask: Option<NonZeroU8> },
  TOff { off: usize }, TArr { len: NonZeroUsize },
}

/// Layout is a single initialized byte ranging from `START` to `END`.
struct Init<const START: u8 = 0, const END: u8 = 0xff, const MASK: u8 = 0xff>;

/// Layout is a single uninitialized byte.
struct Uninit;

/// Layout is `T` offset by `OFF` bytes.
struct Off<const OFF: usize, T>(PhantomData<T>);

/// Where `T` is a tuple of layouts, layout is any item of `T`.
struct Any<T>(PhantomData<T>);

/// Where `T` is a tuple of layouts, layout is all items of `T`.
struct All<T>(PhantomData<T>);

/// Where `T` is a tuple of layouts, layout is all concatenated items of `T`.
///
/// # Internals
/// `Join` isn't required and could be implemented using `All` and `Off`, but it adds some
/// indirection hence reducing the risk to reach the recursion limit.
struct Join<T>(PhantomData<T>);

trait BitsPattern: Sized {
  const TAG: Tag;
  type Lhs: BitsPattern;
  type Rhs: BitsPattern;
}

struct Layout<S, T> {
  sized: S,
  dst_slice_item: Option<T>,
}

use Byte::*;
use MaybeOob::*;
use Tag::*;
use Intersection::*;
use MetadataChecks::*;

const fn size<T: BitsPattern>() -> usize {
  const {
    match T::TAG {
      TVoid => 0,
      TByte { .. } => 1,
      TOff { off } => off + size::<T::Lhs>(),
      TArr { len } => len.get() * size::<T::Lhs>(),
      TJoin => size::<T::Lhs>() + size::<T::Rhs>(),
      _ => match (size::<T::Lhs>(), size::<T::Rhs>()) {
        (a, b) if a > b => a,
        (_, b) => b
      },
    }
  }
}

const fn is_any<T: BitsPattern, const ACCEPT_UNINIT: bool>() -> bool {
  const {
    match T::TAG {
      TVoid => true,
      TByte { byte: Uninitialized, .. } => ACCEPT_UNINIT,
      TByte { byte: Initialized(interval), .. } => interval.len() == 256,
      TOff { .. } => is_any::<T::Lhs, ACCEPT_UNINIT>(),
      TArr { .. } => is_any::<T::Lhs, ACCEPT_UNINIT>(),
      _ => is_any::<T::Lhs, ACCEPT_UNINIT>() && is_any::<T::Rhs, ACCEPT_UNINIT>()
    }
  }
}

impl Intersection {
  const fn and(self, other: Self) -> Self {
    match (self, other) {
      (Complete, Complete) => Complete,
      (Empty, _) | (_, Empty) => Empty,
      _ => Incomplete,
    }
  }
}

const fn intersects<A: BitsPattern, B: BitsPattern>(x: usize) -> Result<Intersection, &'static str> {
  let ok = match (A::TAG, B::TAG) {
    (TVoid, _) => Complete,
    (TByte { byte, .. }, _) => match (byte, byte.intersects::<B>(x)) {
      (_, OutOfBounds) => return Err("out-of-bounds"),
      (Initialized(_), InBounds(Uninitialized)) => return Err("uninitialized byte"),
      (Uninitialized, InBounds(Uninitialized)) => Complete,
      (Initialized(a), InBounds(Initialized(b))) if a.len() == b.len() => Complete,
      (_, InBounds(Initialized(b))) if b.len() > 0 => Incomplete,
      _ => Incomplete,
    },
    (TOff { off }, _) => return intersects::<A::Lhs, B>(x + off),
    (TArr { len }, _) => fold!(..len.get(), bail!(intersects::<A::Lhs, B>(x)), |j, acc| match acc {
      Incomplete => Incomplete,
      i => i.and(bail!(intersects::<A::Lhs, B>(x + j * size::<A::Lhs>())))
    }),
    (TAll, _) => match bail!(intersects::<A::Lhs, B>(x)) {
      Incomplete => Incomplete,
      i => i.and(bail!(intersects::<A::Rhs, B>(x))),
    }
    (TAny, _) => match (bail!(intersects::<A::Lhs, B>(x)), bail!(intersects::<A::Rhs, B>(x))) {
      (Complete, Complete) => Complete,
      (Empty, Empty) => Empty,
      _ => Incomplete,
    }
    (TJoin, _) => match bail!(intersects::<A::Lhs, B>(x)) {
      Incomplete => Incomplete,
      i => i.and(bail!(intersects::<A::Rhs, B>(x + size::<A::Lhs>())))
    }
  };
  Ok(ok)
}

#[inline(always)]
const unsafe fn matches<T: BitsPattern>(ptr: *const u8) -> bool {
  if is_any::<T, true>() {
    return true;
  }
  match T::TAG {
    TVoid => true,
    TAny => matches::<T::Lhs>(ptr) || matches::<T::Rhs>(ptr),
    TAll => matches::<T::Lhs>(ptr) && matches::<T::Rhs>(ptr),
    TJoin => matches::<T::Lhs>(ptr) && matches::<T::Rhs>(ptr.add(size::<T::Lhs>())),
    TByte { byte, mask: Some(mask) } => byte.contains(*ptr & mask.get()),
    TByte { byte, .. } => byte.contains(*ptr),
    TOff { off } => matches::<T::Lhs>(ptr.add(off)),
    TArr { len } => fold!(..len.get(),
      true,
      |j, acc| acc && matches::<T::Lhs>(ptr.add(j * size::<T::Lhs>()))
    )
  }
}

// -------------------------------------------------------------------------------------------------
// Internal implements.

include!(concat!(env!("OUT_DIR"), "/impls.rs"));

const _: () =  {

  // -----------------------------------------------------------------------------------------------
  // impl `Bits`

  impl BitsPattern for () {
    const TAG: Tag = TVoid;
    type Lhs = ();
    type Rhs = ();
  }

  impl BitsPattern for Uninit {
    const TAG: Tag = TByte { byte: Uninitialized, mask: None };
    type Lhs = ();
    type Rhs = ();
  }

  impl<const START: u8, const END: u8, const MASK: u8> BitsPattern for Init<START, END, MASK> {
    const TAG: Tag = TByte {
      byte: Initialized(Interval { start: START, end: END }),
      mask: if MASK == 0xff { None } else { Some(unwrap!(NonZeroU8::new(MASK))) }
    };
    type Lhs = ();
    type Rhs = ();
  }

  impl<const OFF: usize, T: BitsPattern> BitsPattern for Off<OFF, T> {
    const TAG: Tag = TOff { off: OFF };
    type Lhs = T;
    type Rhs = ();
  }

  impl<const N: usize, T: BitsPattern> BitsPattern for [T; N] {
    const TAG: Tag = TArr { len: unwrap!(NonZeroUsize::new(N)) };
    type Lhs = T;
    type Rhs = ();
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Layout`

  impl<H: BitsPattern, T: BitsPattern> Layout<H, T> {
    const fn is_sized(&self) -> bool {
      self.dst_slice_item.is_none()
    }

    const fn min_size_of(&self) -> usize {
      size::<H>()
    }

    const fn size_of_dst_slice_item(&self) -> usize {
      size::<T>()
    }

    #[inline(always)]
    const unsafe fn matches(mut ptr: *const u8, mut metadata: usize) -> bool {
      if !matches::<H>(ptr) {
        return false;
      }
      ptr = ptr.add(size::<H>());
      while metadata > 0 {
        if !matches::<T>(ptr) {
          return false;
        }
        ptr = ptr.add(size::<T>());
        metadata -= 1;
      }
      true
    }

    const fn intersects<Dh: BitsPattern, Dt: BitsPattern>(
      &self, dst: &Layout<Dh, Dt>, oob: usize,
    ) -> Result<Intersection, &'static str> {
      // Alias self to `src`:
      let src = self;
      // let int = intersects::<H, Dh>(0)?;
      todo!()
    }

    const fn intersects_unidirectional<Dh: BitsPattern, Dt: BitsPattern>(
      &self, dst: &Layout<Dh, Dt>
    ) -> Result<(Intersection, Intersection), &'static str> {
      // Alias self to `src`:
      let src = self;
      // Try to resolve `xa[ya] => xb[yb]`, where:
      // - `a` and `b` are source and target respectively,
      // - `xa` and `xb` are the minimum sizes of `a` and `b` respectively,
      // - `ya` and `yb` are the DST slice item sizes of `a` and `b` respectively.
      // - `n` is the minimum number of the source DST slice items needed to align `a` and `b`.
      // - `m` is the source DST slice items multiplier.
      // Calculate minimum and DST slice item sizes for both source and target types.
      let (xa, ya) = (src.min_size_of(), src.size_of_dst_slice_item());
      let (xb, yb) = (dst.min_size_of(), dst.size_of_dst_slice_item());
      // Compute if source and target are DSTs, i.e. had a DST slice with a non-zero item size.
      let (src_is_dst, dst_is_dst) = (ya != 0, yb != 0);
      // Compute the minimum source DST slice items needed to align source and target.
      // 1. Source is smaller than target:
      //  - Check for source to be a DST.
      //  - Compute the missing bytes to fit the target.
      //  - In case the target isn't a DST, check if the missing bytes are a multiple of
      //    the source DST slice item size.
      //  - The minimum source DST slice items is the missing bytes ceil divided by the source DST
      //    slice item size.
      let mut n = if xa < xb {
        // Check for source to be a DST.
        check!(src_is_dst, "source too small to fit target");
        // Compute the missing bytes to fit the target.
        let missing = xb - xa;
        // In case the target isn't a DST:
        if !dst_is_dst {
          // Check if the missing bytes are a multiple of the source DST slice item size.
          check!(
          missing % ya == 0,
          "missing bytes to fit the target is not a multiple of the source DST trailing slice \
           item size"
        );
        }
        // The minimum source DST slice items is the missing bytes ceil divided by the source DST
        // slice item size.
        (missing + ya - 1) / ya
      }
      // 2. Source is larger or equal to target:
      //  - The minimum source DST slice items is 0.
      else {
        // In case the target is a DST:
        // - Compute the remaining bytes to fit in the target.
        // - In case the source isn't a DST, check if the remaining bytes is a multiple
        //   of the target DST slice item size.
        if dst_is_dst {
          // Compute the remaining bytes to fit in the target.
          let remaining = xa - xb;
          // In case the source isn't a DST:
          if !src_is_dst {
            // Check if the remaining bytes is a multiple of the target DST slice item size.
            check!(
            remaining % yb == 0,
            "remaining bytes to fit in the target is not a multiple of the target DST trailing \
             slice item size",
          );
          }
        }
        // The minimum source DST slice items is 0.
        0
      };
      // Compute the possible size mismatch between source and target.
      // 1. Source isn't a DST:
      let maybe_size_mismatch = if !src_is_dst {
        // No size mismatch.
        None
      }
      // 2. Source is a DST and target isn't a DST:
      else if !dst_is_dst {
        // In case the source is smaller than the target, the source metadata must be the minimum
        // source DST slice items count.
        // Otherwise, the source metadata must be empty.
        Some(if xa < xb { Is(n) } else { IsEmpty })
      }
      // 3. Source and target are DSTs:
      else {
        /// Greatest common divisor and least common multiple calculations to determine the
        /// alignment and size compatibility.
        const fn gcd(a: usize, b: usize) -> usize {
          match b {
            0 => a,
            _ => gcd(b, a % b),
          }
        }
        // Compute the GCD of the source and target DST trailing slice item sizes.
        let gcd = gcd(ya, yb);
        // Compute the absolute difference between the source and target DST minimum sizes.
        let absolute_miss = if xa < xb { xb - xa } else { xa - xb };
        // Make sure the source can catch up on the target.
        check!(absolute_miss % gcd == 0, "DST source cannot catch up on DST target");
        // Compute the minimum number of source DST slice items needed to align source and target.
        // We need to find `n` in a way that `xa + n*ya - yb` is a multiple of `yb`. Although this
        // could be resolved using complex equations, for the sake of readability, we can simplify
        // it by using the following loop:
        // - `n * ya` is the total size in bytes of the source DST slice for `n` source items.
        // - `xa + n * ya` is the total size in bytes of the source for `n` source items.
        // - `(xa + n * ya - xb)` is the total size of the target DST slice for `n` source items.
        // If the latter is a multiple of `yb`, then we found the minimum number of source DST slice
        // items needed to align source and target.
        while (xa + n * ya - xb) % yb != 0 {
          n += 1; // Increment the minimum number of source DST slice items by 1.
        }
        // Compute the metadata checks for the source DST slice.
        match (n, yb / gcd) {
          // No size mismatch.
          (0, 1) => None,
          // Source metadata must be a multiple of the GCD.
          (0, m) => Some(IsMultipleOf(m)),
          // Source metadata must be at least `n` and a multiple of `m`.
          (n, 1) => Some(IsAtLeast(n)),
          // Source metadata must be at least `n` and a multiple of `m`.
          (n, m) => Some(IsAtLeastThenMultipleOf(n, m)),
        }
      };
      // Compute the minimum size in bytes need to fully check `Src` and `Dst` bit patterns
      // compatibility (transmutability).
      let min_check_size = match maybe_size_mismatch {
        None if ya != 0 => xa + ya,
        None => xb,
        Some(IsEmpty) => xa,
        Some(IsMultipleOf(m)) => xa + (m * ya),
        Some(Is(n)) => xa + (n * ya),
        Some(IsAtLeast(n)) => xa + (n * ya) + ya,
        Some(IsAtLeastThenMultipleOf(n, m)) => xa + (n * ya) + (m * ya),
      };
      todo!()
    }
  }

  // -----------------------------------------------------------------------------------------------
  // impl `MaybeOob`

  impl MaybeOob<Byte> {
    const fn union(self, other: Self) -> Self {
      match (self, other) {
        (InBounds(a), InBounds(b)) => InBounds(a.union(b)),
        (InBounds(a), _) | (_, InBounds(a)) => InBounds(a),
        _ => OutOfBounds,
      }
    }

    const fn intersection(self, other: Self) -> Self {
      match (self, other) {
        (InBounds(a), InBounds(b)) => InBounds(a.intersection(b)),
        _ => OutOfBounds,
      }
    }
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Byte`

  impl Byte {
    const fn contains(&self, value: u8) -> bool {
      match self {
        Uninitialized => true,
        Initialized(interval) => interval.contains(value)
      }
    }

    const fn union(self, o: Self) -> Self {
      match (self, o) {
        (Initialized(a), Initialized(b)) => Initialized(a.union(b)),
        (Initialized(a), _) | (_, Initialized(a)) => Initialized(a),
        _ => Uninitialized,
      }
    }

    const fn intersection(self, o: Self) -> Self {
      match (self, o) {
        (Uninitialized, _) | (_, Uninitialized) => Uninitialized,
        (Initialized(a), Initialized(b)) => Initialized(a.intersects(b)),
      }
    }

    const fn intersects<T: BitsPattern>(&self, x: usize) -> MaybeOob<Self> {
      if x > size::<T>() {
        return OutOfBounds;
      }
      self.n::<T>(x, 0)
    }

    const fn n<T: BitsPattern>(&self, x: usize, y: usize) -> MaybeOob<Self> {
      if is_any::<T, false>() {
        return InBounds(*self);
      }
      match T::TAG {
        // This path is unreachable because of the above ifs, but left here for completeness.
        TVoid => OutOfBounds,
        TOff { off } => self.n::<T::Lhs>(x, y + off),
        TArr { len } => fold!(..len.get(),
          self.n::<T::Lhs>(x, y),
          |j, acc| acc.intersection(self.n::<T::Lhs>(x, y + (j * size::<T::Lhs>())))
        ),
        TAny => self.n::<T::Lhs>(x, y).union(self.n::<T::Rhs>(x, y)),
        TAll => self.n::<T::Lhs>(x, y).intersection(self.n::<T::Rhs>(x, y)),
        TJoin => self.n::<T::Lhs>(x, y).intersection(self.n::<T::Rhs>(x, y + size::<T::Lhs>())),
        _ if x != y => return InBounds(*self),
        TByte { byte, .. } => InBounds(self.intersection(byte)),
      }
    }
  }

  // -----------------------------------------------------------------------------------------------
  // impl `Interval`

  impl Interval {
    const fn len(self) -> usize {
      let Self { start, end } = self;
      if start > end { 0 } else { (end - start) as usize + 1 }
    }

    #[inline(always)]
    const fn contains(self, value: u8) -> bool {
      debug_assert!(self.start <= self.end);
      value >= self.start && value <= self.end
    }

    const fn union(self, o: Self) -> Self {
      match (self, o) {
        (a, b) if a.len() == 0 => b,
        (a, b) if b.len() == 0 => a,
        (Self { start: a, end: b }, Self { start: x, end: y }) => {
          let start = if a < x { a } else { x };
          let end = if b > y { b } else { y };
          Self { start, end }
        }
      }
    }

    const fn intersects(self, o: Self) -> Self {
      match (self, o) {
        (a, _) if a.len() == 0 => a,
        (_, b) if b.len() == 0 => b,
        (Self { start: a, end: b }, Self { start: x, end: y }) => {
          let start = if a > x { a } else { x };
          let end = if b < y { b } else { y };
          Self { start, end }
        }
      }
    }
  }
};

type _0or4 = Any<(
  Join<(Init<0, 0>, Init<0, 0xff>)>,
  Join<(Init<4, 4>, Init<0, 0xff>)>,
)>;

type _0or4or8 = Any<(
  _0or4,
  Join<(Init<8, 8>, Init<0, 0xff>)>,
)>;

type _0or4or8or12 = Any<(
  _0or4or8,
  Join<(Init<12, 12>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16 = Any<(
  _0or4or8or12,
  Join<(Init<16, 16>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20 = Any<(
  _0or4or8or12or16,
  Join<(Init<20, 20>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24 = Any<(
  _0or4or8or12or16or20,
  Join<(Init<24, 24>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24or28 = Any<(
  _0or4or8or12or16or20or24,
  Join<(Init<28, 28>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24or28or32 = Any<(
  _0or4or8or12or16or20or24or28,
  Join<(Init<32, 32>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24or28or32or36 = Any<(
  _0or4or8or12or16or20or24or28or32,
  Join<(Init<36, 36>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24or28or32or36or40 = Any<(
  _0or4or8or12or16or20or24or28or32or36,
  Join<(Init<40, 40>, Init<0, 0xff>)>,
)>;

type _0or4or8or12or16or20or24or28or32or36or40or44 = Any<(
  _0or4or8or12or16or20or24or28or32or36or40,
  Join<(Init<44, 44>, Init<0, 0xff>)>,
)>;

type Yolo = Join<(Init, Init, Init, Init, Init, Init, Init, Init, Init, Init, Init, Init, Init)>;
type Yala = Join<(Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo, Yolo)>;
type Toto = Join<(Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala, Yala)>;
type Titi = Join<(Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto, Toto)>;

#[test]
fn test() {
  println!("{}", size::<_0or4>());
  println!("{}", size::<_0or4or8>());
  println!("{}", size::<_0or4or8or12>());
  println!("{}", size::<_0or4or8or12or16>());
  println!("{}", size::<_0or4or8or12or16or20>());
  println!("{}", size::<_0or4or8or12or16or20or24>());
  println!("{}", size::<_0or4or8or12or16or20or24or28>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36or40>());
  println!("{}", size::<_0or4or8or12or16or20or24or28or32or36or40or44>());
  println!("{}", size::<Titi>());

  println!("{:?}", intersects::<_0or4, _0or4>(0));
  println!("{:?}", intersects::<_0or4or8, _0or4or8>(0));
  println!("{:?}", intersects::<_0or4, _0or4or8>(0));
  println!("{:?}", intersects::<_0or4or8, _0or4>(0));

  type BoolPattern = Init<0, 1>;
  type U8Pattern = Init<0, 0xff>;

  println!("{:?}", intersects::<BoolPattern, U8Pattern>(0));
  println!("{:?}", intersects::<U8Pattern, BoolPattern>(0));
  println!("{:?}", intersects::<Titi, Titi>(0));
}

pub const unsafe fn test_0or4(ptr: *const u8) -> bool {
  matches::<_0or4>(ptr)
}

pub const unsafe fn test_0or4or8(ptr: *const u8) -> bool {
  matches::<_0or4or8>(ptr)
}

pub const unsafe fn test_0or4or8or12(ptr: *const u8) -> bool {
  matches::<_0or4or8or12>(ptr)
}

pub const unsafe fn test_0or4or8or12or16(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20or24(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20or24>(ptr)
}

pub const unsafe fn test_0or4or8or12or16or20or24or28(ptr: *const u8) -> bool {
  matches::<_0or4or8or12or16or20or24or28>(ptr)
}
