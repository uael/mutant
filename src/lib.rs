use core::fmt::Debug;
use core::mem;
use core::num::NonZeroUsize;

// mod unalign;
// mod unaligned;
// mod undrop;
//
// pub use unalign::*;
// pub use unaligned::*;
// pub use undrop::*;

pub unsafe trait Mutant2: 'static {
  const LAYOUT: TypeLayout;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct TypeLayout {
  head: usize,
  tail: Option<NonZeroUsize>,
}

#[repr(C, align(65536))]
struct Aligned64k([u8; 65536]);

pub const ALIGNED_64K_ADDR: *const () = {
  const REF: &Aligned64k = &Aligned64k([0; mem::size_of::<Aligned64k>()]);
  let ptr: *const _ = REF;
  ptr.cast()
};

#[macro_export]
macro_rules! offset_of {
  ($Container:ty, $($fields:tt).+ $(,)?) => {{
    let ptr = $crate::ptr::from_addr::<$Container>(ALIGNED_64K_ADDR);
    // SAFETY:
    // - `ptr` is never actually deferenced.
    // - in case 64k isn't enough, [`core::ptr::addr_of`] will emit a compiler error.
    let field_ptr = unsafe {
      core::ptr::addr_of!((*ptr).$($fields).+)
    };
    // SAFETY: Safe as all `offset_from` garantees are assured by [`ptr::addr_of`].
    let offset = unsafe { field_ptr.cast::<u8>().offset_from(ptr.cast::<u8>()) };
    // Guaranteed not to be lossy: `field` comes after `ptr`, so the offset
    // from `ptr` to `field` is guaranteed to be positive.
    assert!(offset >= 0);
    offset as usize
  }};
}

// impl TypeLayout ---------------------------------------------------------------------------------

impl TypeLayout {
  #[inline(always)]
  pub const fn of_sized<T: Sized>() -> Self {
    assert!(mem::align_of::<T>() == 1, "type must be unaligned");
    Self { head: mem::size_of::<T>(), tail: None }
  }

  #[inline(always)]
  pub const fn of_slice<T: Mutant2>() -> Self {
    assert!(mem::align_of::<T>() == 1, "slice item type must be unaligned");
    Self { head: 0, tail: NonZeroUsize::new(T::LAYOUT.head) }
  }

  #[inline(always)]
  pub const unsafe fn of_struct(fields: &[(Self, usize)]) -> Self {
    let (mut i, mut layout) = (0, Self { head: 0, tail: None });
    while i < fields.len() {
      assert!(layout.tail.is_none());
      assert!(layout.head == fields[i].1, "struct must be at least `#[repr(packed)]`");
      layout.head += fields[i].0.head;
      layout.tail = fields[i].0.tail;
      i += 1;
    }
    layout
  }

  #[inline(always)]
  pub const fn size_of(self, metadata: usize) -> usize {
    match (self.head, self.tail) {
      (head, None) => {
        assert!(metadata == 0, "only `DST` have metadata");
        head
      },
      (head, Some(tail)) => {
        assert!(tail.is_power_of_two());
        head + tail.get() * metadata
      }
    }
  }

  #[inline(always)]
  pub const fn metadata(self, mut size: usize) -> Option<usize> {
    match (self.head, self.tail) {
      (head, None) => if size == head {
        Some(0)
      } else {
        None
      },
      (head, Some(tail)) => if size >= head {
        assert!(tail.is_power_of_two());
        size -= head;
        if size % tail.get() == 0 {
          Some(size / tail.get())
        } else {
          None
        }
      } else {
        None
      }
    }
  }
}

// impl --------------------------------------------------------------------------------------------

unsafe impl Mutant2 for () {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

unsafe impl Mutant2 for u8 {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

unsafe impl<T: Mutant2> Mutant2 for [T] {
  const LAYOUT: TypeLayout = TypeLayout::of_slice::<T>();
}

unsafe impl<T: Mutant2, const N: usize> Mutant2 for [T; N] {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u16le([u8; 2]);

unsafe impl Mutant2 for u16le {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u32le([u8; 4]);

unsafe impl Mutant2 for u32le {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

// fn ----------------------------------------------------------------------------------------------

#[inline(always)]
pub const fn try_transmute<T: Mutant2 + ?Sized, U: Mutant2 + ?Sized>(value: &T) -> Option<&U> {
  let metadata = ptr::metadata(value);
  let size = T::LAYOUT.size_of(metadata);
  match U::LAYOUT.metadata(size) {
    Some(metadata) => {
      let ptr = ptr::from_raw_parts::<U>(value as *const T as *const (), metadata);
      Some(unsafe { &*ptr })
    },
    _ => None,
  }
}

#[inline(always)]
pub const fn transmute<T: Mutant2 + ?Sized, U: Mutant2 + ?Sized>(value: &T) -> &U {
  match try_transmute(value) {
    Some(value) => value,
    _ => panic!("transmute of types with incompatible layouts."),
  }
}

#[inline(always)]
pub fn try_transmute_mut<T: Mutant2 + ?Sized, U: Mutant2 + ?Sized>(value: &mut T) -> Option<&mut U> {
  let metadata = ptr::metadata(value);
  let size = T::LAYOUT.size_of(metadata);
  match U::LAYOUT.metadata(size) {
    Some(metadata) => {
      let ptr = ptr::from_raw_parts_mut::<U>(value as *mut T as *mut (), metadata);
      Some(unsafe { &mut *ptr })
    },
    _ => None,
  }
}

#[inline(always)]
pub fn transmute_mut<T: Mutant2 + ?Sized, U: Mutant2 + ?Sized>(value: &mut T) -> &mut U {
  match try_transmute_mut(value) {
    Some(value) => value,
    _ => panic!("transmute of types with incompatible layouts."),
  }
}

pub const fn align_of<T: ?Sized>() -> usize {
  // SAFETY: `OffsetOfTrailingIsAlignment` is `repr(C)`, and its layout is guaranteed [1] to begin
  // with the single-byte layout for `_byte`, followed by the padding needed to align `_trailing`,
  // then the layout for `_trailing`, and finally any trailing padding bytes needed to
  // correctly-align the entire struct.
  //
  // This computes the alignment of `T` by counting the number of bytes preceding `_trailing`.
  // For instance, if the alignment of `T` is `1`, then no padding is required align `_trailing` and
  // it will be located immediately after `_byte` at offset 1. If the alignment of `T` is 2, then a
  // single padding byte is required before `_trailing`, and `_trailing` will be located at
  // offset 2.
  //
  // This correspondence between offset and alignment holds for all valid Rust alignments, and we
  // confirm this exhaustively.
  //
  // [1]: https://doc.rust-lang.org/nomicon/other-reprs.html#reprc
  #[repr(C)]
  struct OffsetOfTrailingIsAlignment<T: ?Sized> {
    _byte: u8,
    _trailing: T,
  }

  offset_of!(OffsetOfTrailingIsAlignment<T>, _trailing)
}

// -------------------------------------------------------------------------------------------------

pub mod ptr {
  pub use core::ptr;

  /// Extract the metadata component of a pointer.
  #[inline]
  pub const fn metadata<T: ?Sized>(ptr: *const T) -> usize {
    unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
  }

  #[inline]
  pub const fn from_addr<T: ?Sized>(address: *const ()) -> *const T {
    unsafe { PtrRepr::with_components(address, 0).const_ptr }
  }

  /// Forms a (possibly-wide) raw pointer from a data address and metadata.
  #[inline]
  pub(super) const fn from_raw_parts<T: ?Sized>(
    address: *const (),
    metadata: usize,
  ) -> *const T {
    unsafe { PtrRepr::with_components(address, metadata).const_ptr }
  }

  /// Performs the same functionality as [`from_raw_parts`], except that a
  /// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
  #[inline]
  pub(super) const fn from_raw_parts_mut<T: ?Sized>(
    address: *mut (),
    metadata: usize,
  ) -> *mut T {
    unsafe { PtrRepr::with_components(address, metadata).mut_ptr }
  }

  #[repr(C)]
  union PtrRepr<T: ?Sized> {
    const_ptr: *const T,
    mut_ptr: *mut T,
    components: PtrComponents,
  }

  #[repr(C)]
  #[derive(Copy, Clone)]
  struct PtrComponents {
    address: *const (),
    metadata: usize,
  }

  impl<T: ?Sized> PtrRepr<T> {
    const fn empty() -> Self {
      Self::empty_with(ptr::null())
    }

    const fn empty_with(address: *const ()) -> Self {
      Self::with_components(address, 0)
    }

    const fn with_components(address: *const (), metadata: usize) -> Self {
      Self { components: PtrComponents { address, metadata } }
    }

    const fn set_const(mut self, const_ptr: *const T) -> Self {
      self.const_ptr = const_ptr;
      self
    }
  }
}

// tests -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use core::mem::ManuallyDrop;

  #[repr(C, packed)]
  struct Foo<T: ?Sized> {
    a: u8,
    b: u16le,
    c: [u8; 6],
    d: ManuallyDrop<T>,
  }

  #[derive(Debug, Eq, PartialEq)]
  #[repr(C)]
  struct Bar<T: ?Sized> {
    a: [u8; 4],
    b: T,
  }

  unsafe impl<T: ?Sized + Mutant2> Mutant2 for Foo<T> {
    const LAYOUT: TypeLayout = unsafe {
      TypeLayout::of_struct(&[
        (<u8 as Mutant2>::LAYOUT, offset_of!(Self, a)),
        (<u16le as Mutant2>::LAYOUT, offset_of!(Self, b)),
        (<[u8; 6] as Mutant2>::LAYOUT, offset_of!(Self, c)),
        (<T as Mutant2>::LAYOUT, offset_of!(Self, d)),
      ])
    };
  }

  unsafe impl<T: ?Sized + Mutant2> Mutant2 for Bar<T> {
    const LAYOUT: TypeLayout = unsafe {
      TypeLayout::of_struct(&[
        (<[u8; 4] as Mutant2>::LAYOUT, offset_of!(Self, a)),
        (<T as Mutant2>::LAYOUT, offset_of!(Self, b))
      ])
    };
  }

  #[test]
  fn test_offset_of() {
    assert_eq!(0, offset_of!(Foo<()>, a));
    assert_eq!(1, offset_of!(Foo<()>, b));
    assert_eq!(3, offset_of!(Foo<()>, c));
    assert_eq!(9, offset_of!(Foo<()>, d));

    assert_eq!(0, offset_of!(Bar<()>, a));
    assert_eq!(4, offset_of!(Bar<()>, b));
  }

  #[test]
  fn test_transmute() {
    assert_eq!(Some(&[1, 2]), try_transmute::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(None, try_transmute::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(Some(&Bar { a: [1, 2, 3, 4], b: () }), try_transmute::<[u8], Bar<()>>(&[1, 2, 3, 4]));
    println!("{:?}", try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    println!("{:?}", try_transmute::<Bar<[u8]>, Bar<[u8; 2]>>(try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()));
    println!("{:?}", transmute::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
  }

  #[test]
  fn test_layout() {
    #[repr(packed)]
    struct Foo {
      a: u8,
      b: u32le,
      c: u16le,
    }

    unsafe impl Mutant2 for Foo {
      const LAYOUT: TypeLayout = unsafe {
        TypeLayout::of_struct(&[
          (<u8 as Mutant2>::LAYOUT, offset_of!(Self, a)),
          (<u32le as Mutant2>::LAYOUT, offset_of!(Self, b)),
          (<u16le as Mutant2>::LAYOUT, offset_of!(Self, c))
        ])
      };
    }

    println!("{:?}", <Foo as Mutant2>::LAYOUT);

    // let FOO: TypeLayout = struct_layout!(Foo { a: u8, c: u32, d: u16 });
    //
    // let x: Foo = unsafe { core::mem::zeroed() };
    // println!("{:?}", FOO);
  }
}
//
//   #[derive(Copy, Clone)]
//   #[repr(C, packed)]
//   struct Dst<T: ?Sized> {
//     len: u8,
//     payload: Undrop<T>,
//   }
//
//   impl<T: ?Sized + Debug + Unaligned> Debug for Dst<T> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//       f.debug_struct("Dst")
//         .field("len", &self.len)
//         .field("payload", &read_aligned!(self.payload))
//         .finish()
//     }
//   }
//
//   unsafe impl<T: ?Sized> Unaligned for Dst<T> {}
//
//   #[test]
//   fn test() {
//     println!("{:?}", unpack::<()>(&[1, 0, 0, 0]));
//     println!("{:?}", unpack::<[u8]>(&[0, 1, 2, 3, 4, 5]));
//     println!("{:?}", unpack::<[bool]>(&[0, 1, 0, 1]));
//     println!("{:?}", unpack::<[Unalign<u16>]>(&[0, 1, 0, 2]));
//     println!("{:?}", unpack::<u8>(&[0]));
//     println!("{:?}", unpack::<bool>(&[0]));
//     println!("{:?}", unpack::<bool>(&[1]));
//     println!("{:?}", unpack::<bool>(&[2]));
//     println!("{:?}", unpack::<Unalign<char>>(&[b'a', 0, 0, 0]));
//     println!("{:?}", unpack::<Dst<[u8]>>(&[3, 1, 2, 3, 4]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[2, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[1, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<u16>>>(&[3, 1, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[4, 1, 0, 2, 0]));
//     println!("{:?}", unpack::<Dst<Unalign<[u16; 2]>>>(&[5, 1, 0, 2, 0]));
//     //
//     // println!("{:?}", unpack::<A>(&[0]));
//     // println!("{:?}", unpack::<B>(&[1, 69]));
//     // println!("{:?}", unpack::<C>(&[2, 3, 1, 2, 3]));
//     //
//     // println!("{:?}", unpack_x(&[0]));
//     // println!("{:?}", unpack_x(&[1, 69]));
//     // println!("{:?}", unpack_x(&[2, 3, 1, 2, 3]));
//     //
//     // println!("{:?}", unpack::<Command<[u8]>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
//     // println!("{:?}", unpack::<Command<SetEventMask>>(&[0x43, 0, 8, 69, 69, 69, 69, 69, 69, 69, 69]));
//     //
//     // println!("{:?}", unpack::<Command<[u8]>>(&[0x42, 0, 0]));
//     // println!("{:?}", unpack::<Command<Reset>>(&[0x42, 0, 0]));
//   }
//
//   #[test]
//   fn test_hci() {
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Packet<T: ?Sized> {
//       r#type: u8,
//       data: Undrop<T>,
//     }
//
//     trait PacketVariant: Pointee {
//       const PACKET_TYPE: u8;
//     }
//
//     impl<T: ?Sized + EventVariant> PacketVariant for Event<T> {
//       const PACKET_TYPE: u8 = 1;
//     }
//
//     impl PacketVariant for Acl<[u8]> {
//       const PACKET_TYPE: u8 = 2;
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Packet<T> {}
//
//     impl Pointee for Packet<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl<T: ?Sized + PacketVariant> Pointee for Packet<T> {
//       type Metadata = T::Metadata;
//       type Head = Packet<()>;
//       type Tail = T;
//
//       fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
//         head.r#type == T::PACKET_TYPE
//       }
//     }
//
//     impl<T: ?Sized + Unaligned + Debug> Debug for Packet<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Packet")
//           .field("type", &self.r#type)
//           .field("data", &read_aligned!(self.data))
//           .finish()
//       }
//     }
//
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Acl<T: ?Sized> {
//       handle_and_flags: Unalign<u16>,
//       data_total_len: Unalign<u16>,
//       parameters: Undrop<T>,
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Acl<T> {}
//
//     impl Pointee for Acl<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for Acl<[u8]> {
//       type Metadata = usize;
//       type Head = Acl<()>;
//       type Tail = [u8];
//
//       #[inline(always)]
//       fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
//         head.data_total_len.into_inner() as usize
//       }
//     }
//
//     impl<T: ?Sized + Unaligned + Debug> Debug for Acl<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Acl")
//           .field("handle_and_flags", &self.handle_and_flags)
//           .field("data_total_len", &self.data_total_len)
//           .field("completed_packets", &read_aligned!(self.parameters))
//           .finish()
//       }
//     }
//
//     #[derive(Copy, Clone)]
//     #[repr(C, packed)]
//     struct Event<T: ?Sized> {
//       event_code: u8,
//       parameters_len: u8,
//       parameters: Undrop<T>,
//     }
//
//     #[derive(Debug, Copy, Clone)]
//     #[repr(C, packed)]
//     struct ConnectionComplete {
//       address: [u8; 6],
//       handle: u16,
//       link_type: u8,
//     }
//
//     #[derive(Debug, Copy, Clone)]
//     #[repr(C, packed)]
//     struct CompletedPackets {
//       handle: u16,
//       number_of_completed_packets: u16,
//     }
//
//     #[repr(C, packed)]
//     struct NumberOfCompletedPackets {
//       num_handles: u8,
//       completed_packets: [CompletedPackets],
//     }
//
//     unsafe impl<T: ?Sized> Unaligned for Event<T> {}
//     unsafe impl Unaligned for ConnectionComplete {}
//     unsafe impl Unaligned for CompletedPackets {}
//     unsafe impl Unaligned for NumberOfCompletedPackets {}
//
//     trait EventVariant: Pointee {
//       const EVENT_CODE: Option<u8>;
//     }
//
//     impl EventVariant for [u8] {
//       const EVENT_CODE: Option<u8> = None;
//     }
//
//     impl EventVariant for ConnectionComplete {
//       const EVENT_CODE: Option<u8> = Some(21);
//     }
//
//     impl EventVariant for NumberOfCompletedPackets {
//       const EVENT_CODE: Option<u8> = Some(42);
//     }
//
//     impl Pointee for ConnectionComplete {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for NumberOfCompletedPackets {
//       type Metadata = usize;
//       type Head = u8;
//       type Tail = [CompletedPackets];
//     }
//
//     impl Pointee for CompletedPackets {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl Pointee for Event<()> {
//       type Metadata = ();
//       type Head = Self;
//       type Tail = ();
//     }
//
//     impl<T: ?Sized + EventVariant> Pointee for Event<T> {
//       type Metadata = T::Metadata;
//       type Head = Event<()>;
//       type Tail = T;
//
//       #[inline(always)]
//       fn trailing(head: &Self::Head, _tail: &[u8]) -> usize {
//         head.parameters_len as usize
//       }
//
//       #[inline(always)]
//       fn validate(head: &Self::Head, _tail: &Self::Tail) -> bool {
//         match T::EVENT_CODE {
//           Some(event_code) => event_code == head.event_code,
//           _ => true,
//         }
//       }
//     }
//
//     impl<T: ?Sized + Debug + Unaligned> Debug for Event<T> {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("Event")
//           .field("event_code", &self.event_code)
//           .field("parameters_len", &self.parameters_len)
//           .field("parameters", &read_aligned!(self.parameters))
//           .finish()
//       }
//     }
//
//     impl Debug for NumberOfCompletedPackets {
//       fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("NumberOfCompletedPackets")
//           .field("num_handles", &self.num_handles)
//           .field("completed_packets", &read_aligned!(self.completed_packets))
//           .finish()
//       }
//     }
//
//     println!("{:?}", unpack::<Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69]));
//     println!(
//       "{:?}",
//       unpack::<Event<ConnectionComplete>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Event<ConnectionComplete>>(&[22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Packet<Event<[u8]>>>(&[1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
//     );
//     println!(
//       "{:?}",
//       unpack::<Packet<Event<ConnectionComplete>>>(&[
//         1, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3
//       ])
//     );
//   }
// }
