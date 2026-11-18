//! Regions of the crate [`nearest`](https://docs.rs/nearest), described and checked by `puns`
//! (feature `nearest`).
//!
//! A region keeps a value and everything it points to in one buffer; a pointer there is a
//! `Near<T, O>`, the distance from its own address to its target, an integer `O`. This module
//! makes such a value a [`Pun`] one, pointers included:
//!
//! - **Offsets of a given byte order.** [`i16le`](num::i16le), [`i32le`](num::i32le),
//!   [`i64le`](num::i64le) and their big-endian counterparts are `nearest::Offset`s: with one as
//!   the `O` of every pointer, and numbers of a given byte order ([`num`]) as fields, the bytes of
//!   a region are the same on every machine. They are byte arrays, so a `Near<T, i32le>` has no
//!   alignment requirement either.
//! - **`Pun` for the pointers**: `Near<T, O>`, `Near<[T], O>`, `Near<str, O>` and
//!   `Option<Near<T, O>>`, for every offset whose byte order is known ([`Order`]). With
//!   `#[derive(Pun)]` after `#[flat]`, a type of a region is declared once, for both crates.
//! - **Loaders**: [`view`] and [`from_bytes`] (and [`view_with`], [`from_bytes_with`], which do
//!   not allocate to check) check that bytes are the image of a value, as `nearest::view` and
//!   `Region::from_bytes` do, from the layouts: what a layout says cannot be invalid is not
//!   compiled, and what `#[derive(Pun)]` is told of a type (`#[pun(valid(..))]`, `assert(..)`) is
//!   checked as well.
//! - **[`view_ref`](crate::view_ref)**: a value of a region read as another type, proven at
//!   compile time, with everything it points to.
//!
//! ```
//! use nearest::{Near, Region, flat};
//! use puns::{
//!   Pun,
//!   num::{i32le, u32le},
//! };
//!
//! /// A table of names: its bytes are the same on every machine.
//! #[flat]
//! #[derive(Pun, Debug, PartialEq)]
//! struct Table {
//!   version: u32le,
//!   names: Near<[Name], i32le>,
//! }
//!
//! #[flat]
//! #[derive(Pun, Debug, PartialEq)]
//! struct Name {
//!   public: bool,
//!   text: Near<str, i32le>,
//! }
//!
//! let table = Region::new(Table::new(u32le::new(1), [Name::new(true, "main")]));
//! let bytes = table.as_bytes().unwrap();
//! assert_eq!(bytes[..8], [1, 0, 0, 0, 8, 0, 0, 0]); // the version, the distance to the names
//!
//! // Read in place, from bytes that may come from anywhere: only checked.
//! let loaded: &Table = puns::nearest::view(bytes)?;
//! assert_eq!((loaded.names.len(), &*loaded.names[0].text), (1, "main"));
//!
//! // Bytes that are not a table are refused, where they stop being one: here a `bool` of 2.
//! let mut bad = Region::<Table>::from_bytes(bytes)?.as_bytes().unwrap().to_vec();
//! bad[16] = 2;
//! assert_eq!(puns::nearest::from_bytes::<Table>(&bad).err(), Some(nearest::Error::Invalid(16)));
//! # Ok::<(), nearest::Error>(())
//! ```
//!
//! # What is, and is not, done with a pointer
//!
//! What a pointer guarantees is not a property of its bytes: its target is where it says, valid,
//! and its own. So nothing in this crate makes a value that holds a pointer from bytes, or from
//! another value, but the loaders of this module, which check the whole buffer, and
//! [`view_ref`](crate::view_ref), which needs a pointer for a pointer; and nothing gives a `&mut`
//! to a value that holds one. A value that holds pointers can be *read* as plain data
//! ([`pun_ref`](crate::pun_ref), [`as_bytes`](crate::as_bytes)...): an offset is initialized
//! bytes.
//!
//! This module makes no `Near`, writes to no region, and builds none but with the loaders of
//! `nearest`, from bytes it checked.

use alloc::vec::Vec;
use core::num::NonZero;

pub use ::nearest::Error;
use ::nearest::{
  __private::{Buf, Check, Sink, fits},
  Flat, Near, Offset, Region,
};

use crate::{
  Fields, Pun, Variants,
  layout::{Aim, Endian, Kind, Layout, Pointer, Shape, Spec},
  num,
  validate::check_rules,
};

// -------------------------------------------------------------------------------------------------
// Numbers of a given byte order, in a region

macro_rules! flat {
  ($($name:ident[$size:literal])+) => {$(
    // SAFETY: a byte array, as its bytes are: no pointer, no padding, every bit pattern valid
    // (`PLAIN`, `ANY`). `store`, `edit` and `check` are the ones of the array.
    unsafe impl Flat for num::$name {
      type Edit<'e> = Self;
      const PLAIN: bool = true;
      const ANY: bool = true;

      unsafe fn store(&self, sink: &mut Sink, at: usize, shift: Option<isize>) {
        // SAFETY: forwarded: `at` is allocated for `Self`, which has the size of its bytes and
        // their alignment (1).
        unsafe { self.to_bytes().store(sink, at, shift) }
      }

      unsafe fn edit<'a, 'e: 'a>(buf: *mut Buf, at: usize) -> &'a mut Self {
        // SAFETY: forwarded: a `Self` at `at` is its bytes.
        let bytes: &'a mut [u8; $size] = unsafe { <[u8; $size] as Flat>::edit(buf, at) };
        // Proven at compile time: every array of that size is a `Self`, and back.
        crate::pun_mut(bytes)
      }

      fn check(ck: &mut Check<'_>, at: usize) -> Result<(), Error> {
        <[u8; $size]>::check(ck, at)
      }
    }
  )+};
}

flat! {
  u16le[2] u16be[2] u24le[3] u24be[3] u32le[4] u32be[4] u48le[6] u48be[6] u64le[8] u64be[8]
  u128le[16] u128be[16] i16le[2] i16be[2] i32le[4] i32be[4] i64le[8] i64be[8] i128le[16]
  i128be[16] f32le[4] f32be[4] f64le[8] f64be[8]
}

macro_rules! flat_checked {
  ($([$($generics:tt)*] $ty:ty: $size:literal;)+) => {$(
    // SAFETY: bytes of which only some are a value, without pointer nor padding (`PLAIN`, not
    // `ANY`). `store` and `edit` are the ones of the bytes, and `check` accepts the bytes that
    // are a value: the very check of `try_pun`.
    unsafe impl<$($generics)*> Flat for $ty {
      type Edit<'e> = Self;
      const PLAIN: bool = true;

      unsafe fn store(&self, sink: &mut Sink, at: usize, shift: Option<isize>) {
        // Proven at compile time: every `Self` is that many initialized bytes.
        let bytes: [u8; $size] = crate::pun(*self);
        // SAFETY: forwarded: `at` is allocated for `Self`, which has the size of its bytes and
        // their alignment (1).
        unsafe { bytes.store(sink, at, shift) }
      }

      unsafe fn edit<'a, 'e: 'a>(buf: *mut Buf, at: usize) -> &'a mut Self {
        // SAFETY: forwarded: a `Self` at `at` is its bytes.
        let bytes: &'a mut [u8; $size] = unsafe { <[u8; $size] as Flat>::edit(buf, at) };
        // SAFETY: the caller has a valid `Self` there, which these bytes are (the same size,
        // the same alignment); what is written through the result is a `Self`, so they stay
        // one.
        unsafe { &mut *core::ptr::from_mut(bytes).cast::<Self>() }
      }

      fn check(ck: &mut Check<'_>, at: usize) -> Result<(), Error> {
        let mut bytes = [0; $size];
        for (i, byte) in bytes.iter_mut().enumerate() {
          // Any byte: the "tag" of an enum of 256 variants.
          *byte = ck.tag(at + i, 256)?;
        }
        match crate::try_pun::<[u8; $size], Self>(bytes) {
          Ok(_) => Ok(()),
          Err(_) => Err(Error::Invalid(at)),
        }
      }
    }
  )+};
}

flat_checked! {
  [const V: u8] num::u8c<V>: 1;
  [const MIN: u8, const MAX: u8] num::u8in<MIN, MAX>: 1;
  [const V: u16] num::u16lec<V>: 2;
  [const V: u16] num::u16bec<V>: 2;
  [const V: u32] num::u24lec<V>: 3;
  [const V: u32] num::u24bec<V>: 3;
  [const V: u32] num::u32lec<V>: 4;
  [const V: u32] num::u32bec<V>: 4;
  [const V: u64] num::u48lec<V>: 6;
  [const V: u64] num::u48bec<V>: 6;
  [const V: u64] num::u64lec<V>: 8;
  [const V: u64] num::u64bec<V>: 8;
  [const V: u128] num::u128lec<V>: 16;
  [const V: u128] num::u128bec<V>: 16;
}

// -------------------------------------------------------------------------------------------------
// Offsets of a given byte order

/// An offset whose bytes are a signed integer in two's complement of a known byte order: what
/// the layout of a pointer tells of its offset, and how the loaders of this module read it.
///
/// Implemented for `i16`, `i32`, `i64` (the byte order of the target) and for the signed
/// integers of a given byte order of [`num`], which are offsets that this module makes.
///
/// # Safety
/// For every value, `to_isize` must be the integer that the bytes of `Self` are in two's
/// complement, in the byte order [`ENDIAN`](Order::ENDIAN), converted to `isize` as `as` does.
pub unsafe trait Order: Offset {
  /// The byte order of the offset.
  const ENDIAN: Endian;
}

macro_rules! native {
  ($($name:ident)+) => {$(
    // SAFETY: `to_isize` is `self as isize`.
    unsafe impl Order for $name {
      const ENDIAN: Endian = Endian::NATIVE;
    }
  )+};
}

native!(i16 i32 i64);

macro_rules! offsets {
  ($($name:ident $non_zero:ident: $raw:ident $endian:ident;)+) => {$(
    #[doc = concat!("A [`", stringify!($name), "`](num::", stringify!($name), ") that is not ")]
    /// zero: what a pointer whose offset it is, is made of. It has no use of its own.
    // Packed: the alignment of the byte array that the offset is, with the niche of `NonZero`.
    #[derive(Copy, Clone)]
    #[repr(C, packed)]
    pub struct $non_zero(NonZero<$raw>);

    impl From<$non_zero> for num::$name {
      #[inline]
      fn from(value: $non_zero) -> Self {
        // The bytes as they are in memory: the integer is only what `to_isize` makes of them.
        Self::from_bytes({ value.0 }.get().to_ne_bytes())
      }
    }

    // What `nearest` checks where a region uses the offset, checked here as well.
    const _: () = assert!(
      size_of::<$non_zero>() == size_of::<num::$name>()
        && align_of::<$non_zero>() == align_of::<num::$name>()
        && size_of::<Option<$non_zero>>() == size_of::<num::$name>()
    );

    // SAFETY:
    // - a byte array: every bit pattern is a value, and it has no padding;
    // - `new` writes the bytes of the integer in the byte order of the type and `get` reads
    //   them back: a round trip for every integer of its size, which those within `LIMIT` are;
    // - the bytes of 0 are zeros in either byte order, and of no other integer;
    // - the `NonZero` is a `NonZero` of the size of the offset, packed: its size, the alignment
    //   of the offset (1), valid for every bit pattern but zeros. `Option` of it has its size
    //   (asserted above), so `None` is the only bit pattern left, zeros. `From` keeps the bytes;
    // - `from_isize` and `to_isize` are conversions between integers.
    unsafe impl Offset for num::$name {
      type NonZero = $non_zero;

      #[inline]
      fn from_isize(n: isize) -> Self {
        Self::new(n as $raw)
      }

      #[inline]
      fn to_isize(self) -> isize {
        self.get() as isize
      }
    }

    // SAFETY: `get` reads the bytes in that byte order, and `to_isize` is `get() as isize`.
    unsafe impl Order for num::$name {
      const ENDIAN: Endian = Endian::$endian;
    }
  )+};
}

offsets! {
  i16le NonZeroI16le: i16 Little;
  i16be NonZeroI16be: i16 Big;
  i32le NonZeroI32le: i32 Little;
  i32be NonZeroI32be: i32 Big;
  i64le NonZeroI64le: i64 Little;
  i64be NonZeroI64be: i64 Big;
}

// -------------------------------------------------------------------------------------------------
// The pointers

/// The layout of the pointer `P`, whose offset is an `O`, to a `T` (a list of `T`, or text):
/// what `nearest` requires of the three, at compile time, as every function of `nearest` that
/// uses such a pointer does (`fits_behind`, `offset`).
const fn pointer<P, T: Flat, O: Order>(aim: Aim, nullable: bool) -> Layout {
  // Alignment at most 8, a size, no drop glue: what a region holds.
  fits::<T>();
  assert!(
    T::REACH >= O::LIMIT,
    "a `Near` cannot lead to a type holding a narrower `Near`: the pointers of the root of a \
     region bound it, so give the pointers below an offset that is at least as wide"
  );
  let width = size_of::<O>();
  assert!(
    size_of::<O::NonZero>() == width
      && align_of::<O::NonZero>() == align_of::<O>()
      && size_of::<Option<O::NonZero>>() == width,
    "the `NonZero` of an `Offset` has its size and its alignment, and so has `Option` of it"
  );
  assert!(
    O::LIMIT.is_multiple_of(8) && O::LIMIT > 0,
    "the `LIMIT` of an `Offset` is a multiple of 8"
  );
  Layout::pointer::<P>(Pointer::new::<T>(aim, nullable, width, O::ENDIAN, O::LIMIT))
}

// SAFETY: a `Near<T, O>` is the `NonZero` of an `O`, the distance from its address to a `T` of
// the same region (`nearest`, Layout). Only a pointer proves one, whose target is proven to be
// what this one aims at (`analysis`, `deep`), or a check of the whole region (`view`); and no
// `&mut` to one is ever given (`mutable`, `mutable_part`).
unsafe impl<T: Flat + Pun, O: Order> Pun for Near<T, O> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  type Pointee = T;
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = pointer::<Self, T, O>(Aim::One, false);
}

// SAFETY: as for `Near<T, O>`; the target is a list (`nearest`, Layout), what `Aim::List` is.
unsafe impl<T: Flat + Pun, O: Order> Pun for Near<[T], O> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  type Pointee = T;
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = pointer::<Self, T, O>(Aim::List, false);
}

// SAFETY: as for `Near<T, O>`; the target is a length and UTF-8 (`nearest`, Layout), what
// `Aim::Str` is.
unsafe impl<O: Order> Pun for Near<str, O> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  // Text is read as text: there is nothing to relate behind such a pointer.
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = pointer::<Self, u8, O>(Aim::Str, false);
}

// SAFETY: an `O`: `None` is zero, `Some` is the `Near` (`nearest`, Stored types).
unsafe impl<T: Flat + Pun, O: Order> Pun for Option<Near<T, O>> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  type Pointee = T;
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = pointer::<Self, T, O>(Aim::One, true);
}

// -------------------------------------------------------------------------------------------------
// Loading

/// The value whose image is `bytes`, read in place (a mapped file, the buffer of a region): what
/// `nearest::view` does, checked from the layout of `T`. The slice must be 8-aligned.
///
/// The bytes are the *compact image* of a `T`: the value at 0, then everything it points to in
/// the order a deep copy writes it, each at the next position aligned for it, and nothing else.
/// Every pointer must aim at the very position where the copy puts its target, which gives at
/// once bounds, alignment, a tree (no target shared, no cycle) and no gap; values are checked on
/// the way (`bool`, `char`, enums, UTF-8, lengths), and so is what `#[derive(Pun)]` is told of
/// `T` and of the types it is made of: `#[pun(valid(..))]`, `#[pun(assert(..))]`.
///
/// Without such requirements, it accepts the bytes that `nearest::view` accepts, and refuses the
/// others with the same error. What cannot be invalid is not walked, nor compiled: a list of
/// numbers costs nothing to check, however long.
///
/// ```
/// use nearest::{Near, Region, flat};
/// use puns::Pun;
///
/// #[flat]
/// #[derive(Pun)]
/// struct Msg { id: u8, body: Near<str> }
///
/// let region = Region::new(Msg::new(1, "hi"));
/// let bytes = region.as_bytes().unwrap(); // the buffer of a region is 8-aligned
/// let msg: &Msg = puns::nearest::view(bytes)?;
/// assert_eq!((msg.id, &*msg.body), (1, "hi"));
/// # Ok::<(), nearest::Error>(())
/// ```
///
/// # Errors
/// [`Error::Invalid`] where the bytes stop being the compact image of a valid `T` (at 0 if the
/// slice is not 8-aligned, shorter than a `T`, or longer than its pointers reach).
pub fn view<T: Flat + Pun>(bytes: &[u8]) -> Result<&T, Error> {
  read(bytes, Stack::Heap(Vec::new()))
}

/// [`view`], with `scratch` for the memory that the check needs: nothing is allocated.
///
/// `bytes.len() / 4` entries are always enough (`bytes.len() / 2` with pointers of 2 bytes):
/// an entry is a pointer, or the rest of a list, that waits to be checked. Most data needs about
/// as many as the pointers that wait along its deepest path.
///
/// ```
/// use nearest::{Error, Near, Region, flat};
/// use puns::{Pun, nearest::Pending};
///
/// #[flat]
/// #[derive(Pun)]
/// struct Msg { id: u8, body: Near<str> }
///
/// let region = Region::new(Msg::new(1, "hi"));
/// let bytes = region.as_bytes().unwrap();
/// let mut scratch = [Pending::default(); 4];
/// assert_eq!(&*puns::nearest::view_with::<Msg>(bytes, &mut scratch)?.body, "hi");
/// // Too little scratch says nothing of the bytes: the check could not finish.
/// assert_eq!(puns::nearest::view_with::<Msg>(bytes, &mut []).err(), Some(Error::ScratchFull));
/// # Ok::<(), nearest::Error>(())
/// ```
///
/// # Errors
/// As [`view`], and [`Error::ScratchFull`] if `scratch` ran out before the check finished.
pub fn view_with<'b, T: Flat + Pun>(
  bytes: &'b [u8],
  scratch: &mut [Pending],
) -> Result<&'b T, Error> {
  read(bytes, Stack::Scratch(scratch, 0))
}

/// A region that holds a copy of `bytes`, if they are the image of a `T`: what
/// `Region::from_bytes` does, checked from the layout of `T` (see [`view`]). The bytes may be at
/// any address.
///
/// ```
/// use nearest::{Near, Region, flat};
/// use puns::Pun;
///
/// #[flat]
/// #[derive(Pun, Debug, PartialEq)]
/// struct Msg { id: u8, body: Near<str> }
///
/// let region = Region::new(Msg::new(1, "hi"));
/// let saved = region.as_bytes().unwrap().to_vec();
/// assert_eq!(puns::nearest::from_bytes::<Msg>(&saved)?, region);
/// # Ok::<(), nearest::Error>(())
/// ```
///
/// # Errors
/// [`Error::Invalid`] where the bytes stop being the compact image of a valid `T`.
pub fn from_bytes<T: Flat + Pun>(bytes: &[u8]) -> Result<Region<T>, Error> {
  load(bytes, Stack::Heap(Vec::new()))
}

/// [`from_bytes`], with `scratch` for the memory that the check needs (see [`view_with`]): only
/// the region is allocated.
///
/// # Errors
/// As [`from_bytes`], and [`Error::ScratchFull`] if `scratch` ran out before the check finished.
pub fn from_bytes_with<T: Flat + Pun>(
  bytes: &[u8],
  scratch: &mut [Pending],
) -> Result<Region<T>, Error> {
  load(bytes, Stack::Scratch(scratch, 0))
}

/// [`view`], checking with `todo`.
fn read<'b, T: Flat + Pun>(bytes: &'b [u8], todo: Stack<'_>) -> Result<&'b T, Error> {
  if !bytes.as_ptr().addr().is_multiple_of(8) {
    return Err(Error::Invalid(0));
  }
  check::<T>(bytes, todo)?;
  // SAFETY: the bytes are 8-aligned and, as just checked, the compact image of a valid `T`,
  // whose pointers all aim inside them at valid targets of their own: what `nearest::view`
  // checks before it makes the same cast, with this function.
  Ok(unsafe { ::nearest::view_unchecked(bytes) })
}

/// [`from_bytes`], checking with `todo`.
fn load<T: Flat + Pun>(bytes: &[u8], todo: Stack<'_>) -> Result<Region<T>, Error> {
  check::<T>(bytes, todo)?;
  // SAFETY: as just checked, the compact image of a valid `T`: the bytes that `as_bytes` gives
  // for the region of that value, padding aside, which `Region::from_bytes` copies with this
  // function after the same check. The storage is the heap, which holds any image.
  Ok(unsafe { Region::from_bytes_unchecked(bytes) })
}

/// What `nearest` says of `T` and what its layout says, compared at compile time: a type that
/// `nearest` copies as bytes (`PLAIN`) or does not check (`ANY`) must be one, and the largest
/// region of a `T` is the same for both.
const fn agrees<T: Flat + Pun>() {
  const {
    let layout = T::LAYOUT;
    assert!(
      !T::PLAIN || layout.is_plain(),
      "the implementation of `nearest::Flat` for this type says `PLAIN` (no padding, no pointer), \
       which its layout denies: one of the two is wrong"
    );
    assert!(
      !T::ANY || layout.is_free(),
      "the implementation of `nearest::Flat` for this type says `ANY` (every bit pattern is a \
       value), which its layout denies: one of the two is wrong"
    );
    assert!(
      T::REACH == layout.reach(),
      "the implementation of `nearest::Flat` for this type says `REACH` (what its pointers \
       reach), which its layout denies: one of the two is wrong"
    );
  };
}

/// The largest region with a `T` at its root: what its pointers reach, and at most `isize::MAX`
/// bytes, both rounded down to 8 bytes (`nearest`, Layout).
const fn limit<T: Flat>() -> usize {
  let most = isize::MAX as u64 & !7;
  (if T::REACH < most { T::REACH } else { most }) as usize
}

/// What checks a pointer, given its position, or the rest of a list, given the position of its
/// next element and how many are left.
type Step = fn(&mut Image<'_>, usize, usize) -> Result<(), Error>;

/// An entry of the memory that a check needs: a pointer, or the rest of a list, that waits to be
/// checked. [`view_with`] and [`from_bytes_with`] take a slice of them.
#[derive(Clone, Copy, Debug)]
pub struct Pending(usize, usize, Step);

impl Default for Pending {
  fn default() -> Self {
    Self(0, 0, |_, _, _| Ok(()))
  }
}

/// What is still to check, the last one next: the scratch of the caller and how much of it is
/// used, or a vector.
enum Stack<'s> {
  Scratch(&'s mut [Pending], usize),
  Heap(Vec<Pending>),
}

impl Stack<'_> {
  const fn len(&self) -> usize {
    match self {
      Self::Scratch(_, len) => *len,
      Self::Heap(items) => items.len(),
    }
  }

  /// Whatever there was room.
  fn push(&mut self, item: Pending) -> bool {
    match self {
      Self::Scratch(items, len) => {
        let Some(slot) = items.get_mut(*len) else { return false };
        *slot = item;
        *len += 1;
      }
      Self::Heap(items) => items.push(item),
    }
    true
  }

  fn pop(&mut self) -> Option<Pending> {
    match self {
      Self::Scratch(items, len) => {
        *len = len.checked_sub(1)?;
        items.get(*len).copied()
      }
      Self::Heap(items) => items.pop(),
    }
  }

  /// The entries from `mark` on.
  fn top(&mut self, mark: usize) -> &mut [Pending] {
    match self {
      Self::Scratch(items, len) => items.get_mut(mark..*len),
      Self::Heap(items) => items.get_mut(mark..),
    }
    .unwrap_or_default()
  }
}

/// A walk over bytes that must be the compact image of a value: the root, then every target in
/// the order a deep copy writes it (`nearest`, Bytes). It mirrors the walk of `nearest`
/// (`bytes.rs`), so that both stop at the same place: safe code that does not panic, but for
/// the reads of scalars, which are in bounds.
struct Image<'b> {
  bytes: &'b [u8],
  /// Where the next target must start, before alignment.
  next: usize,
  /// Pointers and rests of lists still to check; the last one is next.
  todo: Stack<'b>,
}

impl<'b> Image<'b> {
  /// The `size` bytes at `at`.
  fn get(&self, at: usize, size: usize) -> Result<&'b [u8], Error> {
    self.bytes.get(at..).and_then(|rest| rest.get(..size)).ok_or(Error::Invalid(at))
  }

  /// Whatever the scalar at `at` is one that `spec` accepts.
  fn scalar(&self, spec: &Spec, at: usize) -> Result<(), Error> {
    let bytes = self.get(at, spec.size)?;
    // SAFETY: `spec.size` initialized bytes.
    if unsafe { spec.matches(bytes.as_ptr()) } { Ok(()) } else { Err(Error::Invalid(at)) }
  }

  /// The integer at `at` that is an offset (or a length, or a capacity) of `pointer`, as its
  /// `to_isize` reads it.
  #[expect(clippy::cast_possible_truncation, reason = "as `to_isize` does")]
  fn offset(&self, pointer: &Pointer, at: usize) -> Result<isize, Error> {
    let bytes = self.get(at, pointer.width)?;
    // SAFETY: `pointer.width` initialized bytes.
    Ok(unsafe { pointer.offset(bytes.as_ptr()) } as isize)
  }

  /// The length at `at`; a negative one is as out of bounds as any too large.
  fn len(&self, pointer: &Pointer, at: usize) -> Result<usize, Error> {
    Ok(usize::try_from(self.offset(pointer, at)?).unwrap_or(usize::MAX))
  }

  /// Queues a pointer, or the rest of a list. If the scratch is full, the walk ends.
  fn push(&mut self, at: usize, count: usize, step: Step) -> Result<(), Error> {
    if self.todo.push(Pending(at, count, step)) { Ok(()) } else { Err(Error::ScratchFull) }
  }

  /// The next block, of `size` bytes whose byte `inset` is aligned to `align`: the `pointer` at
  /// `ptr` must aim `lead` bytes into it, which is where a deep copy would have put it. This one
  /// rule gives bounds, alignment, no sharing, no cycle and no gap. Returns its position.
  fn claim(
    &mut self,
    pointer: &Pointer,
    ptr: usize,
    lead: usize,
    inset: usize,
    align: usize,
    size: usize,
  ) -> Result<usize, Error> {
    let off = self.offset(pointer, ptr)?;
    let at = (self.next.checked_add(inset).and_then(|n| n.checked_next_multiple_of(align)))
      .ok_or(Error::Invalid(ptr))?
      - inset;
    let aimed = at.checked_add(lead).is_some_and(|to| ptr.checked_add_signed(off) == Some(to));
    let end = (at.checked_add(size).filter(|&end| aimed && end <= self.bytes.len()))
      .ok_or(Error::Invalid(ptr))?;
    self.next = end;
    Ok(at)
  }

  /// Takes `size` more bytes after the last block.
  fn extend(&mut self, at: usize, size: Option<usize>) -> Result<(), Error> {
    let end = size.and_then(|s| self.next.checked_add(s)).filter(|&end| end <= self.bytes.len());
    self.next = end.ok_or(Error::Invalid(at))?;
    Ok(())
  }
}

/// Whatever `bytes` are the compact image of a `T`, without recursion, keeping what is still to
/// check in `todo`.
fn check<T: Flat + Pun>(bytes: &[u8], todo: Stack<'_>) -> Result<(), Error> {
  agrees::<T>();
  let size = const { T::LAYOUT.size() };
  if bytes.len() < size || bytes.len() > const { limit::<T>() } {
    return Err(Error::Invalid(0));
  }
  let mut image = Image { bytes, next: size, todo };
  value::<T>(&mut image, 0)?;
  let mut mark = 0;
  loop {
    // What the last step queued is checked first-queued first.
    image.todo.top(mark).reverse();
    let Some(Pending(at, count, step)) = image.todo.pop() else { break };
    mark = image.todo.len();
    step(&mut image, at, count)?;
  }
  if image.next == bytes.len() { Ok(()) } else { Err(Error::Invalid(image.next)) }
}

/// Checks the `T` at `at`, whose own bytes are in bounds, and queues its pointers.
///
/// It recurses over the type, as `validate` does: each call is an instance in which the layout is
/// a constant, and what the layout says cannot be invalid is not compiled.
fn value<T: ?Sized + Pun>(image: &mut Image<'_>, at: usize) -> Result<(), Error> {
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  match const { T::LAYOUT.shape() } {
    Shape::Scalar(spec) => image.scalar(&spec, at),
    Shape::Array(len) => {
      let size = const { <T::Elem as Pun>::LAYOUT.size() };
      (0..len).try_for_each(|i| value::<T::Elem>(image, at + i * size))
    }
    Shape::Struct(rules) => {
      fields::<T::Fields>(image, at)?;
      if !rules.is_empty() {
        let bytes = image.get(at, const { T::LAYOUT.size() })?;
        // SAFETY: the bytes of the struct, which is sized (a region holds no other), all
        // initialized, its fields valid. A rule only reads them, at any alignment.
        if !unsafe { check_rules::<T>(crate::ptr::from_raw_parts::<T>(bytes.as_ptr(), 0), rules) } {
          return Err(Error::Invalid(at));
        }
      }
      Ok(())
    }
    Shape::Enum(layout) => {
      let Kind::Scalar(spec) = layout.tag.layout.kind else { return Err(Error::Invalid(at)) };
      let place = at + layout.tag.offset;
      let bytes = image.get(place, spec.size)?;
      // SAFETY: `spec.size` initialized bytes.
      let tag = unsafe { spec.load(bytes.as_ptr()) };
      variants::<T::Variants>(image, at, tag).unwrap_or(Err(Error::Invalid(place)))
    }
    Shape::Pointer(pointer) => {
      if pointer.nullable && image.offset(&pointer, at)? == 0 {
        return Ok(());
      }
      let step: Step = match pointer.aim {
        Aim::One => one::<T>,
        Aim::List => list::<T>,
        Aim::Str => text::<T>,
      };
      image.push(at, 0, step)
    }
    // A region holds sized values.
    Shape::Slice | Shape::Pattern(_) => Err(Error::Invalid(at)),
  }
}

/// The field `F` of the struct (or of the variant) at `at`, and the ones after it, in the order
/// of their declarations.
fn fields<F: Fields>(image: &mut Image<'_>, at: usize) -> Result<(), Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  let place = at + field.offset();
  match field.restriction() {
    Some(spec) => image.scalar(&spec, place)?,
    None => value::<F::Type>(image, place)?,
  }
  fields::<F::Next>(image, at)
}

/// The fields of the variant, among `V` and the ones after it, whose tag is `tag`: `None` if
/// there is none.
fn variants<V: Variants>(image: &mut Image<'_>, at: usize, tag: u128) -> Option<Result<(), Error>> {
  match const { V::TAG } {
    None => None,
    Some(expected) if expected == tag => Some(fields::<V::Fields>(image, at)),
    Some(_) => variants::<V::Next>(image, at, tag),
  }
}

/// The pointer that the pointer type `P` is, and the layout of what it aims at.
///
/// `None` if `P` is not a pointer: `value` names the steps below for every type, and the
/// compiler makes every instance that is named, also where a constant rules it out. Such a step
/// is never run.
const fn pointer_of<P: ?Sized + Pun>() -> Option<(Pointer, Layout)> {
  match P::LAYOUT.kind {
    Kind::Pointer(pointer) => Some((pointer, <P::Pointee as Pun>::LAYOUT)),
    _ => None,
  }
}

/// The target of the pointer `P` at `ptr`: one value.
fn one<P: ?Sized + Pun>(image: &mut Image<'_>, ptr: usize, _: usize) -> Result<(), Error> {
  let Some((pointer, layout)) = (const { pointer_of::<P>() }) else {
    return Err(Error::Invalid(ptr));
  };
  let at = image.claim(&pointer, ptr, 0, 0, layout.align(), layout.size())?;
  value::<P::Pointee>(image, at)
}

/// The list of the pointer `P` at `ptr`: a capacity and a length, integers like the offset, then
/// the elements, aligned for them and for the offset.
fn list<P: ?Sized + Pun>(image: &mut Image<'_>, ptr: usize, _: usize) -> Result<(), Error> {
  let Some((pointer, layout)) = (const { pointer_of::<P>() }) else {
    return Err(Error::Invalid(ptr));
  };
  let w = pointer.width;
  let at = image.claim(&pointer, ptr, w, 2 * w, layout.align().max(w), 2 * w)?;
  let (cap, len) = (image.len(&pointer, at)?, image.len(&pointer, at + w)?);
  image.extend(at, len.checked_mul(layout.size()).filter(|_| cap == len))?;
  items::<P::Pointee>(image, at + 2 * w, len)
}

/// The text of the pointer `P` at `ptr`: a length, an integer like the offset, then UTF-8.
fn text<P: ?Sized + Pun>(image: &mut Image<'_>, ptr: usize, _: usize) -> Result<(), Error> {
  let Some((pointer, _)) = (const { pointer_of::<P>() }) else { return Err(Error::Invalid(ptr)) };
  let w = pointer.width;
  let at = image.claim(&pointer, ptr, 0, 0, w, w)?;
  let len = image.len(&pointer, at)?;
  image.extend(at, Some(len))?;
  match image.bytes.get(at + w..image.next).map(core::str::from_utf8) {
    Some(Ok(_)) => Ok(()),
    _ => Err(Error::Invalid(at)),
  }
}

/// The `count` elements from `first`, in order. When one has pointers, they are checked before
/// the remaining elements, which wait on the stack behind them. Elements that cannot be invalid
/// are not walked.
fn items<T: Pun>(image: &mut Image<'_>, first: usize, count: usize) -> Result<(), Error> {
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  let size = const { T::LAYOUT.size() };
  for i in 0..count {
    let mark = image.todo.len();
    value::<T>(image, first + i * size)?;
    if image.todo.len() > mark {
      if i + 1 < count {
        image.push(first + (i + 1) * size, count - i - 1, items::<T>)?;
      }
      break;
    }
  }
  Ok(())
}
