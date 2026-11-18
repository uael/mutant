//! `validate_in` and `validate_elems_in`: a value in a buffer, everything but what its pointers
//! aim at, each pointer handed to a visitor. What each kind of layout answers (scalars, structs
//! and their rules, enums and niches, unions, `MaybeUninit`, `Option` and `Result`, pointers),
//! where an error is, and that no input makes them read out of the bytes or panic (under Miri on
//! every target of CI).

#[path = "support/relative.rs"]
#[expect(dead_code, reason = "what the model check uses, and these tests do not")]
mod relative;

use core::{mem::MaybeUninit, num::NonZero};

use puns::{Invalid, Pun, Visit, num::u8in};
use relative::{One, Opt, Seq, Text};

/// Records every pointer handed over: its position, and what it is.
#[derive(Default)]
struct Record {
  pointers: Vec<Handed>,
  /// Refuse the pointer at this position.
  refuse: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
enum Stop {
  Invalid(usize),
  Refused(usize),
}

impl From<Invalid> for Stop {
  fn from(invalid: Invalid) -> Self {
    Self::Invalid(invalid.at)
  }
}

impl Visit for Record {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    let pointer = const { P::LAYOUT.as_pointer() }.expect("only pointers are handed over");
    if self.refuse == Some(at) {
      return Err(Stop::Refused(at));
    }
    self.pointers.push((at, pointer.aim(), pointer.is_nullable()));
    Ok(())
  }
}

/// `validate_in::<T>` of `bytes` at `at`, and the pointers handed over.
/// A pointer handed over: its position, what it aims at, whatever it may be null.
type Handed = (usize, puns::layout::Aim, bool);

fn check<T: Pun>(bytes: &[u8], at: usize) -> (Result<(), Stop>, Vec<Handed>) {
  let mut record = Record::default();
  let result = puns::validate_in::<T, Record>(bytes, at, &mut record);
  (result, record.pointers)
}

fn ok<T: Pun>(bytes: &[u8], at: usize) -> Result<(), Stop> {
  check::<T>(bytes, at).0
}

// -------------------------------------------------------------------------------------------------
// Values without pointers

#[derive(Pun)]
#[repr(C)]
#[pun(assert(lo <= hi))]
struct Span {
  lo: u8,
  #[pun(valid(..=9))]
  hi: u8,
  on: bool,
}

#[test]
fn scalars_structs_and_rules() {
  assert_eq!(ok::<u32>(&[1, 2, 3, 4], 0), Ok(()));
  assert_eq!(ok::<bool>(&[0, 1, 2], 1), Ok(()));
  assert_eq!(ok::<bool>(&[0, 1, 2], 2), Err(Stop::Invalid(2)));
  assert_eq!(ok::<char>(&0xd800u32.to_ne_bytes(), 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<NonZero<i16>>(&[0, 0, 0xff, 0xff], 2), Ok(()));
  assert_eq!(ok::<NonZero<i16>>(&[0, 0, 0xff, 0xff], 0), Err(Stop::Invalid(0)));
  // The first thing that is not valid, in the order of the positions; the rule after the fields.
  let bytes = [7, 1, 2, 1, 3, 4, 2];
  assert_eq!(ok::<Span>(&bytes, 1), Ok(()));
  assert_eq!(ok::<Span>(&bytes, 4), Err(Stop::Invalid(6))); // `on` is 2
  assert_eq!(ok::<Span>(&[3, 2, 1], 0), Err(Stop::Invalid(0))); // `lo <= hi`
  assert_eq!(ok::<Span>(&[3, 10, 0], 0), Err(Stop::Invalid(1))); // `valid(..=9)`
  assert_eq!(ok::<[Span; 2]>(&[1, 2, 0, 1, 2, 3], 0), Err(Stop::Invalid(5)));
  // A `u8in`, which guarantees its values.
  assert_eq!(ok::<u8in<1, 3>>(&[0, 4], 1), Err(Stop::Invalid(1)));
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
#[expect(dead_code, reason = "made of bytes")]
enum Shape {
  Dot,
  Line(bool),
  Box(u8, NonZero<u8>),
}

/// An enum without `repr`, with a niche: `None` is 2 in the `bool`.
#[test]
fn enums_and_niches() {
  assert_eq!(ok::<Shape>(&[2, 7, 1], 0), Ok(()));
  assert_eq!(ok::<Shape>(&[1, 2, 9], 0), Err(Stop::Invalid(1))); // the `bool`
  assert_eq!(ok::<Shape>(&[3, 0, 0], 0), Err(Stop::Invalid(0))); // no such tag
  assert_eq!(ok::<Shape>(&[0, 9, 9], 0), Ok(())); // a `Dot`: the rest is padding
  assert_eq!(ok::<Option<bool>>(&[2], 0), Ok(()));
  assert_eq!(ok::<Option<bool>>(&[3], 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<Option<NonZero<u16>>>(&[0, 0], 0), Ok(()));
  assert_eq!(ok::<Result<u8, bool>>(puns::as_bytes(&Ok::<u8, bool>(9)), 0), Ok(()));
  let err = puns::as_bytes(&Err::<bool, bool>(true)).to_vec();
  assert_eq!(ok::<Result<bool, bool>>(&err, 0), Ok(()));
  // The byte of the `bool` of `Err`, made 2: where it is, whatever the layout `rustc` chose.
  let mut bad = err;
  let at = bad.iter().position(|byte| *byte == 1).expect("the `true`");
  bad[at] = 2;
  assert!(ok::<Result<bool, bool>>(&bad, 0).is_err());
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Choice {
  flag: bool,
  small: u8in<0, 3>,
}

/// A union is one of its fields; `MaybeUninit` is anything.
#[test]
fn unions_and_maybe_uninit() {
  assert_eq!(ok::<Choice>(&[3], 0), Ok(()));
  assert_eq!(ok::<Choice>(&[4], 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<MaybeUninit<bool>>(&[9], 0), Ok(()));
  // A `MaybeUninit` of a pointer: no value is required of it, and no pointer handed over.
  assert_eq!(check::<MaybeUninit<One<u8>>>(&[0; 4], 0), (Ok(()), vec![]));
}

// -------------------------------------------------------------------------------------------------
// Values with pointers

#[derive(Pun)]
#[repr(C)]
struct Node {
  id: u8,
  name: Text,
  kids: Seq<Node>,
  next: Opt<Node>,
  flag: bool,
}

#[derive(Pun)]
#[repr(u8)]
#[expect(dead_code, reason = "made of bytes")]
enum Tree {
  Leaf(bool),
  Node(One<Tree>, One<Tree>),
}

/// A pointer whose offset cannot be zero.
#[derive(Pun)]
#[repr(C)]
struct Strict {
  to: One<u8, relative::Nz32le>,
}

/// Each pointer handed over once, in order, with what it is; a null pointer that may be is not;
/// the error of the visitor ends the walk.
#[test]
fn pointers_are_handed_over() {
  use puns::layout::Aim;
  // `id`, then three offsets of 4 bytes, `flag`.
  let mut bytes = vec![5, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1];
  let all =
    vec![(1, Aim::Text, false), (5, Aim::Sequence { align: 4 }, false), (9, Aim::One, true)];
  assert_eq!(check::<Node>(&bytes, 0), (Ok(()), all.clone()));
  // `next` null: not handed over.
  bytes[9] = 0;
  assert_eq!(check::<Node>(&bytes, 0), (Ok(()), all[..2].to_vec()));
  // `flag` is 2: the pointers before it were handed over.
  bytes[13] = 2;
  assert_eq!(check::<Node>(&bytes, 0), (Err(Stop::Invalid(13)), all[..2].to_vec()));
  // Refused by the visitor, before `flag` is looked at.
  let mut record = Record { refuse: Some(5), ..Record::default() };
  assert_eq!(puns::validate_in::<Node, Record>(&bytes, 0, &mut record), Err(Stop::Refused(5)));
  // A pointer of offset zero: one whose offset may be zero is one, handed over (its scheme
  // says what it aims at); one whose offset is a `NonZero` is not valid there.
  let here = [1u8, 0, 0, 0, 0, 1, 0, 0, 0];
  assert_eq!(check::<Tree>(&here, 0), (Ok(()), vec![(1, Aim::One, false), (5, Aim::One, false)]));
  assert_eq!(check::<Strict>(&[0; 4], 0), (Err(Stop::Invalid(0)), vec![]));
  assert_eq!(check::<Strict>(&[4, 0, 0, 0], 0), (Ok(()), vec![(0, Aim::One, false)]));
  // The pointers of the variant that the tag tells, and only those.
  let node = [1u8, 4, 0, 0, 0, 8, 0, 0, 0];
  assert_eq!(check::<Tree>(&node, 0), (Ok(()), vec![(1, Aim::One, false), (5, Aim::One, false)]));
  let leaf = [0u8, 1, 0, 0, 0, 1, 0, 0, 0];
  assert_eq!(check::<Tree>(&leaf, 0), (Ok(()), vec![]));
}

/// The elements one after the other: by blocks without pointers, one by one with.
#[test]
fn elements() {
  let mut record = Record::default();
  let bytes = [1u8, 0, 1, 1, 0, 2, 1];
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 0, 5, &mut record), Ok(()));
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 0, 6, &mut record), Err(Stop::Invalid(5)));
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 3, 5, &mut record), Err(Stop::Invalid(3)));
  assert_eq!(puns::validate_elems_in::<u16, _>(&bytes, 1, 3, &mut record), Ok(()));
  assert_eq!(puns::validate_elems_in::<u16, _>(&bytes, 1, 4, &mut record), Err(Stop::Invalid(1)));
  // Many, by blocks: the first invalid one.
  let mut many = vec![1u8; 1000];
  many[777] = 9;
  assert_eq!(
    puns::validate_elems_in::<bool, _>(&many, 0, 1000, &mut record),
    Err(Stop::Invalid(777))
  );
  assert_eq!(puns::validate_elems_in::<bool, _>(&many, 0, 777, &mut record), Ok(()));
  // With pointers: each, in order.
  let ones = [4u8, 0, 0, 0, 4, 0, 0, 0];
  assert_eq!(puns::validate_elems_in::<One<u8>, _>(&ones, 0, 2, &mut record), Ok(()));
  assert_eq!(record.pointers.iter().map(|p| p.0).collect::<Vec<_>>(), [0, 4]);
  // A count whose bytes overflow, or that is past the end.
  assert_eq!(
    puns::validate_elems_in::<u16, _>(&bytes, 1, usize::MAX, &mut record),
    Err(Stop::Invalid(1))
  );
  assert_eq!(
    puns::validate_elems_in::<u8, _>(&bytes, usize::MAX, 1, &mut record),
    Err(Stop::Invalid(usize::MAX))
  );
  // No bytes, and none needed.
  assert_eq!(puns::validate_elems_in::<(), _>(&[], 0, usize::MAX, &mut record), Ok(()));
}

// -------------------------------------------------------------------------------------------------
// Any input

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
  const fn next(&mut self) -> u64 {
    self.0 ^= self.0 >> 12;
    self.0 ^= self.0 << 25;
    self.0 ^= self.0 >> 27;
    self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
  }
}

/// Random bytes, random positions (past the end too), random counts (some that overflow): no
/// panic and no read out of the bytes (which Miri would report), and what is accepted is what
/// the same check of each element accepts.
#[test]
fn any_input() {
  fn all<T: Pun>(rng: &mut Rng, bytes: &[u8]) {
    let len = bytes.len();
    let at = match rng.next() % 4 {
      0 => usize::MAX - (rng.next() % 8) as usize,
      _ => (rng.next() % (len as u64 + 4)) as usize,
    };
    let count = match rng.next() % 4 {
      0 => usize::MAX / (1 + (rng.next() % 3) as usize),
      _ => (rng.next() % 8) as usize,
    };
    let one = check::<T>(bytes, at).0;
    let mut record = Record::default();
    let many = puns::validate_elems_in::<T, _>(bytes, at, count, &mut record);
    if many.is_ok() && count > 0 {
      assert_eq!(one, Ok(()), "{}: at {at}, {count}", core::any::type_name::<T>());
    }
  }
  let mut rng = Rng(0x1234_5678_9abc_def1);
  let rounds = if cfg!(miri) { 40 } else { 4000 };
  for _ in 0..rounds {
    let len = (rng.next() % 24) as usize;
    let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8 & 0x3).collect();
    all::<bool>(&mut rng, &bytes);
    all::<Span>(&mut rng, &bytes);
    all::<Shape>(&mut rng, &bytes);
    all::<Choice>(&mut rng, &bytes);
    all::<Option<bool>>(&mut rng, &bytes);
    all::<Node>(&mut rng, &bytes);
    all::<Tree>(&mut rng, &bytes);
    all::<[NonZero<u16>; 3]>(&mut rng, &bytes);
    all::<()>(&mut rng, &bytes);
  }
}

/// No value.
#[derive(Pun)]
#[expect(dead_code, reason = "a type without values, made of bytes in no test")]
enum Never {}

/// A struct that holds a type without values: none either.
#[derive(Pun)]
#[repr(C)]
#[expect(dead_code, reason = "a type without values")]
struct Holds {
  id: u8,
  never: Never,
}

/// A type that has no value is never valid, whatever the bytes: it has something to check (its
/// layout is not free of constraints, `is_free` is `leaves == 0`), so `validate_in` does not
/// return early, and what it checks fails.
#[test]
fn types_without_values() {
  use core::convert::Infallible;
  fn never<T: Pun>(bytes: &[u8]) {
    assert!(!const { T::LAYOUT.is_free() }, "{}", core::any::type_name::<T>());
    assert!(!const { T::LAYOUT.is_inhabited() }, "{}", core::any::type_name::<T>());
    for at in 0..=bytes.len() {
      assert!(ok::<T>(bytes, at).is_err(), "{} at {at}", core::any::type_name::<T>());
    }
    let mut record = Record::default();
    for count in 1..3 {
      assert!(puns::validate_elems_in::<T, _>(bytes, 0, count, &mut record).is_err());
    }
  }
  let bytes = [0u8, 1, 2, 0xff];
  never::<Never>(&bytes);
  never::<Infallible>(&bytes);
  never::<Holds>(&bytes);
  never::<[Never; 2]>(&bytes);
  never::<(u8, Infallible)>(&bytes);
  never::<Result<Infallible, Never>>(&bytes);
  // No element of a type without values: one value, of no byte.
  assert_eq!(ok::<[Never; 0]>(&bytes, 4), Ok(()));
  // `Option<Infallible>`: `None` only, whatever its bytes (it has none).
  assert_eq!(size_of::<Option<Infallible>>(), 0);
  assert_eq!(ok::<Option<Infallible>>(&bytes, 0), Ok(()));
  // `Result<u8, Infallible>`: an `Ok` only.
  assert_eq!(ok::<Result<u8, Infallible>>(&[7], 0), Ok(()));
}
