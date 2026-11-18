//! Self-relative pointers of the tests, and a way to build values that hold them.
//!
//! They are declared through the public API of `puns` only, as a crate of its own would
//! ([`puns::SelfRelative`], [`puns::self_relative!`]).
//!
//! The scheme, [`SCHEME`]: a pointer is an offset `O`, a signed integer of a given byte order
//! ([`Offset`]: the numbers of `puns::num`, byte arrays of no alignment), from the address of the
//! pointer to
//!
//! - [`One<T, O>`]: one `T`, at an address aligned for it; [`Opt<T, O>`] is the same, or none
//!   (zero);
//! - [`Seq<T, O>`]: the first of a sequence of `T`, at an address that is a multiple of the
//!   larger of the alignment of `T` and the width of `O`, whose length, an `O`, is right before
//!   it;
//! - [`Text<O>`]: the first byte of UTF-8, whose length, an `O`, is right before it.
//!
//! A pointer is followed through the [`Region`] that holds it ([`Region::get`], [`Region::opt`],
//! [`Region::seq`], [`Region::text`]), with the provenance of the whole buffer, never from its
//! own address: so what is read through a view is sound under stacked borrows as well as tree
//! borrows. A pointer that is not in the buffer of the region is refused (a panic).
//!
//! What `unsafe` code here assumes of the target is asserted where it is used: the alignment of
//! what is put in a buffer (at most [`ALIGN`]); an offset is converted to and from `isize` by
//! checked conversions (an `i64le` on a 32-bit target), and its byte order is its own.

use core::{fmt, marker::PhantomData, mem::MaybeUninit, ptr};

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
  num::{i16be, i16le, i32be, i32le, i64be, i64le},
};

/// The name of the scheme of these pointers.
pub const SCHEME: &str = "puns tests: an offset to the target, a length before a sequence";

/// The alignment of a buffer: the largest of what is put in one.
pub const ALIGN: usize = 16;

/// An offset: a signed integer of a given byte order.
pub trait Offset: Pun + Copy + 'static {
  /// Its byte order.
  const ENDIAN: Endian;

  /// The integer that `n` is.
  ///
  /// # Panics
  /// If it does not hold `n`.
  fn new(n: isize) -> Self;

  /// The integer of its bytes.
  ///
  /// # Panics
  /// If an `isize` does not hold it.
  fn get(self) -> isize;
}

macro_rules! offsets {
  ($($name:ident: $raw:ident $endian:ident;)+) => {$(
    impl Offset for $name {
      const ENDIAN: Endian = Endian::$endian;

      fn new(n: isize) -> Self {
        Self::new($raw::try_from(n).expect("an offset that holds the distance"))
      }

      fn get(self) -> isize {
        isize::try_from(self.get()).expect("an offset that an `isize` holds")
      }
    }
  )+};
}

offsets! {
  i16le: i16 Little;
  i16be: i16 Big;
  i32le: i32 Little;
  i32be: i32 Big;
  i64le: i64 Little;
  i64be: i64 Big;
}

macro_rules! pointer {
  ($(#[$doc:meta])* $name:ident [$($param:ident)?] $target:ty, $pointer:expr) => {
    $(#[$doc])*
    ///
    /// While a value is built, its offset holds the position of its target in the buffer, which
    /// [`Fix::fix`] replaces with the distance.
    #[repr(transparent)]
    pub struct $name<$($param,)? O = i32le> {
      offset: O,
      target: PhantomData<($($param,)?)>,
    }

    // SAFETY: the type is its offset (`repr(transparent)`, a byte array: every byte initialized,
    // no `UnsafeCell`). Where it aims is the scheme of this file, from its address and its bytes
    // only, for every type of the scheme. A value in a buffer is only made by a `Builder`, which
    // `Region::new` turns into a region, where a value of the target is where it aims (the
    // contract of `Region::new`); a region never writes to its buffer, nor gives a `&mut` to any
    // of it, and follows a pointer only by what is said here (`Region::get`...). `POINTER` is
    // made for `Target`.
    unsafe impl<$($param: Pun,)? O: Offset> SelfRelative for $name<$($param,)? O> {
      type Target = $target;
      const POINTER: Pointer = $pointer;
    }

    puns::self_relative!(impl[$($param: Pun,)? O: Offset] $name<$($param,)? O>);

    impl<$($param,)? O: Offset> $name<$($param,)? O> {
      /// A pointer that holds the position of its target, not yet the distance to it.
      fn to(position: usize) -> Self {
        let position = isize::try_from(position).expect("a position that an `isize` holds");
        Self { offset: O::new(position), target: PhantomData }
      }
    }

    impl<$($param,)? O: Offset> fmt::Debug for $name<$($param,)? O> {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({:+})", stringify!($name), self.offset.get())
      }
    }

    // SAFETY: the pointer, whose position is `at`, is made to aim at the position that it holds.
    unsafe impl<$($param,)? O: Offset> Fix for $name<$($param,)? O> {
      fn fix(&mut self, at: usize) {
        let target = self.offset.get();
        if target != 0 {
          self.offset = O::new(target - isize::try_from(at).expect("a position"));
        }
      }
    }
  };
}

pointer! {
  /// A pointer to one `T`.
  One[T] T, Pointer::one::<T>(SCHEME, size_of::<O>(), O::ENDIAN)
}

pointer! {
  /// A pointer to one `T`, or none: zero.
  Opt[T] T, Pointer::one::<T>(SCHEME, size_of::<O>(), O::ENDIAN).or_null()
}

pointer! {
  /// A pointer to a sequence of `T`.
  Seq[T] T, Pointer::sequence::<T>(SCHEME, size_of::<O>(), O::ENDIAN, first::<T, O>())
}

pointer! {
  /// A pointer to text.
  Text[] (), Pointer::text(SCHEME, size_of::<O>(), O::ENDIAN)
}

/// The alignment of the first element of a sequence of `T` whose length is an `O`.
const fn first<T, O>() -> usize {
  if align_of::<T>() > size_of::<O>() { align_of::<T>() } else { size_of::<O>() }
}

impl<T, O: Offset> Opt<T, O> {
  /// No pointer.
  #[must_use]
  pub fn none() -> Self {
    Self { offset: O::new(0), target: PhantomData }
  }
}

impl<T> Opt<T> {
  /// No pointer, in a constant.
  pub const NONE: Self = Self { offset: i32le::new(0), target: PhantomData };
}

/// A value that can be put in a buffer: the pointers it holds are made to aim at the positions
/// that they hold, once its own is known.
///
/// # Safety
/// `fix` calls `Fix::fix` for every pointer of the value, with the position of the pointer, and
/// changes nothing else.
pub unsafe trait Fix: Sized {
  /// Makes the pointers of the value aim where they should, the value being at `at`.
  fn fix(&mut self, at: usize);
}

/// The address of a reference, as an integer: what a field is, in its value.
#[must_use]
pub fn addr<T: ?Sized>(value: &T) -> usize {
  ptr::from_ref(value).addr()
}

/// Fixes the `field` of a value at `at` whose address is `whole`.
pub fn field<F: Fix>(whole: usize, at: usize, field: &mut F) {
  let offset = addr(field) - whole;
  field.fix(at + offset);
}

macro_rules! plain {
  ($($ty:ty)+) => {$(
    // SAFETY: no pointer.
    unsafe impl Fix for $ty {
      fn fix(&mut self, _: usize) {}
    }
  )+};
}

plain! {
  () bool char u8 u16 u32 u64 i8 i16 i32 i64
  puns::num::u16le puns::num::u32le puns::num::u64le puns::num::u16be
  i16le i32le i64le i16be i32be i64be
}

// SAFETY: each element, at its position.
unsafe impl<T: Fix, const N: usize> Fix for [T; N] {
  fn fix(&mut self, at: usize) {
    for (i, element) in self.iter_mut().enumerate() {
      element.fix(at + i * size_of::<T>());
    }
  }
}

/// A block of a buffer, of the alignment of every buffer.
#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct Block([MaybeUninit<u8>; ALIGN]);

const _: () = assert!(align_of::<Block>() == ALIGN && size_of::<Block>() == ALIGN);

/// A buffer that values are put in, each at a position aligned for it, before the values that
/// point to them: what a value points to is put first.
pub struct Builder {
  blocks: Vec<Block>,
  len: usize,
}

impl Default for Builder {
  fn default() -> Self {
    Self::new()
  }
}

impl Builder {
  /// An empty buffer. Nothing is put at 0, which is a null `Opt` while a value is built.
  #[must_use]
  pub const fn new() -> Self {
    Self { blocks: Vec::new(), len: 1 }
  }

  /// `size` bytes at a position that is a multiple of `align`, after `before` more.
  ///
  /// A byte that nothing writes follows: what reads past a value reads it (which Miri, and the
  /// oracle of the model check, report), and a pointer, which is put after what it aims at, never
  /// aims at itself (even at an empty sequence), so that it is zero only if null.
  fn reserve(&mut self, before: usize, size: usize, align: usize) -> usize {
    // What the buffer holds is at most as aligned as the buffer.
    assert!(align.is_power_of_two() && align <= ALIGN, "a buffer is aligned to {ALIGN}");
    let at = (self.len + before).next_multiple_of(align);
    self.len = at + size + 1;
    self.blocks.resize(self.len.div_ceil(ALIGN), Block([MaybeUninit::uninit(); ALIGN]));
    at
  }

  /// The start of the buffer, for writes.
  const fn base(&mut self) -> *mut u8 {
    self.blocks.as_mut_ptr().cast()
  }

  /// Puts `value`, and returns its position.
  fn put<T: Fix>(&mut self, mut value: T) -> usize {
    let at = self.reserve(0, size_of::<T>(), align_of::<T>());
    value.fix(at);
    // SAFETY: `at..at + size_of::<T>()` is in the buffer, at an address aligned for `T` (the
    // buffer is aligned to `ALIGN`, which `reserve` asserted is enough, and `at` is a multiple
    // of the alignment of `T`).
    unsafe { self.base().add(at).cast::<T>().write(value) };
    at
  }

  /// Writes the length `len` right before `at`.
  fn length<O: Offset>(&mut self, at: usize, len: usize) {
    let len = O::new(isize::try_from(len).expect("a length that an `isize` holds"));
    // SAFETY: `at - size_of::<O>()..at` is in the buffer (`reserve` left that much before
    // `at`); an `O` is a byte array, of no alignment.
    unsafe { self.base().add(at - size_of::<O>()).cast::<O>().write(len) };
  }

  /// Puts `value`, and returns a pointer to it.
  pub fn one<T: Fix, O: Offset>(&mut self, value: T) -> One<T, O> {
    One::to(self.put(value))
  }

  /// Puts `value`, if there is one, and returns a pointer to it, or none.
  pub fn opt<T: Fix, O: Offset>(&mut self, value: Option<T>) -> Opt<T, O> {
    value.map_or_else(Opt::none, |value| Opt::to(self.put(value)))
  }

  /// Puts the `values`, one after the other, and their number before them; returns a pointer to
  /// the first.
  pub fn seq<T: Fix, O: Offset>(&mut self, values: impl IntoIterator<Item = T>) -> Seq<T, O> {
    let values: Vec<T> = values.into_iter().collect();
    let size = values.len() * size_of::<T>();
    let at = self.reserve(size_of::<O>(), size, first::<T, O>());
    self.length::<O>(at, values.len());
    for (i, mut value) in values.into_iter().enumerate() {
      let place = at + i * size_of::<T>();
      value.fix(place);
      // SAFETY: in the buffer (`reserve`), aligned for `T` (`first` is at least its alignment,
      // and so is the size of `T`, between two elements).
      unsafe { self.base().add(place).cast::<T>().write(value) };
    }
    Seq::to(at)
  }

  /// Puts the bytes of `text`, and their number before them; returns a pointer to the first.
  pub fn text<O: Offset>(&mut self, text: &str) -> Text<O> {
    let at = self.reserve(size_of::<O>(), text.len(), 1);
    self.length::<O>(at, text.len());
    // SAFETY: `at..at + text.len()` is in the buffer (`reserve`), and is not `text`.
    unsafe { ptr::copy_nonoverlapping(text.as_ptr(), self.base().add(at), text.len()) };
    Text::to(at)
  }
}

/// A buffer whose root is a `T`: what [`Builder`] made.
pub struct Region<T> {
  blocks: Vec<Block>,
  len: usize,
  root: usize,
  value: PhantomData<T>,
}

impl<T: Fix> Region<T> {
  /// Puts `root` in what `builder` holds: the region of the value.
  ///
  /// # Safety
  /// Every pointer of `root`, and of what was put in `builder`, was made by `builder` (`one`,
  /// `opt`, `seq`, `text`), with what it aims at, and is in one value only; every value that
  /// holds pointers implements [`Fix`] as it says.
  pub unsafe fn new(mut builder: Builder, root: T) -> Self {
    let root = builder.put(root);
    Self { blocks: builder.blocks, len: builder.len, root, value: PhantomData }
  }
}

impl<T> Region<T> {
  /// The start of the buffer, with the provenance of all of it.
  const fn base(&self) -> *const u8 {
    self.blocks.as_ptr().cast()
  }

  /// The bytes of the buffer, some of which are not initialized (padding, gaps).
  #[must_use]
  pub const fn bytes(&self) -> &[MaybeUninit<u8>] {
    // SAFETY: `len` bytes of the buffer, of any value; nothing writes to them.
    unsafe { core::slice::from_raw_parts(self.base().cast(), self.len) }
  }

  /// The position of the root.
  #[must_use]
  pub const fn root(&self) -> usize {
    self.root
  }

  /// The position in the buffer of the pointer `pointer`, and its offset.
  ///
  /// # Panics
  /// If it is not in the buffer.
  fn place<P, O: Offset>(&self, pointer: &P, offset: O) -> (usize, isize) {
    let here = addr(pointer).wrapping_sub(self.base().addr());
    assert!(here + size_of::<P>() <= self.len, "a pointer of another buffer");
    (here, offset.get())
  }

  /// The position of a target of `size` bytes at `offset` from `here`.
  ///
  /// # Panics
  /// If it is not in the buffer.
  fn aim(&self, here: usize, offset: isize, size: usize) -> usize {
    let to = here.checked_add_signed(offset).filter(|to| to + size <= self.len);
    to.expect("a pointer that aims in its buffer")
  }

  /// The length right before the `O` at `to`.
  fn length<O: Offset>(&self, to: usize) -> usize {
    let at = to.checked_sub(size_of::<O>()).expect("a length in the buffer");
    // SAFETY: in the buffer (`to` is), where the builder wrote a length (the scheme, which
    // `Region::new` promises), an `O`, of no alignment.
    let len = unsafe { *self.base().add(at).cast::<O>() };
    usize::try_from(len.get()).expect("a length")
  }

  /// What `pointer` aims at.
  ///
  /// # Panics
  /// If the pointer is not in this region.
  pub fn get<'a, U, O: Offset>(&'a self, pointer: &'a One<U, O>) -> &'a U {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, size_of::<U>());
    // SAFETY: a pointer of the scheme in the buffer, which `Region::new` promises aims at a
    // value of its target, or one that `puns::view_ref` read as such: a valid `U` (the contract
    // of `SelfRelative`), aligned for it.
    unsafe { &*self.base().add(to).cast::<U>() }
  }

  /// What `pointer` aims at, if anything.
  ///
  /// # Panics
  /// If the pointer is not in this region.
  pub fn opt<'a, U, O: Offset>(&'a self, pointer: &'a Opt<U, O>) -> Option<&'a U> {
    let (here, offset) = self.place(pointer, pointer.offset);
    if offset == 0 {
      return None;
    }
    let to = self.aim(here, offset, size_of::<U>());
    // SAFETY: as in `get`.
    Some(unsafe { &*self.base().add(to).cast::<U>() })
  }

  /// The sequence that `pointer` aims at.
  ///
  /// # Panics
  /// If the pointer is not in this region.
  pub fn seq<'a, U, O: Offset>(&'a self, pointer: &'a Seq<U, O>) -> &'a [U] {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, 0);
    let len = self.length::<O>(to);
    self.aim(to, 0, len * size_of::<U>());
    // SAFETY: as in `get`: `len` valid `U`s from `to`, aligned for them.
    unsafe { core::slice::from_raw_parts(self.base().add(to).cast::<U>(), len) }
  }

  /// The text that `pointer` aims at.
  ///
  /// # Panics
  /// If the pointer is not in this region.
  pub fn text<'a, O: Offset>(&'a self, pointer: &'a Text<O>) -> &'a str {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, 0);
    let len = self.length::<O>(to);
    self.aim(to, 0, len);
    // SAFETY: as in `get`: `len` bytes of UTF-8 from `to`.
    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.base().add(to), len)) }
  }
}

impl<T> core::ops::Deref for Region<T> {
  type Target = T;

  fn deref(&self) -> &T {
    // SAFETY: the root, a valid `T` (`Region::new`), aligned for it (`Builder::put`).
    unsafe { &*self.base().add(self.root).cast::<T>() }
  }
}
