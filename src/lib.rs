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

// fn ----------------------------------------------------------------------------------------------

pub fn unpack<T: ?Sized + Mutant>(bytes: &[u8]) -> Option<(&T, &[u8])> {
  union PtrRepr<T: ?Sized + Mutant> {
    bytes: *const [u8],
    value: *const T,
    head: *const T::Head,
  }

  let ptr = PtrRepr::<T> { bytes };

  // Fast path for bytes.
  if TypeId::of::<T>() == TypeId::of::<[u8]>() {
    return Some((unsafe { &*ptr.value }, &[]));
  }

  // Primitive types.
  if TypeId::of::<T>() == TypeId::of::<T::Head>() {
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
  }

  Some(( unsafe { &*value_ptr }, rest))
}

// impl Mutant -------------------------------------------------------------------------------------

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

// #[repr(C, u8)]
// #[derive(Debug)]
// enum X {
//   A = 0,
//   B(u8) = 1,
// }

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

#[test]
fn test() {
  println!("{:?}", unpack::<[u8]>(&[0, 1, 2, 3, 4, 5]));
  println!("{:?}", unpack::<[bool]>(&[0, 1, 0, 1]));
  println!("{:?}", unpack::<[Unalign<u16>]>(&[0, 1, 0, 2]));
  println!("{:?}", unpack::<u8>(&[0]));
  println!("{:?}", unpack::<bool>(&[0]));
  println!("{:?}", unpack::<bool>(&[1]));
  println!("{:?}", unpack::<bool>(&[2]));
  println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
  println!("{:?}", unpack::<Dst<[u8]>>(&[3, 1, 2, 3, 4]));
}
