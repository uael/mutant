//! An option where it means nothing is refused, not ignored.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(len = count)]
struct LenOfTheStruct {
  count: u8,
  items: [u8],
}

#[derive(puns::Pun)]
#[repr(C)]
struct SizeOfAField {
  #[pun(size = 4)]
  count: u8,
  items: [u8],
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[pun(assert(true))]
enum AssertOfAnEnum {
  A,
  B,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[pun(bound(T: Copy))]
enum BoundOfAnEnumWithoutFields {
  A,
}

#[derive(puns::Pun)]
#[pun(bound(T: puns::Pun))]
enum BoundOfAView<'a, T> {
  Bytes(&'a [u8]),
  Value(&'a T),
}

fn main() {}
