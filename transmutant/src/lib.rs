//! ***Safe transmutation, checked at compile time. Even for dynamically sized types.***
//!
//! ```
//! use transmutant::Data;
//!
//! #[derive(Data, Debug, PartialEq)]
//! #[repr(C)]
//! struct Position {
//!   x: u8,
//!   visible: bool,
//! }
//!
//! // Every `Position` is two initialized bytes: nothing is checked, nothing is compiled.
//! assert_eq!(transmutant::transmute_ref::<_, [u8; 2]>(&Position { x: 7, visible: true }), &[7, 1]);
//!
//! // Not every pair of bytes is a `Position`: only the `bool` is checked.
//! assert_eq!(transmutant::try_transmute_ref(&[7u8, 1]), Ok(&Position { x: 7, visible: true }));
//! assert_eq!(transmutant::try_transmute_ref::<_, Position>(&[7u8, 2]), Err(transmutant::Error::Validity));
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
//! - What `#[derive(Data)]` is told: values that a field can take, sizes and lengths that are in
//!   a field, assertions over fields. See [`macro@Data`], and the examples.
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
//! # What deriving `Data` means
//!
//! A type is known by its layout and by what the type system enforces of its bytes, nothing more.
//! Two consequences are by design.
//!
//! `PhantomData<T>` is a scalar of no byte, for any `T`: types that differ only by phantom
//! parameters, or by lifetimes, convert to each other freely. A type whose parameters are a brand
//! (a unit, an invariant lifetime that ties a value to a scope, a marker of what a value was
//! checked against) should not derive `Data`:
//!
//! ```
//! use core::marker::PhantomData;
//!
//! #[derive(Clone, Copy)]
//! struct Meters;
//! #[derive(Clone, Copy)]
//! struct Feet;
//!
//! #[derive(transmutant::Data, Clone, Copy)]
//! #[repr(C)]
//! struct Length<U> {
//!   value: u32,
//!   unit: PhantomData<U>,
//! }
//!
//! // Not what a unit is for: 3 meters are 3 feet.
//! let feet: Length<Feet> = transmutant::transmute(Length::<Meters> { value: 3, unit: PhantomData });
//! assert_eq!(feet.value, 3);
//! ```
//!
//! Deriving `Data` makes every field as accessible as if it were public: a value can be made from
//! anything whose bytes are valid for its fields (and the rules the derive is told of), read as
//! anything its bytes are, and written through a `&mut` to another type. A type whose private
//! fields keep an invariant that the derive is not told of should not derive `Data`:
//!
//! ```
//! mod even {
//!   /// Only even numbers, says `new`.
//!   #[derive(transmutant::Data, Clone, Copy)]
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
//! // What `new` refuses, `transmute` makes.
//! let odd: even::Even = transmutant::transmute(3u32);
//! assert_eq!(odd.get(), 3);
//! ```
//!
//! # What does not compile
//!
//! Reading padding:
//!
//! ```compile_fail
//! let _: &[u8; 4] = transmutant::transmute_ref(&(0u8, 0u16));
//! ```
//!
//! Making a value that may not be valid, without a check:
//!
//! ```compile_fail
//! let _: &str = transmutant::transmute_ref(&[0u8; 4]);
//! ```
//!
//! A check that can never succeed:
//!
//! ```compile_fail
//! let _ = transmutant::try_transmute::<transmutant::num::u8c<2>, bool>(Default::default());
//! ```
//!
//! A `&mut` through which an invalid value could be written back:
//!
//! ```compile_fail
//! let _: &mut u8 = transmutant::transmute_mut(&mut true);
//! ```
//!
//! ```compile_fail
//! let mut text = String::from("text");
//! let _: &mut [u8] = transmutant::transmute_mut(text.as_mut_str());
//! ```
//!
//! Sizes or alignments that do not match:
//!
//! ```compile_fail
//! let _: &u32 = transmutant::transmute_ref(&[0u8; 4]);
//! ```
//!
//! # Enums with fields
//!
//! An enum is known by its tag: a variant proves the variant of the same tag, and nothing else
//! proves an enum.
//!
//! ```
//! use transmutant::Data;
//!
//! #[derive(Data, Debug, PartialEq, Clone, Copy)]
//! #[repr(u8)]
//! enum Shape {
//!   Circle { radius: u16 },
//!   Rect(u8, bool),
//! }
//!
//! #[derive(Data, Debug, PartialEq, Clone, Copy)]
//! #[repr(u8)]
//! enum Drawing {
//!   Circle { radius: u16 },
//!   Rect(u8, u8),
//!   Nothing,
//! }
//!
//! // Every `Shape` is a `Drawing`, variant by variant: nothing is checked.
//! assert_eq!(transmutant::transmute::<_, Drawing>(Shape::Rect(3, true)), Drawing::Rect(3, 1));
//! // Not every `Drawing` is a `Shape`.
//! assert_eq!(transmutant::try_transmute(Drawing::Rect(3, 1)), Ok(Shape::Rect(3, true)));
//! assert!(transmutant::try_transmute::<_, Shape>(Drawing::Rect(3, 2)).is_err());
//! assert!(transmutant::try_transmute::<_, Shape>(Drawing::Nothing).is_err());
//! // Bytes are a `Shape` if they start with one of its tags, then are what that variant holds.
//! assert_eq!(transmutant::try_transmute([1u8, 3, 1, 0]), Ok(Shape::Rect(3, true)));
//! assert!(transmutant::try_transmute::<_, Shape>([1u8, 3, 2, 0]).is_err());
//! assert!(transmutant::try_transmute::<_, Shape>([2u8, 0, 0, 0]).is_err());
//! ```
//!
//! A `Shape` is not 4 bytes: the byte after the tag of a `Circle` is padding, and nothing tells a
//! `Circle` from a `Rect` at compile time.
//!
//! ```compile_fail
//! # use transmutant::Data;
//! # #[derive(Data, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! let _: [u8; 4] = transmutant::transmute(Shape::Rect(3, true));
//! ```
//!
//! Neither can a `Drawing` be written where a `Shape` is:
//!
//! ```compile_fail
//! # use transmutant::Data;
//! # #[derive(Data, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! # #[derive(Data, Clone, Copy)] #[repr(u8)] enum Drawing { Circle { radius: u16 }, Rect(u8, u8), Nothing }
//! let mut shape = Shape::Rect(3, true);
//! let _: &mut Drawing = transmutant::transmute_mut(&mut shape);
//! ```
//!
//! The layout of an enum with fields that has no `repr` is not specified, and cannot be told on
//! stable Rust: `#[derive(Data)]` asks for one.
//!
//! ```compile_fail
//! #[derive(transmutant::Data)]
//! enum Unspecified { A(u8), B(u16) }
//! ```
//!
//! These are errors of the monomorphization of the functions: `cargo build` reports them,
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

use analysis::{Analysis, Exact, Marks, analyze, mutable};
use layout::{Dfa, Endian, Field, Layout, Rule, Spec, eval};
pub use layout::{crc32, sum16};
pub use transmutant_derive::Data;
use validate::{Checks, validate};

// -------------------------------------------------------------------------------------------------
// Traits

/// Leaf types: integers and the like, see [`Spec`].
///
/// This is what `#[derive(Data)]` implements for enums without fields.
///
/// # Safety
/// `SPEC` must describe every valid value of `Self`, and nothing more.
pub unsafe trait Scalar {
  const SPEC: Spec;
}

/// Types made of plain data, whose layout and valid values are known at compile time.
///
/// # Safety
/// Not meant to be implemented by hand: use `#[derive(Data)]`.
pub unsafe trait Data {
  /// Type of the elements of an array or of a slice, `()` otherwise.
  #[doc(hidden)]
  type Elem: Data;
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
/// This is implemented by `#[data(tag(K = T))]`, see [`macro@Data`]:
///
/// ```
/// use transmutant::num::{u8c, u16be};
/// use transmutant::{Data, Tag};
///
/// /// The kind of a message, when its payload is not known.
/// #[derive(Data, Copy, Clone)]
/// #[repr(transparent)]
/// struct Kind(u8);
///
/// #[derive(Data)]
/// #[repr(C)]
/// struct Message<P: ?Sized + Tag<Kind> = [u8]> {
///   kind: P::Tag,
///   payload: P,
/// }
///
/// #[derive(Data)]
/// #[repr(C)]
/// #[data(tag(Kind = u8c<7>))]
/// struct Ping {
///   sequence: u16be,
/// }
///
/// // A message of any kind, and a message that must be a ping.
/// assert_eq!(transmutant::try_transmute_ref::<_, Message>(&[9u8, 0, 1]).unwrap().kind.0, 9);
/// assert!(transmutant::try_transmute_ref::<_, Message<Ping>>(&[9u8, 0, 1]).is_err());
/// assert!(transmutant::try_transmute_ref::<_, Message<Ping>>(&[7u8, 0, 1]).is_ok());
/// ```
pub trait Tag<K: ?Sized>: Data {
  type Tag: Data + Copy;
}

/// Bytes are carried with any tag.
impl<K: Data + Copy> Tag<K> for [u8] {
  type Tag = K;
}

/// A field of a struct, and the ones after it: a list at the type level, for validation.
///
/// # Safety
/// Implemented by `#[derive(Data)]`.
#[doc(hidden)]
pub unsafe trait Fields {
  type Type: ?Sized + Data;
  type Next: Fields;
  /// Where the field is in the target, given where the struct is.
  type Path<P: Path>: Path;
  /// `None` at the end of the list.
  const FIELD: Option<Field>;
}

/// A variant of an enum with fields, and the ones after it: a list at the type level.
///
/// # Safety
/// Implemented by `#[derive(Data)]`.
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
/// Implemented by `#[derive(Data)]`.
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
unsafe impl<T: Scalar> Data for T {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::scalar::<T>(T::SPEC);
}

// SAFETY: an array is its elements.
unsafe impl<T: Data, const N: usize> Data for [T; N] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::array(&T::LAYOUT, N);
}

// SAFETY: a slice is its elements.
unsafe impl<T: Data> Data for [T] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = core::ptr::slice_from_raw_parts(core::ptr::null(), ptr::PROBE_LEN);
  // SAFETY: the probe of a slice.
  const LAYOUT: Layout = unsafe { Layout::slice::<T>(Self::PROBE, &T::LAYOUT) };
}

// SAFETY: a `str` is bytes that are UTF-8.
unsafe impl Data for str {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  const PROBE: *const Self = <[u8] as Data>::PROBE as *const str;
  // SAFETY: the probe of a `str`, which is the one of a slice.
  const LAYOUT: Layout = unsafe { Layout::pattern::<str>(Self::PROBE, Dfa::UTF8) };
}

// SAFETY: `ManuallyDrop<T>` is a `T`.
unsafe impl<T: ?Sized + Data> Data for ManuallyDrop<T> {
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
    unsafe impl<$($all: Data),+> Fields for TupleField<($($all,)+), $index> {
      type Type = $ty;
      type Next = $next;
      type Path<P: Path> = At<P, Self>;
      const FIELD: Option<Field> = Some(*<($($all,)+) as Data>::LAYOUT.field($index));
    }
  };
}

macro_rules! tuple {
  ([$($all:ident)+] $($index:tt)+) => {
    // SAFETY: the offsets come from the compiler.
    unsafe impl<$($all: Data),+> Data for ($($all,)+) {
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

impl_any!(() u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64);

// SAFETY: no byte.
unsafe impl<T: ?Sized> Scalar for PhantomData<T> {
  const SPEC: Spec = Spec::any::<Self>();
}

// SAFETY: 0 or 1.
unsafe impl Scalar for bool {
  const SPEC: Spec = Spec::ranges::<bool>(Endian::NATIVE, &[(0, 1)]);
}

// SAFETY: a Unicode scalar value.
unsafe impl Scalar for char {
  const SPEC: Spec = Spec::ranges::<char>(Endian::NATIVE, &[(0, 0xD7FF), (0xE000, 0x10_FFFF)]);
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

/// What is known at compile time of a transmutation from `S` to `D`.
struct Plan<S: ?Sized, D: ?Sized>(PhantomData<S>, PhantomData<D>);

/// What a function requires of a transmutation.
#[derive(Copy, Clone)]
struct Need {
  /// The target may be smaller than the source.
  truncate: bool,
  /// What is not proven is checked at run time.
  fallible: bool,
  /// The target is behind a reference.
  reference: bool,
}

const MUTABLE: &str = "some values of the target are not valid for the source (or are padding \
                       where the source is not), and could be written through the `&mut`";

impl<S: ?Sized + Data, D: ?Sized + Data> Plan<S, D> {
  const ANALYSIS: Analysis = analyze(&S::LAYOUT, &D::LAYOUT);

  const MUTABLE: bool = mutable(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit);

  /// Fail to compile if the transmutation does not meet the `need`.
  const fn require(need: Need) {
    if let Some(message) = Self::check(need) {
      message.panic();
    }
  }

  /// Why the transmutation does not meet the `need`, if it does not.
  const fn check(need: Need) -> Option<Message> {
    let Analysis { fit, relation, aligned } = Self::ANALYSIS;
    match fit.exact {
      _ if need.truncate && fit.shrink => {}
      _ if need.truncate => {
        return Some(Message::new(
          "the source can be smaller than the target (if it has a slice, it may be empty)",
        ));
      }
      Exact::Always => {}
      Exact::Sometimes if need.fallible => {}
      Exact::Sometimes => {
        return Some(Message::new(
          "the size of the source may not be a size of the target, depending on the length of \
           its slice.\nUse `try_transmute_ref` or `try_transmute_mut` to check it at run time",
        ));
      }
      Exact::Never(why) if fit.shrink => {
        return Some(
          Message::new(why)
            .str(".\nUse `truncate` or `truncate_ref` to ignore the end of the source"),
        );
      }
      Exact::Never(why) => return Some(Message::new(why)),
    }
    if let Some(offset) = relation.uninit {
      return Some(
        Message::new("a byte that the target requires to be initialized is padding in the source")
          .at(offset),
      );
    }
    if let Some(offset) = relation.disjoint {
      return Some(Message::new("no value of the source is valid for the target").at(offset));
    }
    if let (Some(offset), false) = (relation.partial, need.fallible) {
      return Some(
        Message::new(
          "some values of the source are not valid for the target, or do not prove one of its \
           rules.\nUse a `try_*` function to check them at run time",
        )
        .at(offset),
      );
    }
    if need.reference && !aligned && !need.fallible {
      return Some(Message::new(
        "the target has a greater alignment than the source.\nUse `try_transmute_ref` or \
         `try_transmute_mut` to check the address at run time",
      ));
    }
    None
  }

  const BY_VALUE: () = Self::require(Need { truncate: false, fallible: false, reference: false });
  const BY_REF: () = Self::require(Need { truncate: false, fallible: false, reference: true });
  const BY_MUT: () = {
    let () = Self::BY_REF;
    if !Self::MUTABLE {
      Message::new(MUTABLE).panic();
    }
  };
  const TRUNCATE: () = Self::require(Need { truncate: true, fallible: false, reference: false });
  const TRUNCATE_REF: () = Self::require(Need { truncate: true, fallible: false, reference: true });
  const TRY: () = Self::require(Need { truncate: false, fallible: true, reference: false });
  const TRY_REF: () = Self::require(Need { truncate: false, fallible: true, reference: true });
  const TRY_MUT: () = {
    let () = Self::TRY_REF;
    if !Self::MUTABLE {
      Message::new(MUTABLE).panic();
    }
  };

  /// Pointer to the target at `source`, when there is nothing to check (`BY_REF`,
  /// `TRUNCATE_REF`).
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

impl<S: ?Sized + Data, D: ?Sized + Data> Checks for Plan<S, D> {
  const MARKS: Marks = Self::ANALYSIS.relation.marks;
}

/// A compile-time error message, without `format!`.
struct Message {
  bytes: [u8; 512],
  len: usize,
}

impl Message {
  const fn new(what: &str) -> Self {
    Self { bytes: [0; 512], len: 0 }.str("\ncannot transmute: ").str(what)
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
/// assert_eq!(transmutant::transmute::<[bool; 2], u16>([true, true]), 0x0101);
/// ```
///
/// ```compile_fail
/// // Not every `u8` is a `bool`.
/// let _: bool = transmutant::transmute(2u8);
/// ```
#[inline(always)]
pub const fn transmute<S: Data + Copy, D: Data + Copy>(value: S) -> D {
  const { Plan::<S, D>::BY_VALUE };
  // SAFETY: checked at compile time.
  unsafe { transmute_unchecked(value) }
}

/// Reinterpret the first bytes of a value as a value of a smaller type.
///
/// ```
/// assert_eq!(transmutant::truncate::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [1, 2]);
/// ```
#[inline(always)]
pub const fn truncate<S: Data + Copy, D: Data + Copy>(value: S) -> D {
  const { Plan::<S, D>::TRUNCATE };
  // SAFETY: checked at compile time.
  unsafe { transmute_unchecked(value) }
}

/// Reinterpret a value as a value of another type of the same size, if it is a valid one.
///
/// Only what could not be proven at compile time is checked.
///
/// ```
/// assert_eq!(transmutant::try_transmute::<u8, bool>(1), Ok(true));
/// assert_eq!(transmutant::try_transmute::<u8, bool>(2), Err(transmutant::Error::Validity));
/// ```
///
/// # Errors
/// [`Error::Validity`] if `value` is not a valid `D`.
#[inline]
pub const fn try_transmute<S: Data + Copy, D: Data + Copy>(value: S) -> Result<D, Error> {
  const { Plan::<S, D>::TRY };
  // SAFETY: `value` is a valid `S` of the size of `D`; nothing is assumed of its alignment.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Err(Error::Validity);
  }
  // SAFETY: just validated.
  Ok(unsafe { transmute_unchecked(value) })
}

/// # Safety
/// `D` must not be larger than `S`, and the first bytes of `value` must be a valid `D`.
#[inline(always)]
const unsafe fn transmute_unchecked<S: Copy, D: Copy>(value: S) -> D {
  union Transmute<S: Copy, D: Copy> {
    source: S,
    target: D,
  }
  // SAFETY: guaranteed by the caller.
  unsafe { Transmute { source: value }.target }
}

// -------------------------------------------------------------------------------------------------
// By reference

/// Reinterpret a reference as a reference to another type.
///
/// This compiles if every value of `S` is a valid `D` of the same size, whatever the length of
/// its slice if it has one, and if `D` has no greater alignment. It then costs nothing.
///
/// ```
/// let bytes: &[u8] = transmutant::transmute_ref(&[0x0102u16, 0x0304][..]);
/// assert_eq!(bytes.len(), 4);
/// let text: &[u8] = transmutant::transmute_ref("héllo");
/// assert_eq!(text.len(), 6);
/// ```
#[inline(always)]
pub const fn transmute_ref<S: ?Sized + Data, D: ?Sized + Data>(value: &S) -> &D {
  const { Plan::<S, D>::BY_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a mutable reference as a mutable reference to another type.
///
/// On top of what [`transmute_ref`] requires, every value of `D` must be a valid `S`: anything
/// can be written through the result.
///
/// ```
/// let mut value = 0x0102u16;
/// transmutant::transmute_mut::<_, [u8; 2]>(&mut value)[0] = 0x02;
/// assert!(value == 0x0202 || value == 0x0102);
/// ```
#[inline(always)]
pub const fn transmute_mut<S: ?Sized + Data, D: ?Sized + Data>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::BY_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterpret a reference as a reference to a type that covers the start of the value.
///
/// If the target has a slice, it takes as many elements as fit.
///
/// ```
/// let head: &[u8; 2] = transmutant::truncate_ref(&[1u8, 2, 3]);
/// assert_eq!(head, &[1, 2]);
/// let pairs: &[[u8; 2]] = transmutant::truncate_ref(&[1u8, 2, 3, 4, 5]);
/// assert_eq!(pairs, &[[1, 2], [3, 4]]);
/// ```
#[inline(always)]
pub const fn truncate_ref<S: ?Sized + Data, D: ?Sized + Data>(value: &S) -> &D {
  const { Plan::<S, D>::TRUNCATE_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a reference as a reference to another type, if the value is a valid one.
///
/// Only what could not be proven at compile time is checked: the size if it depends on a slice
/// length, the address if the target has a greater alignment, and the scalars, strings and rules
/// of the target that the source does not guarantee.
///
/// ```
/// assert_eq!(transmutant::try_transmute_ref::<[u8], str>(b"caf\xc3\xa9"), Ok("café"));
/// assert!(transmutant::try_transmute_ref::<[u8], str>(b"caf\xc3").is_err());
/// ```
///
/// This is a `const fn`, but a target of greater alignment than the source cannot be made at
/// compile time, where addresses are not known.
///
/// # Errors
/// [`Error::Alignment`] if the address of `value` is not aligned for `D`, [`Error::Size`] if its
/// size is not one that `D` can have, [`Error::Validity`] if it is not a valid `D`.
#[inline]
pub const fn try_transmute_ref<S: ?Sized + Data, D: ?Sized + Data>(value: &S) -> Result<&D, Error> {
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
/// See [`try_transmute_ref`] and [`transmute_mut`].
///
/// # Errors
/// As [`try_transmute_ref`].
#[inline]
pub const fn try_transmute_mut<S: ?Sized + Data, D: ?Sized + Data>(
  value: &mut S,
) -> Result<&mut D, Error> {
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
/// assert_eq!(transmutant::zeroed::<(u32, [bool; 2])>(), (0, [false; 2]));
/// ```
///
/// ```compile_fail
/// let _ = transmutant::zeroed::<core::num::NonZero<u8>>();
/// ```
#[inline(always)]
#[must_use]
pub const fn zeroed<T: Data + Copy>() -> T {
  const { assert!(T::LAYOUT.zeroable(), "zero is not a valid value of this type") };
  // SAFETY: checked at compile time.
  unsafe { core::mem::zeroed() }
}

/// The value of an integer field, as `#[data(get(..))]` expressions see it.
#[doc(hidden)]
#[inline(always)]
pub const fn int<T: Data>(value: &T) -> u128 {
  let spec = const {
    match T::LAYOUT.as_integer() {
      Some(spec) => spec,
      None => panic!("not an integer field"),
    }
  };
  // SAFETY: a `T` is that integer, which is not allowed to be uninitialized.
  unsafe { spec.load(core::ptr::from_ref::<T>(value).cast()) }
}

/// The bytes of a value that has no padding.
#[inline(always)]
pub const fn as_bytes<S: ?Sized + Data>(value: &S) -> &[u8] {
  transmute_ref(value)
}

// -------------------------------------------------------------------------------------------------
// Parsing

/// The body of [`unpack`], after its compile-time check: a macro rather than a function, so that
/// `hook::unpack` shares it without changing the code of `unpack`.
macro_rules! unpack {
  ($d:ident, $bytes:ident) => {{
    let (layout, rule) = const { ($d::LAYOUT, $d::LAYOUT.length_rule()) };
    let (head, stride) = (layout.head(), layout.stride());
    if $bytes.len() < head {
      return Err(Error::Size);
    }
    if const { $d::LAYOUT.align() > 1 } && !ptr::is_aligned($bytes.as_ptr(), layout.align()) {
      return Err(Error::Alignment);
    }
    let len = match rule {
      _ if stride == 0 => 0,
      // As many elements as fit, trailing padding included.
      None => match $bytes.len() & !(layout.align() - 1) {
        room if room >= head => (room - head) / stride,
        _ => return Err(Error::Size),
      },
      Some(rule) => {
        // SAFETY: the sized fields of the target are in the first `head` bytes.
        let Some(value) = (unsafe { eval(rule.ops, $bytes.as_ptr().add(rule.base), 0) }) else {
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
        if len > (($bytes.len() - head) / stride) as u128 {
          return Err(Error::Size);
        }
        len as usize
      }
    };
    // With its trailing padding, if any.
    let size = match layout.size_of_val(len) {
      Some(size) if size <= $bytes.len() => size,
      _ => return Err(Error::Size),
    };
    let target = ptr::from_raw_parts::<$d>($bytes.as_ptr(), len);
    // SAFETY: the target is in the bounds of `$bytes`, which are initialized.
    if !unsafe { validate::<Plan<[u8], $d>, Root, $d>(target) } {
      return Err(Error::Validity);
    }
    // SAFETY: validated.
    Ok((unsafe { &*target }, $bytes.split_at(size).1))
  }};
}

/// Take a `D` from the start of `bytes`: returns it and what remains.
///
/// A `Sized` target takes its size. An unsized one takes what its `size` or `len` rule says
/// (see `#[derive(Data)]`), or else as many elements of its slice as there are.
///
/// ```
/// use transmutant::Data;
/// use transmutant::num::u16be;
///
/// #[derive(Data)]
/// #[repr(C)]
/// #[data(size = length)]
/// struct Chunk {
///   kind: u8,
///   length: u16be,
///   data: [u8],
/// }
///
/// let (chunk, rest) = transmutant::unpack::<Chunk>(&[7, 0, 5, b'h', b'i', 1, 2, 3]).unwrap();
/// assert_eq!((chunk.kind, &chunk.data, rest), (7, &b"hi"[..], &[1, 2, 3][..]));
/// ```
///
/// # Errors
/// [`Error::Alignment`] if `bytes` are not aligned for `D`, [`Error::Size`] if they are fewer
/// than a `D` takes, [`Error::Validity`] if its `size` or `len` rule cannot be computed or does
/// not give a size it can have, or if what it takes is not a valid `D`.
#[inline]
pub const fn unpack<D: ?Sized + Data>(bytes: &[u8]) -> Result<(&D, &[u8]), Error> {
  const { Plan::<[u8], D>::TRY_REF };
  unpack!(D, bytes)
}

/// [`unpack`], without the compile-time check that `Plan::<[u8], D>::TRY_REF` holds, which the
/// caller must make (`hook::unpack`).
#[cfg(feature = "verdict")]
const fn unpack_unchecked<D: ?Sized + Data>(bytes: &[u8]) -> Result<(&D, &[u8]), Error> {
  unpack!(D, bytes)
}

/// The bytes of the `index`th region of the tail of a struct, see `#[derive(Data)]`.
///
/// Regions are not something that the type system knows of: if the fields of `value` tell of
/// regions that do not fit in its tail, they are cut short.
#[doc(hidden)]
#[inline(always)]
pub const fn region<S: ?Sized + Data>(value: &S, index: usize) -> &[u8] {
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
