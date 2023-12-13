use core::fmt::Debug;
use core::mem;
use core::num::NonZeroUsize;
use std::marker::PhantomData;

// mod unalign;
// mod unaligned;
// mod undrop;
//
// pub use unalign::*;
// pub use unaligned::*;
// pub use undrop::*;

pub unsafe trait Packed: 'static {
  const LAYOUT: TypeLayout;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct TypeLayout {
  head: usize,
  tail: Option<NonZeroUsize>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum MetadataRequirement {
  Is(usize),
  IsAtLeast(usize),
  IsMultipleOf(usize),
  IsAtLeastThenMultipleOf(usize, usize),
  IsEmpty,
}

// impl TypeLayout ---------------------------------------------------------------------------------

impl TypeLayout {
  #[inline(always)]
  const fn new(head: usize, tail: Option<NonZeroUsize>) -> Self {
    Self { head, tail }
  }

  #[inline(always)]
  pub const fn of_sized<T: Sized>() -> Self {
    assert!(mem::align_of::<T>() == 1, "type must be unaligned");
    Self::new(mem::size_of::<T>(), None)
  }

  #[inline(always)]
  pub const fn of_slice<T: Packed>() -> Self {
    assert!(mem::align_of::<T>() == 1, "slice item type must be unaligned");
    Self::new(0, NonZeroUsize::new(T::LAYOUT.head))
  }

  #[inline(always)]
  pub const unsafe fn of_struct(fields: &[Self]) -> Self {
    let (mut i, mut layout) = (0, Self::new(0, None));
    while i < fields.len() {
      assert!(layout.tail.is_none());
      layout.head += fields[i].head;
      layout.tail = fields[i].tail;
      i += 1;
    }
    layout
  }

  #[inline(always)]
  pub const fn size_of(&self, metadata: usize) -> usize {
    match (self.head, self.tail) {
      (head, None) => {
        assert!(metadata == 0, "only `DST` have metadata");
        head
      },
      (head, Some(tail)) => head + tail.get() * metadata
    }
  }

  #[inline(always)]
  pub const fn metadata(&self, mut size: usize) -> Option<usize> {
    match (self.head, self.tail) {
      (head, None) => if size == head {
        Some(0)
      } else {
        None
      },
      (head, Some(tail)) => if size >= head {
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

  #[inline(always)]
  pub const fn transmute(&self, into: &Self) -> Option<MetadataRequirement> {
    if self.head > into.head {
      // Calculate the size difference between `self.head` and `into.head`.
      let remaining = self.head - into.head;
      match (self.tail, into.tail) {
        (a, Some(b)) => {
          // Calculate how much of the `remaining` bytes can fit in the tail of `other`.
          // This is done by finding the remainder of `remaining` when divided by the size of `b`.
          let remaining = remaining % b.get();
          if remaining != 0 {
            return match a {
              None => panic!(
                "cannot transmute between types of incompatible byte layouts, \
                remaining bytes to fit in the target is not a multiple of the target `DST` \
                trailing slice item size"
              ),
              Some(a) => {
                let gcd = gcd(a.get(), b.get());
                if self.head % gcd != 0 {
                  panic!(
                    "cannot transmute between `DST` types of incompatible byte layouts, \
                    source cannot catchup on target"
                  )
                }
                if into.head % gcd != 0 {
                  panic!(
                    "cannot transmute between `DST` types of incompatible byte layouts, \
                    target cannot catchup on source"
                  )
                }
                let mut i = self.head;
                while (i - into.head) % b.get() != 0 {
                  i += a.get();
                }
                let x = (i - self.head) / a.get();
                match lcm(a.get(), b.get()) / a.get() {
                  1 => Some(MetadataRequirement::IsAtLeast(x)),
                  n => Some(MetadataRequirement::IsAtLeastThenMultipleOf(x, n)),
                }
              }
            }
          }
        }
        _ => panic!(
          "cannot transmute between types of different sizes, \
          source is bigger than target"
        ),
      };
    } else if self.head < into.head {
      // Calculate the size difference between `into.head` and `self.head`.
      let missing = into.head - self.head;
      return match (self.tail, into.tail) {
        (None, _) => panic!(
          "cannot transmute between types of different sizes, \
          source is smaller than target"
        ),
        (Some(a), None) => {
          assert!(
            missing % a.get() == 0,
            "cannot transmute between types of incompatible byte layouts, \
            missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
            item size"
          );
          Some(MetadataRequirement::Is(missing / a.get()))
        }
        (Some(a), Some(b)) => {
          let x = lcm(missing + a.get(), missing + b.get()) - missing;
          assert!(x % a.get() == 0, "origin cannot fit target");
          assert!(x % b.get() == 0, "origin cannot fit target");
          // Determine the metadata requirement based on the value of `n`,
          // which is the LCM of `a.get()` and `b.get()` divided by `a.get()`.
          match lcm(a.get(), b.get()) / a.get() {
            // If `n` is 1, it implies that the size of `a` is already aligned with `b`.
            // Thus, the metadata requirement is at least `x / a.get()`.
            1 => Some(MetadataRequirement::IsAtLeast(x / a.get())),
            // If `n` is greater than 1, it indicates a more complex alignment requirement,
            // where `x / a.get()` is the minimum alignment, and `n` is the multiple of `a`
            // that fits into `b`. This represents a case where multiple instances of `a`
            // are required for alignment with `b`.
            n => Some(MetadataRequirement::IsAtLeastThenMultipleOf(x / a.get(), n)),
          }
        }
      }
    }

    // Heads are equivalents.
    match (self.tail, into.tail) {
      // This block calculates the minimum number of times `a` must be repeated to reach a size
      // that is a multiple of `b`. It first computes the least common multiple (LCM) of the
      // sizes `a` and `b` using the `lcm` function. The LCM is the smallest positive integer
      // that is divisible by both `a` and `b`.
      // The value of `n` is then determined by dividing this LCM by the size of `a`.
      // This gives the minimum number of `a` items that fit into a size multiple of `b`.
      //
      // The function then returns different metadata requirements based on the value of `n`:
      // - If `n` is 1, it implies that the size of `a` is already a multiple of `b`, or they
      //   are equal. In this case, no additional metadata requirements are needed.
      // - If `n` is greater than 1, it indicates that multiple instances of `a` are required to
      //   reach a size that is a multiple of `b`. Hence, it returns
      //   `MetadataRequirement::MultipleOf(n)`, signifying the need for `n` instances of `a`.
      (Some(a), Some(b)) => match lcm(a.get(), b.get()) / a.get() {
        1 => None,
        n => Some(MetadataRequirement::IsMultipleOf(n)),
      },
      (Some(_), _) => Some(MetadataRequirement::IsEmpty),
      _ => None,
    }
  }
}

#[inline(always)]
const fn gcd(a: usize, b: usize) -> usize {
  match b {
    0 => a,
    _ => gcd(b, a % b),
  }
}

#[inline(always)]
const fn lcm(a: usize, b: usize) -> usize {
  a * b / gcd(a, b)
}

// impl --------------------------------------------------------------------------------------------

unsafe impl Packed for () {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

unsafe impl Packed for u8 {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

unsafe impl<T: Packed> Packed for [T] {
  const LAYOUT: TypeLayout = TypeLayout::of_slice::<T>();
}

unsafe impl<T: Packed, const N: usize> Packed for [T; N] {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u16le([u8; 2]);

unsafe impl Packed for u16le {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct u32le([u8; 4]);

unsafe impl Packed for u32le {
  const LAYOUT: TypeLayout = TypeLayout::of_sized::<Self>();
}

// fn ----------------------------------------------------------------------------------------------

struct Check<A: Packed + ?Sized, B: Packed + ?Sized>(PhantomData<A>, PhantomData<B>);

impl<A: Packed + ?Sized, B: Packed + ?Sized> Check<A, B> {
  const REQUIREMENT: Option<MetadataRequirement> = A::LAYOUT.transmute(&B::LAYOUT);
}

#[inline(always)]
pub const fn try_transmute<T: Packed + ?Sized, U: Packed + ?Sized>(value: &T) -> Option<&U> {
  let _ = Check::<T, U>::REQUIREMENT;
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
pub const fn transmute<T: Packed + ?Sized, U: Packed + ?Sized>(value: &T) -> &U {
  match try_transmute(value) {
    Some(value) => value,
    _ => panic!("transmute of types with incompatible layouts."),
  }
}

#[inline(always)]
pub fn try_transmute_mut<T: Packed + ?Sized, U: Packed + ?Sized>(value: &mut T) -> Option<&mut U> {
  let _ = Check::<T, U>::REQUIREMENT;
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
pub fn transmute_mut<T: Packed + ?Sized, U: Packed + ?Sized>(value: &mut T) -> &mut U {
  match try_transmute_mut(value) {
    Some(value) => value,
    _ => panic!("transmute of types with incompatible layouts."),
  }
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

  #[macro_export]
  macro_rules! assert_panic {
    ($msg:expr, $e:expr) => {
      assert_eq!(
        Some($msg),
        std::panic::catch_unwind(|| { $e })
          .err()
          .and_then(|a| a.downcast_ref::<&'static str>().cloned())
      )
    };
  }

  #[test]
  fn test_layout() {
    macro_rules! layout {
      ($h:expr $(,)?) => { &TypeLayout::new($h, None) };
      ($h:expr, [$t:expr] $(,)?) => { &TypeLayout::new($h, NonZeroUsize::new($t)) };
    }

    assert_eq!(
      None,
      layout! { 1 }.transmute(layout!{ 1 })
    );

    assert_eq!(
      None,
      layout! { 42 }.transmute(layout!{ 42 })
    );

    assert_panic!(
      "cannot transmute between types of different sizes, \
      source is bigger than target",
      layout! { 2 }.transmute(layout!{ 1 })
    );

    assert_panic!(
      "cannot transmute between types of different sizes, \
      source is smaller than target",
      layout! { 1 }.transmute(layout!{ 2 })
    );

    assert_eq!(
      Some(MetadataRequirement::IsEmpty),
      layout! { 42, [1] }.transmute(layout!{ 42 })
    );

    assert_eq!(
      Some(MetadataRequirement::Is(7)),
      layout! { 35, [1] }.transmute(layout!{ 42 })
    );

    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
      item size",
      layout! { 35, [2] }.transmute(layout!{ 42 })
    );

    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      missing bytes to fit the target is not a multiple of the source `DST` trailing slice \
      item size",
      layout! { 40, [3] }.transmute(layout!{ 42 })
    );

    assert_eq!(
      None,
      layout! { 42, [1] }.transmute(layout!{ 42, [1] })
    );

    assert_eq!(
      None,
      layout! { 42, [3] }.transmute(layout!{ 42, [1] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(2)),
      layout! { 42, [1] }.transmute(layout!{ 42, [2] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(3)),
      layout! { 42, [1] }.transmute(layout!{ 42, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(3)),
      layout! { 42, [2] }.transmute(layout!{ 42, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(3)),
      layout! { 42, [4] }.transmute(layout!{ 42, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(8)),
      layout! { 42, [5] }.transmute(layout!{ 42, [8] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsMultipleOf(3)),
      layout! { 42, [2] }.transmute(layout!{ 42, [6] })
    );

    assert_panic!(
      "cannot transmute between types of incompatible byte layouts, \
      remaining bytes to fit in the target is not a multiple of the target `DST` \
      trailing slice item size",
      layout! { 42 }.transmute(layout!{ 38, [3] })
    );

    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      target cannot catchup on source",
      layout! { 42, [3] }.transmute(layout!{ 38, [3] })
    );

    assert_panic!(
      "cannot transmute between `DST` types of incompatible byte layouts, \
      source cannot catchup on target",
      layout! { 41, [3] }.transmute(layout!{ 39, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsAtLeastThenMultipleOf(2, 3)),
      layout! { 42, [1] }.transmute(layout!{ 38, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsAtLeastThenMultipleOf(1, 3)),
      layout! { 42, [2] }.transmute(layout!{ 38, [3] })
    );

    assert_eq!(
      Some(MetadataRequirement::IsAtLeastThenMultipleOf(1, 3)),
      layout! { 43, [2] }.transmute(layout!{ 39, [3] })
    );
  }

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

  unsafe impl<T: ?Sized + Packed> Packed for Foo<T> {
    const LAYOUT: TypeLayout = unsafe {
      TypeLayout::of_struct(&[
        <u8 as Packed>::LAYOUT,
        <u16le as Packed>::LAYOUT,
        <[u8; 6] as Packed>::LAYOUT,
        <T as Packed>::LAYOUT,
      ])
    };
  }

  unsafe impl<T: ?Sized + Packed> Packed for Bar<T> {
    const LAYOUT: TypeLayout = unsafe {
      TypeLayout::of_struct(&[
        <[u8; 4] as Packed>::LAYOUT,
        <T as Packed>::LAYOUT,
      ])
    };
  }

  #[test]
  fn test_transmute() {
    println!("{:?}", Check::<[u8; 4], [u8; 4]>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[u8; 4]>, Bar<[u8; 4]>>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[u8; 4]>, Bar<[u8]>>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[u8; 4]>, Bar<[[u8; 4]]>>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[u8; 4]>, Bar<[[u8; 2]]>>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[[u8; 2]]>, Bar<[[u8; 2]]>>::REQUIREMENT);
    println!("{:?}", Check::<Bar<[[u8; 2]]>, Bar<[[u8; 4]]>>::REQUIREMENT);

    assert_eq!(Some(&[1, 2]), try_transmute::<[u8], [u8; 2]>(&[1, 2]));
    assert_eq!(None, try_transmute::<[u8], [u8; 3]>(&[1, 2]));
    assert_eq!(Some(&Bar { a: [1, 2, 3, 4], b: () }), try_transmute::<[u8], Bar<()>>(&[1, 2, 3, 4]));
    println!("{:?}", try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]));
    println!("{:?}", try_transmute::<Bar<[u8]>, Bar<[u8; 2]>>(try_transmute::<[u8], Bar<[u8]>>(&[1, 2, 3, 4, 5, 6]).unwrap()));
    println!("{:?}", transmute::<Bar<[u8; 3]>, [u8]>(&Bar { a: [1, 2, 3, 4], b: [5, 6, 7] }));
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
