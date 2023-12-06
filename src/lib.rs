use core::any::TypeId;
use core::convert::Infallible;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use mem::{align_of, size_of};

mod unalign;
pub use unalign::*;

#[derive(Debug)]
pub enum Error<T: Mutant + ?Sized> {
  SizeMismatch,
  AlignmentMismatch,
  InvalidBytesPattern(T::Error),
}

trait InvalidBytesPattern {
  fn into_str(self) -> &'static str;
  fn from_str(details: &'static str) -> Self;
}

trait TailSize: Copy {
  fn trailing_bytes(self) -> usize;
}

pub trait Tail {
  type Size: TailSize;
}

pub unsafe trait Mutant: 'static {
  type Error: InvalidBytesPattern;
  type Head: Mutant<Tail = ()>;
  type Tail: Tail + ?Sized;

  fn validate(head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error>;
}

// -------------------------------------------------------------------------------------------------

impl TailSize for () {
  #[inline(always)]
  fn trailing_bytes(self) -> usize {
    0
  }
}

impl TailSize for usize {
  #[inline(always)]
  fn trailing_bytes(self) -> usize {
    self
  }
}

impl Tail for () {
  type Size = ();
}

impl<T: Mutant<Tail = ()>> Tail for [T] {
  type Size = usize;
}

impl<T: Mutant + ?Sized> Error<T> {
  pub fn upcast<U: Mutant<Head = T> + ?Sized>(self) -> Error<U> {
    match self {
      Error::InvalidBytesPattern(details) => Error::InvalidBytesPattern(U::Error::from_str(details.into_str())),
      Error::SizeMismatch => Error::SizeMismatch,
      Error::AlignmentMismatch => Error::AlignmentMismatch,
    }
  }
}

impl InvalidBytesPattern for Infallible {
  #[inline(always)]
  fn into_str(self) -> &'static str {
    unreachable!()
  }

  #[inline(always)]
  fn from_str(details: &'static str) -> Self {
    todo!()
  }
}

impl InvalidBytesPattern for &'static str {
  #[inline(always)]
  fn into_str(self) -> &'static str {
    self
  }

  #[inline(always)]
  fn from_str(_details: &'static str) -> Self {
    unreachable!()
  }
}

// -------------------------------------------------------------------------------------------------

struct Check<T: Mutant>(PhantomData<T>);

impl<T: Mutant> Check<T>
where
  T::Head: Sized,
{
  const HEAD_ALIGNMENT: () = assert!(align_of::<T::Head>() >= align_of::<T>());
  const HEAD_SIZE: () = assert!(size_of::<T::Head>() <= size_of::<T>());
}

// trait Flags {
//   const SIZED: bool;
//   const SIZE_OF: usize;
// }
//
// impl<T: Mutant> Flags for T {
//   const SIZED: bool = true;
//   const SIZE_OF: usize = size_of::<T>();
// }
//
// impl<T: Mutant + ?Sized> Flags for T {
//   const SIZED: bool = false;
//   const SIZE_OF: usize = 0;
// }

// -------------------------------------------------------------------------------------------------

#[inline(always)]
fn unpack<T: Mutant>(bytes: &[u8]) -> Result<&T, Error<T>> {
  unpack_prefix(bytes)
    .and_then(|(value, rest)| if rest.is_empty() { Ok(value) } else { Err(Error::SizeMismatch) })
}

#[inline(always)]
fn unpack_mut<T: Mutant<Tail = ()>>(bytes: &mut [u8]) -> Result<&mut T, Error<T>> {
  #[allow(mutable_transmutes)]
  Ok(unsafe { mem::transmute(unpack::<T>(bytes)?) })
}

#[inline(always)]
fn unpack_prefix<T: Mutant>(bytes: &[u8]) -> Result<(&T, &[u8]), Error<T>> {
  let _ = Check::<T>::HEAD_ALIGNMENT;
  let _ = Check::<T>::HEAD_SIZE;

  if TypeId::of::<T>() == TypeId::of::<T::Head>() {
    if align_of::<T>() != 1 && bytes.as_ptr().align_offset(align_of::<T>()) != 0 {
      return Err(Error::AlignmentMismatch);
    }

    return if size_of::<T>() <= bytes.len() {
      let (bytes, rest) = bytes.split_at(size_of::<T>());
      T::validate(unsafe { &*(bytes.as_ptr() as *const T::Head) }).map_err(Error::InvalidBytesPattern)?;
      Ok((unsafe { &*(bytes.as_ptr() as *const T) }, rest))
    } else {
      Err(Error::SizeMismatch)
    };
  }

  let head = unpack_prefix::<T::Head>(bytes).map_err(Error::upcast)?.0;
  let tail = T::validate(head).map_err(Error::InvalidBytesPattern)?;
  let size = size_of::<T::Head>() + tail.trailing_bytes();

  if tail.trailing_bytes() != 0 && size > bytes.len() {
    return Err(Error::SizeMismatch);
  }

  let (bytes, rest) = bytes.split_at(size);
  Ok((unsafe { &*(bytes.as_ptr() as *const T) }, rest))
}

#[inline(always)]
fn unpack_unsize_prefix<T: Debug + Mutant<Tail = [U]> + ?Sized, U: Mutant<Tail = ()>>(bytes: &[u8]) -> Result<(&T, &[u8]), Error<T>> where T::Head: Debug, <U as Mutant>::Head: Debug {
  let head = unpack_prefix::<T::Head>(bytes).map_err(Error::upcast)?.0;
  let tail = T::validate(head).map_err(Error::InvalidBytesPattern)?.trailing_bytes();
  let size = size_of::<T::Head>() + tail;

  if tail != 0 && size > bytes.len() {
    return Err(Error::SizeMismatch);
  }

  let (mut bytes, rest) = bytes.split_at(size);
  unsafe {
    let x = mem::transmute::<_, &mut (usize, usize)>(&mut bytes);
    x.1 = tail;
  }

  struct Value<H, T> {
    head: H,
    tail: [T],
  }

  union Ptr<T: ?Sized, H, U> {
    bytes: *const [u8],
    val: *const Value<H, U>,
    ptr: *const T,
  }

  let ptr = Ptr::<T, T::Head, U::Head> { bytes };
  for x in &unsafe { &*(ptr.val) }.tail {
    dbg!(x);
    // U::validate(x)
    //   .map_err(|x: U::Error| Error::<U>::InvalidBytesPattern(x).upcast())
    //   .map_err(|x: Error<T::Head>| x.upcast())?;
  }
  Ok((unsafe { &*(ptr.ptr) }, rest))
}

#[inline(always)]
fn unpack_prefix_mut<T: Mutant<Tail = ()>>(
  bytes: &mut [u8],
) -> Result<(&mut T, &mut [u8]), Error<T>> {
  #[allow(mutable_transmutes)]
  Ok(unsafe { mem::transmute(unpack_prefix::<T>(bytes)?) })
}

// -------------------------------------------------------------------------------------------------

unsafe impl Mutant for () {
  type Error = Infallible;
  type Head = ();
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(())
  }
}

unsafe impl Mutant for u8 {
  type Error = Infallible;
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(())
  }
}

unsafe impl Mutant for u16 {
  type Error = Infallible;
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(())
  }
}

unsafe impl Mutant for u32 {
  type Error = Infallible;
  type Head = Self;
  type Tail = ();

  #[inline]
  fn validate(_head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(())
  }
}

unsafe impl Mutant for bool {
  type Error = &'static str;
  type Head = u8;
  type Tail = ();

  #[inline]
  fn validate(head: &Self::Head) -> Result<(), Self::Error> {
    match *head {
      0 | 1 => Ok(()),
      _ => Err("expected 0 or 1"),
    }
  }
}

unsafe impl Mutant for char {
  type Error = &'static str;
  type Head = u32;
  type Tail = ();

  #[inline]
  fn validate(head: &Self::Head) -> Result<(), Self::Error> {
    if char::from_u32(*head).is_some() {
      Ok(())
    } else {
      Err("zero value")
    }
  }
}

// -------------------------------------------------------------------------------------------------

#[repr(C, u8)]
#[derive(Debug)]
enum X {
  A = 0,
  B(u8) = 1,
}

unsafe impl Mutant for X {
  type Error = &'static str;
  type Head = u8;
  type Tail = [u8];

  fn validate(head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    match *head {
      0 => Ok(0),
      1 => Ok(1),
      _ => Err("invalid tag")
    }
  }
}

#[derive(Debug)]
#[repr(C, packed)]
struct Dst {
  len: u8,
  data: u8,
  payload: [u8]
}

#[derive(Debug)]
#[repr(C, packed)]
struct DstHeader {
  len: u8,
  data: u8,
}

unsafe impl Mutant for DstHeader {
  type Error = Infallible;
  type Head = Self;
  type Tail = ();

  fn validate(_head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(())
  }
}

unsafe impl Mutant for Dst {
  type Error = Infallible;
  type Head = DstHeader;
  type Tail = [u8];

  fn validate(head: &Self::Head) -> Result<<Self::Tail as Tail>::Size, Self::Error> {
    Ok(head.len as usize)
  }
}

#[test]
fn test() {
  println!("{:?}", unpack::<u8>(&[0]));
  println!("{:?}", unpack::<bool>(&[0]));
  println!("{:?}", unpack::<bool>(&[1]));
  println!("{:?}", unpack::<bool>(&[2]));
  println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
  println!("{:?}", unpack::<X>(&[2]));
  println!("{:?}", unpack::<X>(&[0]));
  println!("{:?}", unpack::<X>(&[1]));
  println!("{:?}", unpack::<X>(&[1, 2]));
  let d: &Dst = unpack_unsize_prefix(&[4, 2, 1, 2, 3, 4]).unwrap().0;
  println!("{:?}", d);
}
