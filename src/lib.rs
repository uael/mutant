use core::fmt::Debug;
use core::mem::{align_of, align_of_val, size_of, size_of_val};
use core::fmt::Formatter;
use core::any::TypeId;

mod ptr;
mod unalign;
mod unaligned;
mod undrop;

pub use unalign::*;
pub use unaligned::*;
pub use undrop::*;

pub trait Mutant: 'static {
  type Metadata: Copy;

  type Head: Copy + Mutant<Tail = ()>;
  type Tail: ?Sized + Mutant<Metadata = Self::Metadata>;

  #[inline(always)]
  fn trailing(_head: &Self::Head, tail: &[u8]) -> usize {
    size_of_within::<Self::Tail>(tail)
  }

  #[inline(always)]
  fn validate(_head: &Self::Head, _tail: &Self::Tail) -> bool {
    true
  }

  #[inline(always)]
  unsafe fn metadata(size: usize) -> Option<Self::Metadata> {
    if size < size_of::<Self::Head>() {
      None
    } else {
      Self::Tail::metadata(size - size_of::<Self::Head>())
    }
  }
}

// fn ----------------------------------------------------------------------------------------------

#[inline(always)]
pub fn unpack_head<T: ?Sized + Mutant>(bytes: &[u8]) -> Option<(&T::Head, &[u8])> {
  if TypeId::of::<T>() != TypeId::of::<T::Head>() {
    return unpack(bytes);
  }

  if align_of::<T::Head>() != 1 && bytes.as_ptr().align_offset(align_of::<T::Head>()) != 0 {
    return None;
  }

  if size_of::<T::Head>() > bytes.len() {
    return None;
  }

  let (head, tail) = (size_of::<T::Head>() <= bytes.len())
    .then(|| bytes.split_at(size_of::<T::Head>()))?;

  Some((unsafe { &*(head.as_ptr() as *const T::Head) }, tail))
}

#[inline(always)]
pub fn size_of_within<T: ?Sized + Mutant>(bytes: &[u8]) -> usize {
  unpack_head::<T>(bytes)
    .map(|(head, tail)| size_of::<T::Head>() + T::trailing(head, tail))
    .unwrap_or(0)
}

#[inline(always)]
pub fn unpack<T: ?Sized + Mutant>(bytes: &[u8]) -> Option<(&T, &[u8])> {
  let (head, tail) = unpack_head::<T>(bytes)?;
  let tail_size = T::trailing(head, tail);
  let meta = unsafe { T::Tail::metadata(tail_size) }?;

  // compute raw address from base pointer and metadata.
  let addr = ptr::from_raw_parts::<T>(bytes.as_ptr() as *const (), meta);

  // SAFETY: [`align_of_val`] never actually deference `val`.
  assert!(align_of_val(head) >= align_of_val(unsafe { &*addr }));

  // SAFETY: [`size_of_val`] never actually deference `val`.
  let size = size_of_val(unsafe { &*addr });
  if size > bytes.len() {
    // alignment mismatch.
    return None;
  }

  let tail = if TypeId::of::<T>() == TypeId::of::<T::Tail>() {
    assert_eq!(size_of::<T::Head>(), 0);
    let addr = ptr::from_raw_parts::<T::Tail>(bytes.as_ptr() as *const (), meta);
    // SAFETY: `tail` is bound checked.
    unsafe { &*addr }
  } else {
    let (tail, _) = unpack(&bytes[size_of::<T::Head>()..size])?;
    if size_of_val(tail) != tail_size {
      return None;
    }
    tail
  };

  assert_eq!(size_of_val(head) + size_of_val(tail), size);

  if T::validate(head, tail) {
    // SAFETY: bound checked.
    Some(( unsafe { &*addr }, &bytes[size..]))
  } else {
    None
  }
}

// impl Mutant -------------------------------------------------------------------------------------

impl Mutant for () {
  type Metadata = ();

  type Head = ();
  type Tail = ();

  #[inline(always)]
  fn trailing(_head: &Self::Head, _tail: &[u8]) -> usize {
    0
  }

  #[inline(always)]
  unsafe fn metadata(size: usize) -> Option<Self::Metadata> {
    if size == 0 {
      Some(())
    } else {
      None
    }
  }
}

impl<T: Mutant<Tail = ()>> Mutant for [T] {
  type Metadata = usize;

  type Head = ();
  type Tail = [T::Head];

  #[inline(always)]
  fn trailing(_head: &Self::Head, tail: &[u8]) -> usize {
    tail.len()
  }

  #[inline(always)]
  fn validate(_head: &Self::Head, tail: &Self::Tail) -> bool {
    tail.iter().all(|x| T::validate(x, &()))
  }

  #[inline(always)]
  unsafe fn metadata(size: usize) -> Option<Self::Metadata> {
    match size_of::<T>() {
      0 => (size == 0).then_some(0),
      n => (size % n == 0).then_some(size / n)
    }
  }
}

impl<T: Mutant<Tail = ()>, const N: usize> Mutant for [T; N] {
  type Metadata = ();

  type Head = [T::Head; N];
  type Tail = ();

  #[inline(always)]
  fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
    head.iter().all(|x| T::validate(x, &()))
  }
}

impl Mutant for u8 {
  type Metadata = ();

  type Head = Self;
  type Tail = ();
}

impl Mutant for u16 {
  type Metadata = ();

  type Head = Self;
  type Tail = ();
}

impl Mutant for u32 {
  type Metadata = ();

  type Head = Self;
  type Tail = ();
}

impl Mutant for u64 {
  type Metadata = ();

  type Head = Self;
  type Tail = ();
}

impl Mutant for bool {
  type Metadata = ();

  type Head = u8;
  type Tail = ();

  #[inline(always)]
  fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
    matches!(*head, 0 | 1)
  }
}

impl Mutant for char {
  type Metadata = ();

  type Head = u32;
  type Tail = ();

  #[inline(always)]
  fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
    char::from_u32(*head).is_some()
  }
}

// tests -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

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

  impl<T: ?Sized + Mutant> Mutant for Dst<T> {
    type Metadata = T::Metadata;

    type Head = u8;
    type Tail = T;

    #[inline(always)]
    fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
      *head as usize
    }
  }

  #[test]
  fn test() {
    println!("{:?}", unpack::<()>(&[1, 0, 0, 0]));
    println!("{:?}", unpack::<[u8]>(&[0, 1, 2, 3, 4, 5]));
    println!("{:?}", unpack::<[bool]>(&[0, 1, 0, 1]));
    println!("{:?}", unpack::<[Unalign<u16>]>(&[0, 1, 0, 2]));
    println!("{:?}", unpack::<u8>(&[0]));
    println!("{:?}", unpack::<bool>(&[0]));
    println!("{:?}", unpack::<bool>(&[1]));
    println!("{:?}", unpack::<bool>(&[2]));
    println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
    println!("{:?}", unpack::<Dst<[u8]>>(&[3, 1, 2, 3, 4]));
    println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[2, 1, 0]));
    println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[1, 1, 0]));
    println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[3, 1, 0]));
    println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[4, 1, 0, 2, 0]));
    println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[5, 1, 0, 2, 0]));
    //
    // println!("{:?}", unpack::<A>(&[0]));
    // println!("{:?}", unpack::<B>(&[1, 69]));
    // println!("{:?}", unpack::<C>(&[2, 3, 1, 2, 3]));
    //
    // println!("{:?}", unpack_x(&[0]));
    // println!("{:?}", unpack_x(&[1, 69]));
    // println!("{:?}", unpack_x(&[2, 3, 1, 2, 3]));
    //
    // println!("{:?}", unpack::<Command<[u8]>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
    // println!("{:?}", unpack::<Command<SetEventMask>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
    //
    // println!("{:?}", unpack::<Command<[u8]>>(&[0x42, 0, 0]));
    // println!("{:?}", unpack::<Command<Reset>>(&[0x42, 0, 0]));
  }

  #[test]
  fn test_hci() {
    #[derive(Copy, Clone)]
    #[repr(C, packed)]
    struct Packet<T: ?Sized> {
      r#type: u8,
      data: Undrop<T>,
    }

    trait PacketVariant: Mutant {
      const PACKET_TYPE: u8;
    }

    impl<T: ?Sized + EventVariant> PacketVariant for Event<T> {
      const PACKET_TYPE: u8 = 1;
    }

    impl PacketVariant for Acl<[u8]> {
      const PACKET_TYPE: u8 = 2;
    }

    unsafe impl<T: ?Sized> Unaligned for Packet<T> {}

    impl Mutant for Packet<()> {
      type Metadata = ();
      type Head = Self;
      type Tail = ();
    }

    impl<T: ?Sized + PacketVariant> Mutant for Packet<T> {
      type Metadata = T::Metadata;
      type Head = Packet<()>;
      type Tail = T;

      fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
        head.r#type == T::PACKET_TYPE
      }
    }

    impl<T: ?Sized + Unaligned + Debug> Debug for Packet<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Packet")
          .field("type", &self.r#type)
          .field("data", &read_aligned!(self.data))
          .finish()
      }
    }

    #[derive(Copy, Clone)]
    #[repr(C, packed)]
    struct Acl<T: ?Sized> {
      handle_and_flags: Unalign<u16>,
      data_total_len: Unalign<u16>,
      parameters: Undrop<T>,
    }

    unsafe impl<T: ?Sized> Unaligned for Acl<T> {}

    impl Mutant for Acl<()> {
      type Metadata = ();
      type Head = Self;
      type Tail = ();
    }

    impl Mutant for Acl<[u8]> {
      type Metadata = usize;
      type Head = Acl<()>;
      type Tail = [u8];

      #[inline(always)]
      fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
        head.data_total_len.into_inner() as usize
      }
    }

    impl<T: ?Sized + Unaligned + Debug> Debug for Acl<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Acl")
          .field("handle_and_flags", &self.handle_and_flags)
          .field("data_total_len", &self.data_total_len)
          .field("completed_packets", &read_aligned!(self.parameters))
          .finish()
      }
    }

    #[derive(Copy, Clone)]
    #[repr(C, packed)]
    struct Event<T: ?Sized> {
      event_code: u8,
      parameters_len: u8,
      parameters: Undrop<T>,
    }

    #[derive(Debug, Copy, Clone)]
    #[repr(C, packed)]
    struct ConnectionComplete {
      address: [u8; 6],
      handle: u16,
      link_type: u8,
    }

    #[derive(Debug, Copy, Clone)]
    #[repr(C, packed)]
    struct CompletedPackets {
      handle: u16,
      number_of_completed_packets: u16,
    }

    #[repr(C, packed)]
    struct NumberOfCompletedPackets {
      num_handles: u8,
      completed_packets: [CompletedPackets]
    }

    unsafe impl<T: ?Sized> Unaligned for Event<T> {}
    unsafe impl Unaligned for ConnectionComplete {}
    unsafe impl Unaligned for CompletedPackets {}
    unsafe impl Unaligned for NumberOfCompletedPackets {}

    trait EventVariant: Mutant {
      const EVENT_CODE: Option<u8>;
    }

    impl EventVariant for [u8] {
      const EVENT_CODE: Option<u8> = None;
    }

    impl EventVariant for ConnectionComplete {
      const EVENT_CODE: Option<u8> = Some(21);
    }

    impl EventVariant for NumberOfCompletedPackets {
      const EVENT_CODE: Option<u8> = Some(42);
    }

    impl Mutant for ConnectionComplete {
      type Metadata = ();
      type Head = Self;
      type Tail = ();
    }

    impl Mutant for NumberOfCompletedPackets {
      type Metadata = usize;
      type Head = u8;
      type Tail = [CompletedPackets];
    }

    impl Mutant for CompletedPackets {
      type Metadata = ();
      type Head = Self;
      type Tail = ();
    }

    impl Mutant for Event<()> {
      type Metadata = ();
      type Head = Self;
      type Tail = ();
    }

    impl<T: ?Sized + EventVariant> Mutant for Event<T> {
      type Metadata = T::Metadata;
      type Head = Event<()>;
      type Tail = T;

      #[inline(always)]
      fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
        head.parameters_len as usize
      }

      #[inline(always)]
      fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
        match T::EVENT_CODE {
          Some(event_code) => event_code == head.event_code,
          _ => true,
        }
      }
    }

    impl<T: ?Sized + Debug + Unaligned> Debug for Event<T> {
      fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
          .field("event_code", &self.event_code)
          .field("parameters_len", &self.parameters_len)
          .field("parameters", &read_aligned!(self.parameters))
          .finish()
      }
    }

    impl Debug for NumberOfCompletedPackets {
      fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumberOfCompletedPackets")
          .field("num_handles", &self.num_handles)
          .field("completed_packets", &read_aligned!(self.completed_packets))
          .finish()
      }
    }

    println!("{:?}", unpack::<Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
    println!("{:?}", unpack::<Event<ConnectionComplete>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
    println!("{:?}", unpack::<Event<ConnectionComplete>>(&[22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
    println!("{:?}", unpack::<Packet<Event<[u8]>>>(&[1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
    println!("{:?}", unpack::<Packet<Event<ConnectionComplete>>>(&[1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3]));
  }
}
