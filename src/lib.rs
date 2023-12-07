#![feature(ptr_metadata)]
#![feature(offset_of)]
use core::any::TypeId;
use core::fmt::Debug;
use core::mem::{self, align_of, size_of};
use core::fmt::Formatter;
use core::ptr::Pointee;

mod unalign;
mod unaligned;
mod undrop;

pub use unalign::*;
pub use unaligned::*;
pub use undrop::*;

pub unsafe trait Mutant: Pointee + 'static {
  type Head: Copy + Mutant<Tail = ()>;
  type Tail: ?Sized + Mutant<Metadata = Self::Metadata>;

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata>;
}

pub unsafe trait Super<'a>: Sized {
  type Tag: Mutant<Head = Self::Tag, Tail = ()>;
}

pub unsafe trait State<'a, T: Super<'a>>: Mutant
where
  &'a Self: TryFrom<T>,
  &'a Self: Into<T>,
{
  const TAG: T::Tag;

  fn upcast(&'a self) -> T;
}

// fn ----------------------------------------------------------------------------------------------

pub fn head_of<T: ?Sized + Mutant>(value: &T) -> &T::Head {
  unsafe { &*(value as *const T as *const T::Head) }
}

pub fn tail_of<T: ?Sized + Mutant>(value: &T) -> &T::Tail {
  let addr = unsafe { (value as *const T as *const ()).add(size_of::<T::Head>()) };
  let meta = core::ptr::metadata(value);
  unsafe { &*core::ptr::from_raw_parts(addr, meta) }
}

pub fn validate<T: ?Sized + Mutant>(bytes: &[u8]) -> Option<T::Metadata> {
  union PtrRepr<T: ?Sized + Mutant> {
    bytes: *const [u8],
    value: *const T,
    head: *const T::Head,
  }

  let ptr = PtrRepr::<T> { bytes };

  // Fast path for bytes.
  if TypeId::of::<T>() == TypeId::of::<[u8]>() {
    return Some(core::ptr::metadata(unsafe { ptr.value }));
  }

  // Primitive types.
  if TypeId::of::<T>() == TypeId::of::<T::Head>() {
    if align_of::<T::Head>() != 1 && bytes.as_ptr().align_offset(align_of::<T::Head>()) != 0 {
      return None;
    }

    return if size_of::<T::Head>() <= bytes.len() {
      let (_, tail) = bytes.split_at(size_of::<T::Head>());
      T::validate(unsafe { &*ptr.head }, tail)?;
      Some(core::ptr::metadata(unsafe { ptr.value }))
    } else {
      None
    };
  }

  let (head, tail) = unpack::<T::Head>(bytes)?;
  T::validate(head, tail)
}

pub fn unpack<T: ?Sized + Mutant>(bytes: &[u8]) -> Option<(&T, &[u8])> {
  union PtrRepr<T: ?Sized + Mutant> {
    bytes: *const [u8],
    value: *const T,
    head: *const T::Head,
    tail: *const T::Tail,
  }

  let ptr = PtrRepr::<T> { bytes };

  // Fast path for bytes.
  if TypeId::of::<T>() == TypeId::of::<[u8]>() {
    return Some((unsafe { &*ptr.value }, &[]));
  }

  // Primitive types.
  if TypeId::of::<T>() == TypeId::of::<T::Head>() {

    dbg!(align_of::<T::Head>());
    dbg!(mem::align_of_val(unsafe { &*ptr.head }));
    dbg!(bytes.as_ptr().align_offset(align_of::<T::Head>()));

    if align_of::<T::Head>() != 1 && bytes.as_ptr().align_offset(align_of::<T::Head>()) != 0 {
      return None;
    }

    return if size_of::<T::Head>() <= bytes.len() {
      let (_, tail) = bytes.split_at(size_of::<T::Head>());
      T::validate(unsafe { &*ptr.head }, tail)?;
      Some((unsafe { &*ptr.value }, tail))
    } else {
      None
    };
  }

  let (head, tail) = unpack::<T::Head>(bytes)?;
  let meta = T::validate(head, tail)?;

  let value_ptr = core::ptr::from_raw_parts(bytes.as_ptr() as *const (), meta);
  let (bytes, rest) = {
    // Fake a value of type `T` and metadata `meta` to retrieve the size of `T`.
    // SAFETY: `mem::size_of_val` never actually deference `val` address.
    let size = mem::size_of_val(unsafe { &*value_ptr });
    if size > bytes.len() {
      return None;
    }
    bytes.split_at(size)
  };

  if TypeId::of::<T>() != TypeId::of::<T::Tail>() {
    let (tail, _) = unpack::<T::Tail>(&bytes[size_of::<T::Head>()..])?;
    assert_eq!(mem::size_of_val(head) + mem::size_of_val(tail), bytes.len());
  } else {
    assert_eq!(mem::size_of_val(head), 0);
  }

  Some(( unsafe { &*value_ptr }, rest))
}

// impl Mutant -------------------------------------------------------------------------------------

unsafe impl<T: Mutant<Tail = ()>, const N: usize> Mutant for [T; N] {
  type Head = [T::Head; N];
  type Tail = ();

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata> {
    for x in head {
      // let _ = T::validate(x)?;
    }
    Some(())
  }
}

unsafe impl<T: Mutant> Mutant for [T] {
  type Head = ();
  type Tail = [T];

  #[inline(always)]
  fn validate(_head: &Self::Head, mut tail: &[u8]) -> Option<Self::Metadata> {
    if size_of::<T>() == 0 {
      Some(0)
    } else {
      let len = tail.len() / size_of::<T>();
      for _ in 0..len {
        (_, tail) = unpack::<T>(tail)?;
      }
      Some(len)
    }
  }
}

unsafe impl Mutant for () {
  type Head = Self;
  type Tail = Self;

  #[inline]
  fn validate(_head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for u8 {
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for u16 {
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for u32 {
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for bool {
  type Head = u8;
  type Tail = ();

  #[inline]
  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    match *head {
      0 | 1 => Some(()),
      _ => None,
    }
  }
}

unsafe impl Mutant for char {
  type Head = u32;
  type Tail = ();

  #[inline]
  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    char::from_u32(*head).map(|_| ())
  }
}

// Tests -------------------------------------------------------------------------------------------

#[repr(C, u8)]
#[derive(Debug)]
enum X<'a> {
  A(&'a A) = 0,
  B(&'a B) = 1,
  C(&'a C) = 2,
}

#[repr(C, packed)]
#[derive(Debug)]
struct A(u8);

#[repr(C, packed)]
#[derive(Debug)]
struct B(u8, u8);

#[repr(C, packed)]
struct C(u8, Dst<[u8]>);

unsafe impl<'a> Super<'a> for X<'a> {
  type Tag = u8;
}

impl<'a> Into<X<'a>> for &'a A {
  fn into(self) -> X<'a> {
    X::A(self)
  }
}

impl<'a> TryFrom<X<'a>> for &'a A {
  type Error = ();

  fn try_from(value: X<'a>) -> Result<Self, Self::Error> {
    match value {
      X::A(value) => Ok(value),
      _ => Err(()),
    }
  }
}

unsafe impl<'a> State<'a, X<'a>> for A {
  const TAG: u8 = 0;

  fn upcast(&'a self) -> X<'a> {
    X::A(self)
  }
}

unsafe impl Mutant for A {
  type Head = u8;
  type Tail = ();

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    if *head == Self::TAG {
      Some(())
    } else {
      None
    }
  }
}

impl<'a> Into<X<'a>> for &'a B {
  fn into(self) -> X<'a> {
    X::B(self)
  }
}

impl<'a> TryFrom<X<'a>> for &'a B {
  type Error = ();

  fn try_from(value: X<'a>) -> Result<Self, Self::Error> {
    match value {
      X::B(value) => Ok(value),
      _ => Err(()),
    }
  }
}

unsafe impl<'a> State<'a, X<'a>> for B {
  const TAG: u8 = 1;

  fn upcast(&'a self) -> X<'a> {
    X::B(self)
  }
}

unsafe impl Mutant for B {
  type Head = [u8; 2];
  type Tail = ();

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    if head[0] == Self::TAG {
      Some(())
    } else {
      None
    }
  }
}

impl<'a> Into<X<'a>> for &'a C {
  fn into(self) -> X<'a> {
    X::C(self)
  }
}

impl<'a> TryFrom<X<'a>> for &'a C {
  type Error = ();

  fn try_from(value: X<'a>) -> Result<Self, Self::Error> {
    match value {
      X::C(value) => Ok(value),
      _ => Err(()),
    }
  }
}

unsafe impl<'a> State<'a, X<'a>> for C {
  const TAG: u8 = 2;

  fn upcast(&'a self) -> X<'a> {
    X::C(self)
  }
}

unsafe impl Mutant for C {
  type Head = u8;
  type Tail = Dst<[u8]>;

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata> {
    if *head == Self::TAG {
      validate::<Self::Tail>(tail)
    } else {
      None
    }
  }
}

impl Debug for C {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("C")
      .field("0", &self.0)
      .field("1", &&self.1)
      .finish()
  }
}

fn unpack_x(bytes: &[u8]) -> Option<(X<'_>, &[u8])> {
  match *unpack::<<X as Super>::Tag>(bytes)?.0 {
    A::TAG => unpack(bytes).map(|(v, r)| (X::A(v), r)),
    B::TAG => unpack(bytes).map(|(v, r)| (X::B(v), r)),
    C::TAG => unpack(bytes).map(|(v, r)| (X::C(v), r)),
    _ => None,
  }
}

// unsafe impl Mutant for X {
//   type Head = u8;
//   type Tail = [u8];
//
//   fn validate(head: &Self::Head) -> Option<Self::Metadata> {
//     match *head {
//       0 => Ok(0),
//       1 => Ok(1),
//       _ => Err("invalid tag")
//     }
//   }
// }

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct Dst<T: ?Sized> {
  len: u8,
  payload: Undrop<T>,
}

impl<T: ?Sized + Debug + Unaligned> Debug for Dst<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Dst")
      .field("len", &self.len)
      .field("payload", &read_aligned!(self.payload))
      .finish()
  }
}

unsafe impl<T: ?Sized> Unaligned for Dst<T> { }

unsafe impl Mutant for Dst<()> {
  type Head = Self;
  type Tail = ();

  fn validate(_head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl<T: ?Sized + Mutant> Mutant for Dst<T>
where
  Self: Pointee<Metadata = usize>,
  T: Pointee<Metadata = usize>,
{
  type Head = Dst<()>;
  type Tail = T;

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(head.len as usize)
  }
}

trait CommandParams: Mutant {
  const OPCODE: u16;
}

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, PartialEq)]
struct Command<T: ?Sized> {
  opcode: u16,
  parameters_len: u8,
  parameters: Undrop<T>,
}

impl<T: CommandParams> Command<T> {
  fn new(parameters: T) -> Self {
    Self {
      opcode: T::OPCODE,
      parameters_len: mem::size_of_val(&parameters) as u8,
      parameters: Undrop::new(parameters),
    }
  }
}

impl<T: ?Sized + Unaligned + Debug> Debug for Command<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Command")
      .field("opcode", &{ self.opcode })
      .field("parameters_len", &{ self.parameters_len })
      .field("parameters", &read_aligned!(self.parameters))
      .finish()
  }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct Reset;

unsafe impl Unaligned for Reset {}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct SetEventMask {
  mask: u64
}

unsafe impl Unaligned for SetEventMask {}

enum Commands<'a> {
  Reset(&'a Command<Reset>),
  SetEventMask(&'a Command<SetEventMask>),
  Unknown(&'a Command<[u8]>),
}

unsafe impl Mutant for Command<()> {
  type Head = Self;
  type Tail = ();

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for Command<[u8]> {
  type Head = Command<()>;
  type Tail = [u8];

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    Some(head.parameters_len as usize)
  }
}

unsafe impl Mutant for Command<Reset> {
  type Head = Command<()>;
  type Tail = Reset;

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    if head.opcode == 0x42 {
      Some(())
    } else {
      None
    }
  }
}

unsafe impl Mutant for Command<SetEventMask> {
  type Head = Command<()>;
  type Tail = SetEventMask;

  fn validate(head: &Self::Head, _tail: &[u8]) -> Option<Self::Metadata> {
    if head.opcode == 0x43 {
      Some(())
    } else {
      None
    }
  }
}

unsafe impl Mutant for Reset {
  type Head = Self;
  type Tail = ();

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

unsafe impl Mutant for SetEventMask {
  type Head = Self;
  type Tail = ();

  fn validate(head: &Self::Head, tail: &[u8]) -> Option<Self::Metadata> {
    Some(())
  }
}

#[test]
fn test() {
  println!("{:?}", unpack::<u32>(&[1, 0, 0, 0]));
  println!("{:?}", unpack::<[u8]>(&[0, 1, 2, 3, 4, 5]));
  println!("{:?}", unpack::<[bool]>(&[0, 1, 0, 1]));
  println!("{:?}", unpack::<[Unalign<u16>]>(&[0, 1, 0, 2]));
  println!("{:?}", unpack::<u8>(&[0]));
  println!("{:?}", unpack::<bool>(&[0]));
  println!("{:?}", unpack::<bool>(&[1]));
  println!("{:?}", unpack::<bool>(&[2]));
  println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
  println!("{:?}", unpack::<Dst<[u8]>>(&[3, 1, 2, 3, 4]));

  println!("{:?}", unpack::<A>(&[0]));
  println!("{:?}", unpack::<B>(&[1, 69]));
  println!("{:?}", unpack::<C>(&[2, 3, 1, 2, 3]));

  println!("{:?}", unpack_x(&[0]));
  println!("{:?}", unpack_x(&[1, 69]));
  println!("{:?}", unpack_x(&[2, 3, 1, 2, 3]));

  println!("{:?}", unpack::<Command<[u8]>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
  println!("{:?}", unpack::<Command<SetEventMask>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));

  println!("{:?}", unpack::<Command<[u8]>>(&[0x42, 0, 0]));
  println!("{:?}", unpack::<Command<Reset>>(&[0x42, 0, 0]));
}
