//! ***Type punning, proven at compile time: a safe alternative to `transmute`, and to hand-written
//! parsing of bytes. Even for dynamically sized types.***
//!
//! A *pun* reads the bytes of a value of one type, the *source*, as a value of another, the
//! *target*. The layouts and the valid values of both are compared at compile time, and a pun is
//!
//! - **proven** when every source is a valid target: it compiles to nothing;
//! - **checked** when only some are: a `try_` function (or a `from_*` function, from bytes)
//!   checks at run time what could not be proven, and nothing else;
//! - **refused** when it can never be valid, or could read padding, or could write through a
//!   `&mut` what the source cannot hold: it does not compile.
//!
//! ```
//! use puns::Pun;
//!
//! #[derive(Pun, Debug, PartialEq)]
//! #[repr(C)]
//! struct Position {
//!   x: u8,
//!   visible: bool,
//! }
//!
//! // Proven: every `Position` is two initialized bytes, nothing is checked, nothing is compiled.
//! assert_eq!(puns::pun_ref::<_, [u8; 2]>(&Position { x: 7, visible: true }), &[7, 1]);
//!
//! // Checked: not every pair of bytes is a `Position`, only the `bool` is looked at.
//! assert_eq!(puns::try_pun_ref(&[7u8, 1]), Ok(&Position { x: 7, visible: true }));
//! assert_eq!(puns::try_pun_ref::<_, Position>(&[7u8, 2]), Err(puns::Error::Validity));
//! ```
//!
//! # Functions
//!
//! What is read (a typed value, or bytes), by how much of it:
//!
//! |               | Whole                                  | Front                     | Back                      |
//! |---------------|----------------------------------------|---------------------------|---------------------------|
//! | a typed value | [`pun`], [`pun_ref`], [`pun_mut`] (and [`try_pun`], [`try_pun_ref`], [`try_pun_mut`]) | [`head`], [`head_ref`], [`head_mut`] | [`tail`], [`tail_ref`], [`tail_mut`] |
//! | bytes         | [`from_bytes`], [`from_bytes_mut`], [`as_bytes`] | [`from_head`], [`from_head_mut`] | [`from_tail`], [`from_tail_mut`] |
//!
//! The first row is proven, but for the `try_` functions, which are checked; the second row is
//! checked: the size, the address, then what bytes do not prove of the target. A part is the
//! first or the last `size_of::<D>()` bytes of the source (or, for the front, as many elements of
//! a slice as fit). A `&mut` to a part is refused when what is written through it could break
//! the rest of the source: a rule that holds over the part, an enum of which it is only a part.
//!
//! ```
//! let mut bytes = [0u8, 1, 2, 3, 4];
//! let (first, rest) = puns::from_head::<[u8; 2]>(&bytes).unwrap();
//! let (_, last) = puns::from_tail::<[u8; 2]>(rest).unwrap();
//! assert_eq!((first, last), (&[0, 1], &[3, 4]));
//! assert_eq!(puns::tail::<[u8; 5], [u8; 2]>(bytes), [3, 4]);
//! *puns::head_mut::<_, [u8; 2]>(&mut bytes) = [7, 7];
//! assert_eq!(bytes, [7, 7, 2, 3, 4]);
//! ```
//!
//! # What it knows
//!
//! - Integers, `bool`, `char`, `NonZero`, enums without fields: scalars with a set of valid
//!   values. See [`num`] for integers of a given byte order (`u16be`) or value (`u16bec<V>`).
//! - Structs (of any `repr`), tuples, arrays, slices, and structs that end with a slice: their
//!   padding, and what each of their bytes can be.
//! - Enums with fields (`repr(C)`, `repr(u8)`...): their tag, and what each variant is made of.
//! - `str`: bytes that are UTF-8.
//! - What `#[derive(Pun)]` is told: values that a field can take, sizes and lengths that are in
//!   a field, assertions over fields. See [`macro@Pun`], and the examples.
//!
//! # Examples
//!
//! The files of `examples/` declare formats with the derive only, then read them from bytes and
//! check that corrupted ones are rejected; each tells what it does not cover, and why:
//!
//! - `ethernet.rs`: Ethernet, VLANs, ARP, IPv4, IPv6 and its extension headers, UDP, TCP, ICMP.
//! - `ip_services.rs`: DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP.
//! - `transport.rs`: TLS, QUIC, HTTP/2, WebSocket, SSH, MQTT, CoAP, SCTP.
//! - `routing.rs`: BGP, OSPF, GRE, VXLAN, Geneve, IGMP, ICMPv6 neighbor discovery.
//! - `link.rs`: 802.11, Bluetooth HCI and L2CAP, CAN, Modbus, SLIP, PPP.
//! - `images.rs`: PNG, BMP, TrueType and OpenType font directories.
//! - `media.rs`: RIFF and WAVE, MP4 boxes.
//! - `archives.rs`: ZIP, tar, gzip.
//! - `executables.rs`: ELF, PE/COFF, Mach-O, Java class files, WebAssembly.
//! - `storage.rs`: SQLite, FAT, GPT, USB descriptors.
//! - `encodings.rs`: DER (SNMP, Kerberos), Protocol Buffers, MessagePack, CBOR, text fields.
//!
//! # What deriving `Pun` means
//!
//! A type is known by its layout and by what the type system enforces of its bytes, nothing more.
//! Two consequences are by design.
//!
//! `PhantomData<T>` is a scalar of no byte, for any `T`: types that differ only by phantom
//! parameters, or by lifetimes, convert to each other freely. A type whose parameters are a brand
//! (a unit, an invariant lifetime that ties a value to a scope, a marker of what a value was
//! checked against) should not derive `Pun`:
//!
//! ```
//! use core::marker::PhantomData;
//!
//! #[derive(Clone, Copy)]
//! struct Meters;
//! #[derive(Clone, Copy)]
//! struct Feet;
//!
//! #[derive(puns::Pun, Clone, Copy)]
//! #[repr(C)]
//! struct Length<U> {
//!   value: u32,
//!   unit: PhantomData<U>,
//! }
//!
//! // Not what a unit is for: 3 meters are 3 feet.
//! let feet: Length<Feet> = puns::pun(Length::<Meters> { value: 3, unit: PhantomData });
//! assert_eq!(feet.value, 3);
//! ```
//!
//! Deriving `Pun` makes every field as accessible as if it were public: a value can be made from
//! anything whose bytes are valid for its fields (and the rules the derive is told of), read as
//! anything its bytes are, and written through a `&mut` to another type. A type whose private
//! fields keep an invariant that the derive is not told of should not derive `Pun`:
//!
//! ```
//! mod even {
//!   /// Only even numbers, says `new`.
//!   #[derive(puns::Pun, Clone, Copy)]
//!   #[repr(transparent)]
//!   pub struct Even(u32);
//!
//!   impl Even {
//!     pub const fn new(value: u32) -> Option<Self> {
//!       if value % 2 == 0 { Some(Self(value)) } else { None }
//!     }
//!
//!     pub const fn get(self) -> u32 {
//!       self.0
//!     }
//!   }
//! }
//!
//! assert!(even::Even::new(3).is_none());
//! // What `new` refuses, `pun` makes.
//! let odd: even::Even = puns::pun(3u32);
//! assert_eq!(odd.get(), 3);
//! ```
//!
//! # What is refused
//!
//! Reading padding:
//!
//! ```compile_fail
//! let _: &[u8; 4] = puns::pun_ref(&(0u8, 0u16));
//! ```
//!
//! Making a value that may not be valid, without a check:
//!
//! ```compile_fail
//! let _: &str = puns::pun_ref(&[0u8; 4]);
//! ```
//!
//! A check that can never succeed:
//!
//! ```compile_fail
//! let _ = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
//! ```
//!
//! A `&mut` through which an invalid value could be written back:
//!
//! ```compile_fail
//! let _: &mut u8 = puns::pun_mut(&mut true);
//! ```
//!
//! ```compile_fail
//! let mut text = String::from("text");
//! let _: &mut [u8] = puns::pun_mut(text.as_mut_str());
//! ```
//!
//! Sizes or alignments that do not match:
//!
//! ```compile_fail
//! let _: &u32 = puns::pun_ref(&[0u8; 4]);
//! ```
//!
//! # Enums with fields
//!
//! An enum is known by its tag: a variant proves the variant of the same tag, and nothing else
//! proves an enum.
//!
//! ```
//! use puns::Pun;
//!
//! #[derive(Pun, Debug, PartialEq, Clone, Copy)]
//! #[repr(u8)]
//! enum Shape {
//!   Circle { radius: u16 },
//!   Rect(u8, bool),
//! }
//!
//! #[derive(Pun, Debug, PartialEq, Clone, Copy)]
//! #[repr(u8)]
//! enum Drawing {
//!   Circle { radius: u16 },
//!   Rect(u8, u8),
//!   Nothing,
//! }
//!
//! // Every `Shape` is a `Drawing`, variant by variant: nothing is checked.
//! assert_eq!(puns::pun::<_, Drawing>(Shape::Rect(3, true)), Drawing::Rect(3, 1));
//! // Not every `Drawing` is a `Shape`.
//! assert_eq!(puns::try_pun(Drawing::Rect(3, 1)), Ok(Shape::Rect(3, true)));
//! assert!(puns::try_pun::<_, Shape>(Drawing::Rect(3, 2)).is_err());
//! assert!(puns::try_pun::<_, Shape>(Drawing::Nothing).is_err());
//! // Bytes are a `Shape` if they start with one of its tags, then are what that variant holds.
//! assert_eq!(puns::try_pun([1u8, 3, 1, 0]), Ok(Shape::Rect(3, true)));
//! assert!(puns::try_pun::<_, Shape>([1u8, 3, 2, 0]).is_err());
//! assert!(puns::try_pun::<_, Shape>([2u8, 0, 0, 0]).is_err());
//! ```
//!
//! A `Shape` is not 4 bytes: the byte after the tag of a `Circle` is padding, and nothing tells a
//! `Circle` from a `Rect` at compile time.
//!
//! ```compile_fail
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! let _: [u8; 4] = puns::pun(Shape::Rect(3, true));
//! ```
//!
//! Neither can a `Drawing` be written where a `Shape` is:
//!
//! ```compile_fail
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Drawing { Circle { radius: u16 }, Rect(u8, u8), Nothing }
//! let mut shape = Shape::Rect(3, true);
//! let _: &mut Drawing = puns::pun_mut(&mut shape);
//! ```
//!
//! The layout of an enum with fields that has no `repr` is not specified, and cannot be told on
//! stable Rust: `#[derive(Pun)]` asks for one.
//!
//! ```compile_fail
//! #[derive(puns::Pun)]
//! enum Unspecified { A(u8), B(u16) }
//! ```
//!
//! Refusals are errors of the monomorphization of the functions: `cargo build` reports them,
//! `cargo check` does not.
//!
//! See `DESIGN.md` for how it works.
#![cfg_attr(not(test), no_std)]

mod analysis;
#[cfg(feature = "verdict")]
#[doc(hidden)]
pub mod hook;
pub mod layout;
pub mod num;
mod ptr;
#[cfg(test)]
mod tests;
mod validate;

/// The example of `README.md`, run with the doc tests.
#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
struct ReadmeDoctests;

use core::{marker::PhantomData, mem::ManuallyDrop};

use analysis::{
  Analysis, Exact, MUTABLE, Marks, TAIL_UNSIZED_SOURCE, TAIL_UNSIZED_TARGET, analyze, analyze_tail,
  mutable, mutable_part,
};
use layout::{Dfa, Endian, Field, Layout, Rule, Spec, eval};
pub use layout::{crc32, sum16};
pub use puns_derive::Pun;
use validate::{Checks, validate};

// -------------------------------------------------------------------------------------------------
// Traits

/// Leaf types: integers and the like, see [`Spec`].
///
/// This is what `#[derive(Pun)]` implements for enums without fields.
///
/// # Safety
/// `SPEC` must describe every valid value of `Self`, and nothing more.
pub unsafe trait Scalar {
  const SPEC: Spec;
}

/// Types made of plain data, whose layout and valid values are known at compile time.
///
/// # Safety
/// Not meant to be implemented by hand: use `#[derive(Pun)]`.
pub unsafe trait Pun {
  /// Type of the elements of an array or of a slice, `()` otherwise.
  #[doc(hidden)]
  type Elem: Pun;
  /// First field of a struct, `()` otherwise.
  #[doc(hidden)]
  type Fields: Fields;
  /// First variant of an enum with fields, `()` otherwise.
  #[doc(hidden)]
  type Variants: Variants;
  /// First of the rules of a struct whose expression walks, `()` if none.
  #[doc(hidden)]
  type Walks: Walks;
  /// A null pointer to `Self`, whose slice length is [`ptr::PROBE_LEN`] if `Self` is unsized.
  /// It must be made with `as` casts only: this is what the pointer tricks are checked against.
  #[doc(hidden)]
  const PROBE: *const Self;
  #[doc(hidden)]
  const LAYOUT: Layout;
}

/// A type that goes with a tag, in what carries it: a payload and the value of the field that
/// tells its type, a variant and its discriminant...
///
/// `K` tells of which tag: a type can be carried by several things. It is, by convention, the type
/// of the tag when nothing is known of what is carried: bytes go with any `K`.
///
/// This is implemented by `#[pun(tag(K = T))]`, see [`macro@Pun`]:
///
/// ```
/// use puns::num::{u8c, u16be};
/// use puns::{Pun, Tag};
///
/// /// The kind of a message, when its payload is not known.
/// #[derive(Pun, Copy, Clone)]
/// #[repr(transparent)]
/// struct Kind(u8);
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Message<P: ?Sized + Tag<Kind> = [u8]> {
///   kind: P::Tag,
///   payload: P,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(tag(Kind = u8c<7>))]
/// struct Ping {
///   sequence: u16be,
/// }
///
/// // A message of any kind, and a message that must be a ping.
/// assert_eq!(puns::try_pun_ref::<_, Message>(&[9u8, 0, 1]).unwrap().kind.0, 9);
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[9u8, 0, 1]).is_err());
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[7u8, 0, 1]).is_ok());
/// ```
pub trait Tag<K: ?Sized>: Pun {
  type Tag: Pun + Copy;
}

/// Bytes are carried with any tag.
impl<K: Pun + Copy> Tag<K> for [u8] {
  type Tag = K;
}

/// A field of a struct, and the ones after it: a list at the type level, for validation.
///
/// # Safety
/// Implemented by `#[derive(Pun)]`.
#[doc(hidden)]
pub unsafe trait Fields {
  type Type: ?Sized + Pun;
  type Next: Fields;
  /// Where the field is in the target, given where the struct is.
  type Path<P: Path>: Path;
  /// `None` at the end of the list.
  const FIELD: Option<Field>;
}

/// A variant of an enum with fields, and the ones after it: a list at the type level.
///
/// # Safety
/// Implemented by `#[derive(Pun)]`.
#[doc(hidden)]
pub unsafe trait Variants {
  /// First field of the variant.
  type Fields: Fields;
  type Next: Variants;
  /// The tag of the variant, `None` at the end of the list.
  const TAG: Option<u128>;
}

// SAFETY: the end of every list.
unsafe impl Variants for () {
  type Fields = ();
  type Next = ();
  const TAG: Option<u128> = None;
}

/// A rule whose expression walks, and the ones after it: a list at the type level.
///
/// # Safety
/// Implemented by `#[derive(Pun)]`.
#[doc(hidden)]
pub unsafe trait Walks {
  /// Index of the rule in the layout of the struct, `usize::MAX` at the end of the list.
  const RULE: usize;
  /// Number of operations of the expression of the rule.
  type Ops: Count;
  /// Number of operations of the longest of the two expressions of its walk.
  type Inner: Count;
  type Next: Walks;
}

// SAFETY: the end of every list.
unsafe impl Walks for () {
  const RULE: usize = usize::MAX;
  type Ops = Zero;
  type Inner = Zero;
  type Next = ();
}

/// A number, as a type: what a recursion over the operations of an expression goes down.
///
/// Sealed: the code that validates a value trusts it.
#[doc(hidden)]
pub trait Count: sealed::Sealed {
  const VALUE: usize;
  type Less: Count;
}

#[doc(hidden)]
pub struct Zero;

#[doc(hidden)]
pub struct More<N>(PhantomData<N>);

impl sealed::Sealed for Zero {}
impl<N: Count> sealed::Sealed for More<N> {}
impl sealed::Sealed for Root {}
impl<P: Path, F: ?Sized + Fields> sealed::Sealed for At<P, F> {}

impl Count for Zero {
  const VALUE: usize = 0;
  type Less = Zero;
}

impl<N: Count> Count for More<N> {
  const VALUE: usize = N::VALUE + 1;
  type Less = N;
}

/// Implemented by this crate only: what implements [`Path`] and [`Count`] is trusted.
mod sealed {
  pub trait Sealed {}
}

/// Where something is in a target, for validation.
///
/// Sealed: the code that validates a value trusts it.
#[doc(hidden)]
pub trait Path: sealed::Sealed {
  /// Index of its first leaf.
  const LEAF: usize;
}

#[doc(hidden)]
pub struct Root;

#[doc(hidden)]
pub struct At<P, F: ?Sized>(PhantomData<P>, PhantomData<F>);

impl Path for Root {
  const LEAF: usize = 0;
}

impl<P: Path, F: ?Sized + Fields> Path for At<P, F> {
  const LEAF: usize = P::LEAF
    + match F::FIELD {
      Some(field) => field.leaf(),
      None => 0,
    };
}

// SAFETY: the end of every list.
unsafe impl Fields for () {
  type Type = ();
  type Next = ();
  type Path<P: Path> = P;
  const FIELD: Option<Field> = None;
}

// SAFETY: `SPEC` describes the type.
unsafe impl<T: Scalar> Pun for T {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::scalar::<T>(T::SPEC);
}

// SAFETY: an array is its elements.
unsafe impl<T: Pun, const N: usize> Pun for [T; N] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::array(&T::LAYOUT, N);
}

// SAFETY: a slice is its elements.
unsafe impl<T: Pun> Pun for [T] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::slice_from_raw_parts(core::ptr::null(), ptr::PROBE_LEN);
  // SAFETY: the probe of a slice.
  const LAYOUT: Layout = unsafe { Layout::slice::<T>(Self::PROBE, &T::LAYOUT) };
}

// SAFETY: a `str` is bytes that are UTF-8.
unsafe impl Pun for str {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const str;
  // SAFETY: the probe of a `str`, which is the one of a slice.
  const LAYOUT: Layout = unsafe { Layout::pattern::<str>(Self::PROBE, Dfa::UTF8) };
}

// SAFETY: `ManuallyDrop<T>` is a `T`.
unsafe impl<T: ?Sized + Pun> Pun for ManuallyDrop<T> {
  type Elem = T::Elem;
  type Fields = T::Fields;
  type Variants = T::Variants;
  type Walks = T::Walks;
  const PROBE: *const Self = T::PROBE as *const Self;
  const LAYOUT: Layout = T::LAYOUT;
}

/// The `I`th field of the tuple `T`.
#[doc(hidden)]
pub struct TupleField<T, const I: usize>(PhantomData<T>);

macro_rules! tuple_field {
  ([$($all:ident)+] $index:tt $ty:ident $next:ty) => {
    // SAFETY: the field of the layout of the tuple.
    unsafe impl<$($all: Pun),+> Fields for TupleField<($($all,)+), $index> {
      type Type = $ty;
      type Next = $next;
      type Path<P: Path> = At<P, Self>;
      const FIELD: Option<Field> = Some(*<($($all,)+) as Pun>::LAYOUT.field($index));
    }
  };
}

macro_rules! tuple {
  ([$($all:ident)+] $($index:tt)+) => {
    // SAFETY: the offsets come from the compiler.
    unsafe impl<$($all: Pun),+> Pun for ($($all,)+) {
      type Elem = ();
      type Fields = TupleField<Self, 0>;
      type Variants = ();
      type Walks = ();
      const PROBE: *const Self = core::ptr::null();
      // SAFETY: a tuple of `Sized` things is `Sized`.
      const LAYOUT: Layout = unsafe {
        Layout::structure::<Self>(
          Self::PROBE,
          const {
            &Layout::sort_fields([
              $(Field::new(core::mem::offset_of!(Self, $index), &$all::LAYOUT)),+
            ])
          },
          &[],
        )
      };
    }
  };
}

tuple!([A] 0);
tuple_field!([A] 0 A ());
tuple!([A B] 0 1);
tuple_field!([A B] 0 A TupleField<(A, B), 1>);
tuple_field!([A B] 1 B ());
tuple!([A B C] 0 1 2);
tuple_field!([A B C] 0 A TupleField<(A, B, C), 1>);
tuple_field!([A B C] 1 B TupleField<(A, B, C), 2>);
tuple_field!([A B C] 2 C ());
tuple!([A B C D] 0 1 2 3);
tuple_field!([A B C D] 0 A TupleField<(A, B, C, D), 1>);
tuple_field!([A B C D] 1 B TupleField<(A, B, C, D), 2>);
tuple_field!([A B C D] 2 C TupleField<(A, B, C, D), 3>);
tuple_field!([A B C D] 3 D ());

macro_rules! impl_any {
  ($($ty:ty)+) => {$(
    // SAFETY: every bit pattern is valid.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>();
    }
  )+};
}

impl_any!(() u8 u16 u32 u64 u128 usize);

macro_rules! impl_bits {
  ($($ty:ty)+) => {$(
    // SAFETY: every bit pattern is valid.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>().not_a_number();
    }
  )+};
}

impl_bits!(i8 i16 i32 i64 i128 isize f32 f64);

// SAFETY: no byte.
unsafe impl<T: ?Sized> Scalar for PhantomData<T> {
  const SPEC: Spec = Spec::any::<Self>();
}

// SAFETY: 0 or 1.
unsafe impl Scalar for bool {
  const SPEC: Spec = Spec::ranges::<bool>(Endian::NATIVE, &[(0, 1)]).not_a_number();
}

// SAFETY: a Unicode scalar value.
unsafe impl Scalar for char {
  const SPEC: Spec =
    Spec::ranges::<char>(Endian::NATIVE, &[(0, 0xD7FF), (0xE000, 0x10_FFFF)]).not_a_number();
}

macro_rules! impl_non_zero {
  ($($ty:ident)+) => {$(
    // SAFETY: anything but 0.
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(Endian::NATIVE, &[(1, $ty::MAX as u128)]);
    }

    // SAFETY: `None` is 0, this is documented.
    unsafe impl Scalar for Option<core::num::NonZero<$ty>> {
      const SPEC: Spec = Spec::any::<$ty>();
    }
  )+};
}

impl_non_zero!(u8 u16 u32 u64 u128 usize);

// -------------------------------------------------------------------------------------------------
// `Error`

/// Why a value cannot be seen as another type.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Error {
  /// The address of the value is not a multiple of the alignment of the target.
  Alignment,
  /// The size of the value is not a size that the target can have.
  Size,
  /// The value is not a valid target: a scalar is out of its valid values, a string is not
  /// UTF-8, a rule does not hold...
  Validity,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(match self {
      Error::Alignment => "value is not aligned for the target type",
      Error::Size => "value has not the size of the target type",
      Error::Validity => "value is not valid for the target type",
    })
  }
}

impl core::error::Error for Error {}

// -------------------------------------------------------------------------------------------------
// `Plan`

/// What is known at compile time of a pun from `S` to `D`.
struct Plan<S: ?Sized, D: ?Sized>(PhantomData<S>, PhantomData<D>);

/// What a function requires of a pun.
#[derive(Copy, Clone)]
struct Need {
  /// The target may be a part of the source: its first bytes, or with `Plan::TAIL` its last.
  part: bool,
  /// What is not proven is checked at run time.
  fallible: bool,
  /// The target is behind a reference.
  reference: bool,
}

impl Need {
  const VALUE: Need = Need { part: false, fallible: false, reference: false };
  const REF: Need = Need { reference: true, ..Need::VALUE };
  const PART: Need = Need { part: true, ..Need::VALUE };
  const PART_REF: Need = Need { part: true, ..Need::REF };
  const TRY: Need = Need { fallible: true, ..Need::VALUE };
  const TRY_REF: Need = Need { fallible: true, ..Need::REF };
}

/// Why a pun of which `analysis` is the analysis does not meet the `need`, if it does not.
const fn check(analysis: &Analysis, need: Need) -> Option<Message> {
  let Analysis { fit, relation, aligned, shift } = *analysis;
  match fit.exact {
    _ if need.part && fit.shrink => {}
    _ if need.part => {
      return Some(Message::new(
        "the source can be smaller than the target (if it has a slice, it may be empty)",
      ));
    }
    Exact::Always => {}
    Exact::Sometimes if need.fallible => {}
    Exact::Sometimes => {
      return Some(Message::new(
        "the size of the source may not be a size of the target, depending on the length of \
         its slice.\nUse `try_pun_ref` or `try_pun_mut` to check it at run time",
      ));
    }
    Exact::Never(why) if fit.shrink => {
      return Some(Message::new(why).str(
        ".\nUse `head` or `tail` (or their `_ref` and `_mut` forms) to take a part of the source",
      ));
    }
    Exact::Never(why) => return Some(Message::new(why)),
  }
  // Offsets of the target.
  if let Some(offset) = relation.uninit {
    return Some(
      Message::new("a byte that the target requires to be initialized is padding in the source")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let Some(offset) = relation.disjoint {
    return Some(
      Message::new("no value of the source is valid for the target")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let (Some(offset), false) = (relation.partial, need.fallible) {
    return Some(
      Message::new(
        "some values of the source are not valid for the target, or do not prove one of its \
         rules.\nUse a `try_*` function to check them at run time",
      )
      .at(offset.saturating_sub(shift)),
    );
  }
  if need.reference && !aligned && !need.fallible {
    return Some(Message::new(
      "the target has a greater alignment than the source.\nUse `try_pun_ref` or \
       `try_pun_mut` to check the address at run time",
    ));
  }
  None
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Plan<S, D> {
  const ANALYSIS: Analysis = analyze(&S::LAYOUT, &D::LAYOUT);

  /// The analysis of the last bytes of the source as the target.
  const TAIL_ANALYSIS: Analysis = analyze_tail(&S::LAYOUT, &D::LAYOUT);

  const MUTABLE: bool = mutable(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit);

  /// Fail to compile if `message` says why.
  const fn require(message: Option<Message>) {
    if let Some(message) = message {
      message.panic();
    }
  }

  /// Why the pun does not meet the `need`, if it does not.
  const fn check(need: Need) -> Option<Message> {
    check(&Self::ANALYSIS, need)
  }

  /// Why the pun of the last bytes of the source does not meet the `need` (a part), if it does
  /// not.
  const fn check_tail(need: Need) -> Option<Message> {
    if S::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_SOURCE));
    }
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_TARGET));
    }
    match check(&Self::TAIL_ANALYSIS, Need { reference: false, ..need }) {
      None if need.reference && !Self::TAIL_ANALYSIS.aligned => Some(Message::new(
        "the target has a greater alignment than the source.\nUse `tail` to copy it, or \
         `from_tail` to take it from bytes, whose address is checked at run time",
      )),
      verdict => verdict,
    }
  }

  const BY_VALUE: () = Self::require(Self::check(Need::VALUE));
  const BY_REF: () = Self::require(Self::check(Need::REF));
  const BY_MUT: () = {
    let () = Self::BY_REF;
    if !Self::MUTABLE {
      Message::new(MUTABLE).panic();
    }
  };
  const HEAD: () = Self::require(Self::check(Need::PART));
  const HEAD_REF: () = Self::require(Self::check(Need::PART_REF));
  const TAIL: () = Self::require(Self::check_tail(Need::PART));
  const TAIL_REF: () = Self::require(Self::check_tail(Need::PART_REF));
  const HEAD_MUT: () = Self::require(Self::check_head_mut());
  const TAIL_MUT: () = Self::require(Self::check_tail_mut());
  const TRY: () = Self::require(Self::check(Need::TRY));
  const TRY_REF: () = Self::require(Self::check(Need::TRY_REF));
  const FROM_TAIL: () = Self::require(Self::check_from_tail());
  const FROM_TAIL_MUT: () = {
    let () = Self::FROM_TAIL;
    if !Self::MUTABLE {
      Message::new(MUTABLE).panic();
    }
  };

  /// Why the target cannot be taken from the end of the source (bytes) at run time, if it
  /// cannot.
  const fn check_from_tail() -> Option<Message> {
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(
        "the target is unsized: where it starts cannot be found from the end of the bytes.\nUse \
         `from_head` or `from_bytes`",
      ));
    }
    Self::check(Need::TRY_REF)
  }

  /// Why a `&mut` to the first bytes of the source cannot be handed out, if it cannot: what a
  /// `&` cannot, and what [`mutable_part`] refuses.
  const fn check_head_mut() -> Option<Message> {
    if let Some(message) = Self::check(Need::PART_REF) {
      return Some(message);
    }
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit, 0) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  /// Why a `&mut` to the last bytes of the source cannot be handed out, if it cannot.
  const fn check_tail_mut() -> Option<Message> {
    if let Some(message) = Self::check_tail(Need::PART_REF) {
      return Some(message);
    }
    let Analysis { fit, shift, .. } = Self::TAIL_ANALYSIS;
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &fit, shift) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  const TRY_MUT: () = {
    let () = Self::TRY_REF;
    if !Self::MUTABLE {
      Message::new(MUTABLE).panic();
    }
  };

  /// Pointer to the target at `source`, when there is nothing to check (`BY_REF`,
  /// `HEAD_REF`).
  #[inline(always)]
  const fn cast(source: *const S) -> *const D {
    let (head, stride, exact) = const {
      // The slice length of the target is `head + len * stride`, exactly or rounded down.
      let (source, target) = (S::LAYOUT, D::LAYOUT);
      match (target.stride(), Self::ANALYSIS.fit.exact) {
        (0, _) => (0, 0, true),
        (by, Exact::Always) => ((source.head() - target.head()) / by, source.stride() / by, true),
        _ => (0, 0, false),
      }
    };
    let len = if exact {
      head + ptr::metadata(source) * stride
    } else {
      let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
      let extent = layout.head() + ptr::metadata(source) * layout.stride();
      (extent - target.head()) / target.stride()
    };
    ptr::from_raw_parts::<D>(source.cast(), len)
  }

  /// Pointer to the target at the end of `source`, when there is nothing to check (`TAIL_REF`:
  /// both are sized).
  #[inline(always)]
  const fn tail_cast(source: *const S) -> *const D {
    let shift = const { Self::TAIL_ANALYSIS.shift };
    // SAFETY: `shift` is at most the size of the source, at the end of which the target ends.
    ptr::from_raw_parts::<D>(unsafe { source.cast::<u8>().add(shift) }, 0)
  }

  /// Pointer to the target at `source`, with every check that could not be made at compile
  /// time (`TRY`, `TRY_REF`).
  #[inline(always)]
  const fn try_cast(source: *const S) -> Result<*const D, Error> {
    let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
    let addr = source.cast::<u8>();
    if const { !Self::ANALYSIS.aligned } && !ptr::is_aligned(addr, target.align()) {
      return Err(Error::Alignment);
    }
    // The size of a value that exists: no overflow.
    let extent = layout.head() + ptr::metadata(source) * layout.stride();
    if extent < target.head() {
      return Err(Error::Size);
    }
    let rest = extent - target.head();
    let len = match target.stride() {
      0 if rest == 0 => 0,
      stride if stride != 0 && rest.is_multiple_of(stride) => rest / stride,
      _ => return Err(Error::Size),
    };
    // An unsized target of greater alignment may end with more padding than the source has room
    // for, even if its slice has no size (`[()]`).
    if const { D::LAYOUT.is_unsized() && D::LAYOUT.align() > S::LAYOUT.align() } {
      match (target.size_of_val(len), layout.size_of_val(ptr::metadata(source))) {
        (Some(needed), Some(available)) if needed <= available => {}
        _ => return Err(Error::Size),
      }
    }
    let target = ptr::from_raw_parts::<D>(addr, len);
    // SAFETY: the target is in the bounds of the source, which is a valid `S`: what the target
    // reads of it is initialized, since the analysis found no padding under it.
    if !unsafe { validate::<Self, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok(target)
  }
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Checks for Plan<S, D> {
  const MARKS: Marks = Self::ANALYSIS.relation.marks;
}

/// A compile-time error message, without `format!`.
struct Message {
  bytes: [u8; 512],
  len: usize,
}

impl Message {
  const fn new(what: &str) -> Self {
    Self { bytes: [0; 512], len: 0 }.str("\ncannot pun: ").str(what)
  }

  const fn str(mut self, str: &str) -> Self {
    let (bytes, mut i) = (str.as_bytes(), 0);
    while i < bytes.len() && self.len < self.bytes.len() {
      self.bytes[self.len] = bytes[i];
      (self.len, i) = (self.len + 1, i + 1);
    }
    self
  }

  /// Add the offset of what is wrong.
  const fn at(self, offset: usize) -> Self {
    let (mut digits, mut len, mut value) = ([0; 20], 0, offset);
    loop {
      len += 1;
      digits[20 - len] = b'0' + (value % 10) as u8;
      value /= 10;
      if value == 0 {
        break;
      }
    }
    // SAFETY: ASCII digits.
    let digits = unsafe { core::str::from_utf8_unchecked(digits.split_at(20 - len).1) };
    self.str(".\nThis is at byte ").str(digits).str(" of the target")
  }

  const fn panic(self) -> ! {
    let this = self.str(".\n");
    // SAFETY: a concatenation of `str`s (only cut if too long, and messages are ASCII).
    panic!("{}", unsafe { core::str::from_utf8_unchecked(this.bytes.split_at(this.len).0) })
  }
}

// -------------------------------------------------------------------------------------------------
// By value

/// Reinterpret a value as a value of another type of the same size.
///
/// This compiles if every value of `S` is a valid `D`, and then costs nothing.
///
/// ```
/// assert_eq!(puns::pun::<[bool; 2], u16>([true, true]), 0x0101);
/// ```
///
/// ```compile_fail
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::pun(2u8);
/// ```
#[inline(always)]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::BY_VALUE };
  // SAFETY: checked at compile time.
  unsafe { pun_unchecked(value) }
}

/// Reinterpret the first bytes of a value as a value of a type that is not larger.
///
/// This compiles if the first `size_of::<D>()` bytes of every `S` are a valid `D`, and then costs
/// nothing.
///
/// ```
/// assert_eq!(puns::head::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [1, 2]);
/// ```
///
/// ```compile_fail
/// // The second byte is padding.
/// let _: [u8; 2] = puns::head((0u8, 0u16));
/// ```
#[inline(always)]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::HEAD };
  // SAFETY: checked at compile time.
  unsafe { pun_unchecked(value) }
}

/// Reinterpret the last bytes of a value as a value of a type that is not larger.
///
/// This compiles if the last `size_of::<D>()` bytes of every `S` are a valid `D`, and then costs
/// nothing.
///
/// ```
/// assert_eq!(puns::tail::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [3, 4]);
/// // Two `bool`s are two initialized bytes.
/// assert_eq!(puns::tail::<[bool; 3], u16>([false, true, true]), 0x0101);
/// ```
///
/// ```compile_fail
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::tail::<[u8; 2], bool>([1, 7]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Padded {
///   a: u16,
///   b: u8,
/// }
///
/// // The last byte is padding.
/// let _: u8 = puns::tail(Padded { a: 0, b: 0 });
/// ```
#[inline(always)]
pub const fn tail<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::TAIL };
  let shift = const { Plan::<S, D>::TAIL_ANALYSIS.shift };
  // SAFETY: checked at compile time: `shift + size_of::<D>() == size_of::<S>()`, and the bytes of
  // `value` from `shift` are a valid `D` (which may not be aligned there).
  unsafe { (&raw const value).cast::<u8>().add(shift).cast::<D>().read_unaligned() }
}

/// Reinterpret a value as a value of another type of the same size, if it is a valid one.
///
/// Only what could not be proven at compile time is checked.
///
/// ```
/// assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
/// assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
/// ```
///
/// # Errors
/// [`Error::Validity`] if `value` is not a valid `D`.
#[inline]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Result<D, Error> {
  const { Plan::<S, D>::TRY };
  // SAFETY: `value` is a valid `S` of the size of `D`; nothing is assumed of its alignment.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Err(Error::Validity);
  }
  // SAFETY: just validated.
  Ok(unsafe { pun_unchecked(value) })
}

/// # Safety
/// `D` must not be larger than `S`, and the first bytes of `value` must be a valid `D`.
#[inline(always)]
const unsafe fn pun_unchecked<S: Copy, D: Copy>(value: S) -> D {
  union Overlay<S: Copy, D: Copy> {
    source: S,
    target: D,
  }
  // SAFETY: guaranteed by the caller.
  unsafe { Overlay { source: value }.target }
}

// -------------------------------------------------------------------------------------------------
// By reference

/// Reinterpret a reference as a reference to another type.
///
/// This compiles if every value of `S` is a valid `D` of the same size, whatever the length of
/// its slice if it has one, and if `D` has no greater alignment. It then costs nothing.
///
/// ```
/// let bytes: &[u8] = puns::pun_ref(&[0x0102u16, 0x0304][..]);
/// assert_eq!(bytes.len(), 4);
/// let text: &[u8] = puns::pun_ref("héllo");
/// assert_eq!(text.len(), 6);
/// ```
#[inline(always)]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::BY_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a mutable reference as a mutable reference to another type.
///
/// On top of what [`pun_ref`] requires, every value of `D` must be a valid `S`: anything
/// can be written through the result.
///
/// ```
/// let mut value = 0x0102u16;
/// puns::pun_mut::<_, [u8; 2]>(&mut value)[0] = 0x02;
/// assert!(value == 0x0202 || value == 0x0102);
/// ```
#[inline(always)]
pub const fn pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::BY_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterpret a reference as a reference to a type that covers the start of the value.
///
/// If the target has a slice, it takes as many elements as fit. This compiles if those first
/// bytes of every `S` are a valid `D`, and if `D` has no greater alignment. It then costs nothing.
///
/// ```
/// let head: &[u8; 2] = puns::head_ref(&[1u8, 2, 3]);
/// assert_eq!(head, &[1, 2]);
/// let pairs: &[[u8; 2]] = puns::head_ref(&[1u8, 2, 3, 4, 5]);
/// assert_eq!(pairs, &[[1, 2], [3, 4]]);
/// ```
///
/// ```compile_fail
/// // An empty slice has no first byte.
/// let _: &u8 = puns::head_ref::<[u8], u8>(&[1, 2]);
/// ```
#[inline(always)]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::HEAD_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a reference as a reference to a type that covers the end of the value.
///
/// The target is the last `size_of::<D>()` bytes of the source, which must be `Sized`: this
/// compiles if those bytes of every `S` are a valid `D`, and if `D` has no greater alignment (its
/// offset in `S` is then a multiple of its alignment, as sizes are multiples of alignments). It
/// then costs an addition.
///
/// ```
/// let tail: &[u8; 2] = puns::tail_ref(&[1u8, 2, 3]);
/// assert_eq!(tail, &[2, 3]);
/// let last: &u16 = puns::tail_ref(&[1u16, 2, 3]);
/// assert_eq!(*last, 3);
/// ```
///
/// Where the last bytes of an unsized value start is only known at run time: see [`from_tail`].
///
/// ```compile_fail
/// let _: &u8 = puns::tail_ref::<[u8], u8>(&[1, 2]);
/// ```
///
/// ```compile_fail
/// // A greater alignment.
/// let _: &u16 = puns::tail_ref(&[0u8; 4]);
/// ```
#[inline(always)]
pub const fn tail_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::TAIL_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::tail_cast(value) }
}

/// Reinterpret a mutable reference as a mutable reference to a type that covers the start of the
/// value.
///
/// On top of what [`head_ref`] requires, whatever is written through the result must leave a
/// valid `S`: every value of `D` must be valid for the bytes it covers, and nothing of `S`
/// outside of them may depend on them. So a source is refused if a rule of one of its structs
/// (a size, a length, an assertion, a region) holds over those bytes, or if one of its enums is
/// only partly covered (its tag in, its fields out, or the reverse).
///
/// ```
/// let mut words = [0x0102u16, 0x0304];
/// *puns::head_mut::<_, u16>(&mut words) = 7;
/// assert_eq!(words, [7, 0x0304]);
/// let mut bytes = [0u8; 5];
/// puns::head_mut::<_, [[u8; 2]]>(&mut bytes[..]).fill([1, 2]);
/// assert_eq!(bytes, [1, 2, 1, 2, 0]);
/// ```
///
/// ```compile_fail
/// // A `u8` can be 2, which is not a `bool`.
/// let _: &mut u8 = puns::head_mut(&mut [true, false]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun)]
/// #[repr(u8)]
/// enum Shape {
///   Circle { radius: u16 },
///   Rect(u8, bool),
/// }
///
/// // The tag of the shape, without what it tells of.
/// let _: &mut u8 = puns::head_mut(&mut Shape::Rect(1, true));
/// ```
#[inline(always)]
pub const fn head_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::HEAD_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterpret a mutable reference as a mutable reference to a type that covers the end of the
/// value.
///
/// On top of what [`tail_ref`] requires, whatever is written through the result must leave a
/// valid `S`, as for [`head_mut`].
///
/// ```
/// let mut words = [0x0102u16, 0x0304, 0x0506];
/// *puns::tail_mut::<_, [u16; 2]>(&mut words) = [7, 8];
/// assert_eq!(words, [0x0102, 7, 8]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun)]
/// #[repr(C)]
/// #[pun(assert(a < b))]
/// struct Ordered {
///   a: u8,
///   b: u8,
/// }
///
/// // Writing `b` could break the rule.
/// let _: &mut u8 = puns::tail_mut(&mut Ordered { a: 1, b: 2 });
/// ```
#[inline(always)]
pub const fn tail_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::TAIL_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::tail_cast(source).cast_mut() }
}

/// Reinterpret a reference as a reference to another type, if the value is a valid one.
///
/// Only what could not be proven at compile time is checked: the size if it depends on a slice
/// length, the address if the target has a greater alignment, and the scalars, strings and rules
/// of the target that the source does not guarantee.
///
/// ```
/// assert_eq!(puns::try_pun_ref::<[u8], str>(b"caf\xc3\xa9"), Ok("café"));
/// assert!(puns::try_pun_ref::<[u8], str>(b"caf\xc3").is_err());
/// ```
///
/// This is a `const fn`, but a target of greater alignment than the source cannot be made at
/// compile time, where addresses are not known.
///
/// # Errors
/// [`Error::Alignment`] if the address of `value` is not aligned for `D`, [`Error::Size`] if its
/// size is not one that `D` can have, [`Error::Validity`] if it is not a valid `D`.
#[inline]
pub const fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Result<&D, Error> {
  const { Plan::<S, D>::TRY_REF };
  match Plan::<S, D>::try_cast(value) {
    // SAFETY: validated.
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterpret a mutable reference as a mutable reference to another type, if the value is a
/// valid one.
///
/// See [`try_pun_ref`] and [`pun_mut`].
///
/// # Errors
/// As [`try_pun_ref`].
#[inline]
pub const fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Result<&mut D, Error> {
  const { Plan::<S, D>::TRY_MUT };
  let source: *mut S = value;
  match Plan::<S, D>::try_cast(source) {
    // SAFETY: validated.
    Ok(target) => Ok(unsafe { &mut *target.cast_mut() }),
    Err(error) => Err(error),
  }
}

/// A value whose bytes are all `0`.
///
/// ```
/// assert_eq!(puns::zeroed::<(u32, [bool; 2])>(), (0, [false; 2]));
/// ```
///
/// ```compile_fail
/// let _ = puns::zeroed::<core::num::NonZero<u8>>();
/// ```
#[inline(always)]
#[must_use]
pub const fn zeroed<T: Pun + Copy>() -> T {
  const { assert!(T::LAYOUT.zeroable(), "zero is not a valid value of this type") };
  // SAFETY: checked at compile time.
  unsafe { core::mem::zeroed() }
}

/// The value of an integer field, as `#[pun(get(..))]` expressions see it.
#[doc(hidden)]
#[inline(always)]
pub const fn int<T: Pun>(value: &T) -> u128 {
  let spec = const {
    match T::LAYOUT.as_integer() {
      Some(spec) => spec,
      None => panic!(
        "a getter reads the unsigned integer fields of its struct as `u128`s, and this field is \
         not one (a signed or floating-point number, a `bool`, a `char` or an enum is not)"
      ),
    }
  };
  // SAFETY: a `T` is that integer, which is not allowed to be uninitialized.
  unsafe { spec.load(core::ptr::from_ref::<T>(value).cast()) }
}

/// The bytes of a value that has no padding.
///
/// Convenient sugar for [`pun_ref::<S, [u8]>`](pun_ref), with identical behaviour: it exists
/// because it names the target, bytes, so that only the source is left to infer.
///
/// ```
/// assert_eq!(puns::as_bytes(&[1u16.to_be(), 2u16.to_be()]), &[0, 1, 0, 2]);
/// ```
///
/// ```compile_fail
/// // The second byte is padding.
/// let _ = puns::as_bytes(&(0u8, 0u16));
/// ```
#[inline(always)]
pub const fn as_bytes<S: ?Sized + Pun>(value: &S) -> &[u8] {
  pun_ref(value)
}

/// A reference to bytes as a reference to a `D`, if they are one.
///
/// Convenient sugar for [`try_pun_ref::<[u8], D>`](try_pun_ref), with identical behaviour. It
/// exists because it pins the source to bytes, which a call to `try_pun_ref` leaves to inference
/// (`try_pun_ref(&[1, 0])` is a pun of `[i32; 2]`: an unsuffixed integer literal is an `i32`
/// unless something says otherwise), and because it takes one type parameter.
///
/// The size is checked if it depends on a slice length, the address if `D` has an alignment
/// greater than 1, and then what bytes do not prove of `D`.
///
/// ```
/// use puns::{Error, Pun};
///
/// #[derive(Pun, Debug, PartialEq)]
/// #[repr(C)]
/// struct Position {
///   x: u8,
///   visible: bool,
/// }
///
/// assert_eq!(puns::from_bytes(&[7, 1]), Ok(&Position { x: 7, visible: true }));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 2]), Err(Error::Validity));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 1, 0]), Err(Error::Size));
/// assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("caf\u{e9}"));
/// ```
///
/// The source is bytes, whatever the literal:
///
/// ```compile_fail
/// let _ = puns::from_bytes::<[u8; 2]>(&[7i32, 1]);
/// ```
///
/// # Errors
/// As [`try_pun_ref`]: [`Error::Alignment`], [`Error::Size`], [`Error::Validity`].
#[inline]
pub const fn from_bytes<D: ?Sized + Pun>(bytes: &[u8]) -> Result<&D, Error> {
  try_pun_ref::<[u8], D>(bytes)
}

/// A mutable reference to bytes as a mutable reference to a `D`, if they are one.
///
/// Convenient sugar for [`try_pun_mut::<[u8], D>`](try_pun_mut), with identical behaviour; see
/// [`from_bytes`] for why it exists. As anything can be written through the result, `D` must not
/// have padding: bytes would be de-initialized.
///
/// ```
/// let mut bytes = [0u8, 0, 0, 1];
/// *puns::from_bytes_mut::<[bool; 2]>(&mut bytes[2..]).unwrap() = [true, false];
/// assert_eq!(bytes, [0, 0, 1, 0]);
/// assert!(puns::from_bytes_mut::<[bool; 2]>(&mut [2, 0]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_bytes_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`try_pun_mut`].
#[inline]
pub const fn from_bytes_mut<D: ?Sized + Pun>(bytes: &mut [u8]) -> Result<&mut D, Error> {
  try_pun_mut::<[u8], D>(bytes)
}

// -------------------------------------------------------------------------------------------------
// Parsing

impl<D: ?Sized + Pun> Plan<[u8], D> {
  /// The `D` at the start of the `len` bytes at `addr`, and its size, with every check that
  /// could not be made at compile time (`from_head`, `from_head_mut`).
  ///
  /// # Safety
  /// `addr` must be valid for reads of `n` initialized bytes.
  #[inline(always)]
  const unsafe fn front(addr: *const u8, n: usize) -> Result<(*const D, usize), Error> {
    let (layout, rule) = const { (D::LAYOUT, D::LAYOUT.length_rule()) };
    let (head, stride) = (layout.head(), layout.stride());
    if n < head {
      return Err(Error::Size);
    }
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let len = match rule {
      _ if stride == 0 => 0,
      // As many elements as fit, trailing padding included.
      None => match n & !(layout.align() - 1) {
        room if room >= head => (room - head) / stride,
        _ => return Err(Error::Size),
      },
      Some(rule) => {
        // SAFETY: the sized fields of the target are in the first `head` bytes.
        let Some(value) = (unsafe { eval(rule.ops, addr.add(rule.base), 0) }) else {
          return Err(Error::Validity);
        };
        // From the size of the struct that has the rule, down to a number of elements.
        let before = (head - rule.base) as u128;
        let len = match () {
          () if rule.elements => value,
          () if value >= before && (value - before).is_multiple_of(stride as u128) => {
            (value - before) / stride as u128
          }
          () => return Err(Error::Validity),
        };
        if len > ((n - head) / stride) as u128 {
          return Err(Error::Size);
        }
        len as usize
      }
    };
    // With its trailing padding, if any.
    let size = match layout.size_of_val(len) {
      Some(size) if size <= n => size,
      _ => return Err(Error::Size),
    };
    let target = ptr::from_raw_parts::<D>(addr, len);
    // SAFETY: the target is in the bounds of the bytes, which are initialized.
    if !unsafe { validate::<Self, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok((target, size))
  }

  /// The `D` at the end of the `n` bytes at `addr`, and where it starts, with every check that
  /// could not be made at compile time (`from_tail`, `from_tail_mut`). `D` is sized.
  ///
  /// # Safety
  /// `addr` must be valid for reads of `n` initialized bytes.
  #[inline(always)]
  const unsafe fn back(addr: *const u8, n: usize) -> Result<(usize, *const D), Error> {
    let layout = const { D::LAYOUT };
    let Some(start) = n.checked_sub(layout.size()) else { return Err(Error::Size) };
    // SAFETY: `start <= n`.
    let addr = unsafe { addr.add(start) };
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let target = ptr::from_raw_parts::<D>(addr, 0);
    // SAFETY: the target is the last bytes, which are initialized.
    if !unsafe { validate::<Self, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok((start, target))
  }
}

/// Take a `D` from the start of `bytes`: returns it and what remains.
///
/// A `Sized` target takes its size. An unsized one takes what its `size` or `len` rule says
/// (see `#[derive(Pun)]`), or else as many elements of its slice as there are.
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(size = length)]
/// struct Chunk {
///   kind: u8,
///   length: u16be,
///   data: [u8],
/// }
///
/// let (chunk, rest) = puns::from_head::<Chunk>(&[7, 0, 5, b'h', b'i', 1, 2, 3]).unwrap();
/// assert_eq!((chunk.kind, &chunk.data, rest), (7, &b"hi"[..], &[1, 2, 3][..]));
/// ```
///
/// # Errors
/// [`Error::Alignment`] if `bytes` are not aligned for `D`, [`Error::Size`] if they are fewer
/// than a `D` takes, [`Error::Validity`] if its `size` or `len` rule cannot be computed or does
/// not give a size it can have, or if what it takes is not a valid `D`.
#[inline]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&D, &[u8]), Error> {
  const { Plan::<[u8], D>::TRY_REF };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::front(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: validated.
    Ok((target, size)) => Ok((unsafe { &*target }, bytes.split_at(size).1)),
    Err(error) => Err(error),
  }
}

/// Take a `D` from the end of `bytes`: returns what precedes it, and it.
///
/// The target is the last `size_of::<D>()` bytes, which are checked as [`from_bytes`] checks
/// them: their address if `D` has an alignment greater than 1, then what bytes do not prove of
/// `D`.
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// /// A trailer: the length of what precedes it, and a check.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Trailer {
///   length: u16be,
///   check: u8,
/// }
///
/// let (body, trailer) = puns::from_tail::<Trailer>(&[b'h', b'i', 0, 2, 0xff]).unwrap();
/// assert_eq!((body, trailer.length.get(), trailer.check), (&b"hi"[..], 2, 0xff));
/// assert_eq!(puns::from_tail::<Trailer>(&[0, 2]).err(), Some(puns::Error::Size));
/// ```
///
/// Where an unsized target starts cannot be found from the end of the bytes:
///
/// ```compile_fail
/// let _ = puns::from_tail::<[u16]>(&[0; 4]);
/// ```
///
/// # Errors
/// [`Error::Size`] if `bytes` are fewer than `size_of::<D>()`, [`Error::Alignment`] if the last
/// ones are not aligned for `D`, [`Error::Validity`] if they are not a valid `D`.
#[inline]
pub const fn from_tail<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&[u8], &D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::back(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: validated.
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  }
}

/// Take a `D` from the start of mutable `bytes`: returns it and what remains, both mutable.
///
/// As [`from_head`]; as anything can be written through the result, `D` must not have padding
/// (bytes would be de-initialized), as [`from_bytes_mut`] requires.
///
/// ```
/// let mut bytes = [1u8, 0, 7, 7];
/// let (flags, rest) = puns::from_head_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[1] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [1, 1, 9, 7]);
/// assert!(puns::from_head_mut::<[bool; 2]>(&mut [1, 2, 0]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_head_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`from_head`].
#[inline]
pub const fn from_head_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut D, &mut [u8]), Error> {
  const { Plan::<[u8], D>::TRY_MUT };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::front(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((target, size)) => {
      let len = ptr::metadata(target);
      let (front, rest) = bytes.split_at_mut(size);
      let target = ptr::from_raw_parts::<D>(front.as_mut_ptr().cast_const(), len).cast_mut();
      // SAFETY: the bytes of `front` were validated.
      Ok((unsafe { &mut *target }, rest))
    }
    Err(error) => Err(error),
  }
}

/// Take a `D` from the end of mutable `bytes`: returns what precedes it, and it, both mutable.
///
/// As [`from_tail`]; as anything can be written through the result, `D` must not have padding,
/// as [`from_bytes_mut`] requires.
///
/// ```
/// let mut bytes = [7u8, 7, 0, 1];
/// let (rest, flags) = puns::from_tail_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[0] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [9, 7, 1, 1]);
/// assert!(puns::from_tail_mut::<[bool; 2]>(&mut [1, 2]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_tail_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`from_tail`].
#[inline]
pub const fn from_tail_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut [u8], &mut D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL_MUT };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::back(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((start, _)) => {
      let (rest, back) = bytes.split_at_mut(start);
      let target = ptr::from_raw_parts::<D>(back.as_mut_ptr().cast_const(), 0).cast_mut();
      // SAFETY: the bytes of `back` were validated.
      Ok((rest, unsafe { &mut *target }))
    }
    Err(error) => Err(error),
  }
}

/// The bytes of the `index`th region of the tail of a struct, see `#[derive(Pun)]`.
///
/// Regions are not something that the type system knows of: if the fields of `value` tell of
/// regions that do not fit in its tail, they are cut short.
#[doc(hidden)]
#[inline(always)]
pub const fn region<S: ?Sized + Pun>(value: &S, index: usize) -> &[u8] {
  // The bytes of the value can be read: it has no padding.
  const { Plan::<S, [u8]>::BY_REF };
  let (head, rules): (usize, &[Rule]) = const { (S::LAYOUT.tail_shape().start, S::LAYOUT.rules()) };
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  let (_, tail, size) = validate::tail_of::<S>(value);
  // Where the region starts in the tail, and how many regions are before it.
  let (mut start, mut count) = (0, 0);
  macro_rules! rule {
    ($index:expr) => {
      if let Rule::Region(ops) = rules[$index] {
        let rest = tail - start;
        let size = match ops {
          // SAFETY: `base` is a struct of this layout.
          Some(ops) => match unsafe { eval(ops, base, size) } {
            Some(size) if size <= rest as u128 => size as usize,
            _ => rest,
          },
          None => rest,
        };
        if count == index {
          // SAFETY: `start + size <= tail`, the size of what follows the `head` first bytes.
          return unsafe { core::slice::from_raw_parts(base.add(head + start), size) };
        }
        (start, count) = (start + size, count + 1);
      }
    };
  }
  // Not a loop, for the first rules: see `layout::eval`.
  macro_rules! unrolled {
    ($($index:literal)+) => {$(
      if $index < rules.len() {
        rule!($index);
      }
    )+};
  }
  unrolled!(0 1 2 3 4 5 6 7);
  let mut i = 8;
  while i < rules.len() {
    rule!(i);
    i += 1;
  }
  panic!("no such region")
}
