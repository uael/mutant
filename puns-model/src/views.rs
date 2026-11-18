//! View enums (an enum whose variants each hold a reference, see `#[derive(Pun)]`) against what
//! they mean.
//!
//! What a view means is the first variant that the bytes are (start with, for `from_head`), found
//! by trying each variant in turn with `from_bytes` and `from_head` of its type, which the rest of
//! the model checks against the oracles of the types.
//!
//! The derive decides a variant from its *base*, a later variant whose type takes every value of
//! the type of the variant, and keeps what it decided (`puns::view`). For every pattern of bytes
//! up to a length, at an aligned and a misaligned address, the variant that it returns, with its
//! address, its size and what remains, must be the one of trying each in turn: a variant that its
//! type does not take from the bytes, or with another address or size, is unsound; another
//! variant, or none, is incorrect. How many variants have a base is counted: views without one
//! would check nothing new.

use puns::{
  Pun,
  view::{First, Here, Second, base_of},
};

use crate::{alphabet, record};

/// A view of two types: every ordered pair of types of the model is one.
#[derive(Pun)]
pub enum Two<'a, A: ?Sized + Pun, B: ?Sized + Pun> {
  A(&'a A),
  B(&'a B),
}

/// A view of three types: a chain of bases, a base shared by two variants.
#[derive(Pun)]
pub enum Three<'a, A: ?Sized + Pun, B: ?Sized + Pun, C: ?Sized + Pun> {
  A(&'a A),
  B(&'a B),
  C(&'a C),
}

/// What a view, or a variant, takes of bytes: the index of the variant, its address and its
/// size, and the number of bytes after it.
pub type Found = Option<(usize, usize, usize, usize)>;

/// A view of the model.
pub trait View {
  /// The types of its variants.
  fn name() -> String;
  /// The number of its variants.
  const LEN: usize;
  /// `from_bytes` (`from_head` if `head`) of the view.
  fn view(bytes: &[u8], head: bool) -> Found;
  /// `from_bytes` (`from_head` if `head`) of the type of the variant `index`, alone.
  fn variant(bytes: &[u8], head: bool, index: usize) -> Found;
  /// How many variants have a base, for `from_bytes` and for `from_head`.
  fn bases() -> (usize, usize);
}

/// What `value`, the variant `index` of bytes, is: see [`Found`].
#[expect(clippy::unnecessary_wraps, reason = "what is found, which each caller returns")]
fn found<T: ?Sized>(index: usize, value: &T, rest: usize) -> Found {
  Some((index, core::ptr::from_ref(value).cast::<u8>().addr(), size_of_val(value), rest))
}

/// `from_bytes` or `from_head` of `T`, as [`Found`].
fn alone<T: ?Sized + Pun>(bytes: &[u8], head: bool, index: usize) -> Found {
  if head {
    let (value, rest) = puns::from_head::<T>(bytes).ok()?;
    found(index, value, rest.len())
  } else {
    found(index, puns::from_bytes::<T>(bytes).ok()?, 0)
  }
}

/// A base, counted.
const fn based(base: usize) -> usize {
  (base != usize::MAX) as usize
}

impl<A: ?Sized + Pun, B: ?Sized + Pun> View for Two<'static, A, B> {
  const LEN: usize = 2;

  fn name() -> String {
    format!("Two<{}, {}>", crate::name::<A>(), crate::name::<B>())
  }

  fn view(bytes: &[u8], head: bool) -> Found {
    let (view, rest) = if head {
      Two::<A, B>::from_head(bytes).ok()?
    } else {
      (Two::<A, B>::from_bytes(bytes).ok()?, &[][..])
    };
    match view {
      Two::A(value) => found(0, value, rest.len()),
      Two::B(value) => found(1, value, rest.len()),
    }
  }

  fn variant(bytes: &[u8], head: bool, index: usize) -> Found {
    match index {
      0 => alone::<A>(bytes, head, 0),
      _ => alone::<B>(bytes, head, 1),
    }
  }

  fn bases() -> (usize, usize) {
    // The variants are a tree of two leaves: `A` is the first. `B`, the last, has no base.
    let whole = const { based(base_of::<Self, First<Here>, false>()) };
    let head = const { based(base_of::<Self, First<Here>, true>()) };
    (whole, head)
  }
}

impl<A: ?Sized + Pun, B: ?Sized + Pun, C: ?Sized + Pun> View for Three<'static, A, B, C> {
  const LEN: usize = 3;

  fn name() -> String {
    format!("Three<{}, {}, {}>", crate::name::<A>(), crate::name::<B>(), crate::name::<C>())
  }

  fn view(bytes: &[u8], head: bool) -> Found {
    let (view, rest) = if head {
      Three::<A, B, C>::from_head(bytes).ok()?
    } else {
      (Three::<A, B, C>::from_bytes(bytes).ok()?, &[][..])
    };
    match view {
      Three::A(value) => found(0, value, rest.len()),
      Three::B(value) => found(1, value, rest.len()),
      Three::C(value) => found(2, value, rest.len()),
    }
  }

  fn variant(bytes: &[u8], head: bool, index: usize) -> Found {
    match index {
      0 => alone::<A>(bytes, head, 0),
      1 => alone::<B>(bytes, head, 1),
      _ => alone::<C>(bytes, head, 2),
    }
  }

  fn bases() -> (usize, usize) {
    // The variants are a tree of `A`, then of `B` and `C`. `C`, the last, has no base.
    type A = First<Here>;
    type B = Second<First<Here>>;
    let whole = const { based(base_of::<Self, A, false>()) + based(base_of::<Self, B, false>()) };
    let head = const { based(base_of::<Self, A, true>()) + based(base_of::<Self, B, true>()) };
    (whole, head)
  }
}

/// Compare `from_bytes` and `from_head` of the view `V` with trying each of its variants in turn.
///
/// For every pattern of `lengths` bytes over the alphabet of values of that size (all bytes up to
/// 2; with `sample`, the alphabet of 3 bytes for those, which has the values that the types of
/// the model tell apart), at an address aligned for 8 bytes and at the next one.
///
/// # Panics
/// If the report is poisoned.
pub fn check_view<V: View>(lengths: &[usize], sample: bool) {
  let name = V::name();
  let (mut unsound, mut incorrect, mut checked) = (vec![], vec![], 0);
  'all: for &len in lengths {
    let of = if sample { len.max(3) } else { len };
    let alphabet: Vec<u8> = alphabet(of).into_iter().flatten().collect();
    let mut pattern = vec![0usize; len];
    loop {
      let bytes: Vec<u8> = pattern.iter().map(|&i| alphabet[i]).collect();
      for misalign in [0, 1] {
        let mut words = [0u64; 5];
        let all: &mut [u8; 40] = puns::pun_mut(&mut words);
        all[misalign..misalign + len].copy_from_slice(&bytes);
        let source = &all[misalign..misalign + len];
        for head in [false, true] {
          checked += 1;
          let got = V::view(source, head);
          let each: Vec<Found> = (0..V::LEN).map(|i| V::variant(source, head, i)).collect();
          let want = each.iter().find_map(|found| *found);
          let what = if head { "from_head" } else { "from_bytes" };
          if let Some((index, ..)) = got
            && each[index] != got
          {
            unsound.push(format!(
              "{name}: {what}({bytes:02x?} at +{misalign}) is {got:?}, which its type is not \
               ({:?})",
              each[index]
            ));
          } else if got != want {
            incorrect.push(format!(
              "{name}: {what}({bytes:02x?} at +{misalign}) is {got:?}, want {want:?}"
            ));
          }
          if unsound.len() + incorrect.len() > 3 {
            break 'all;
          }
        }
      }
      // Next pattern.
      let mut i = 0;
      while i < len {
        pattern[i] += 1;
        if pattern[i] < alphabet.len() {
          break;
        }
        pattern[i] = 0;
        i += 1;
      }
      if i == len {
        break;
      }
    }
  }
  let (whole, head) = V::bases();
  record(|report| {
    report.views += 1;
    report.view_bases += whole + head;
    report.conversions += checked;
    report.unsound.extend(unsound);
    report.incorrect.extend(incorrect);
  });
}

/// Every ordered pair of the types as a [`Two`], through `check_view`, over `lengths` (a sample
/// of the patterns if `sample`).
#[macro_export]
macro_rules! view_pairs {
  ($lengths:expr, $sample:expr; $($ty:ty),+ $(,)?) => {
    $crate::view_pairs!(@outer ($lengths, $sample); [$($ty),+]; $($ty),+);
  };
  (@outer $args:tt; $all:tt; $($a:ty),+) => {
    $( $crate::view_pairs!(@inner $args; $a; $all); )+
  };
  (@inner ($lengths:expr, $sample:expr); $a:ty; [$($b:ty),+]) => {
    $( $crate::views::check_view::<$crate::views::Two<'static, $a, $b>>($lengths, $sample); )+
  };
}
