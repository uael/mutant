//! A pointer whose offset is a type of the user's is `Pun` once the byte order of that offset is
//! told, by implementing `puns::nearest::Order` for it.
use std::num::NonZero;

use nearest::{Near, Offset, flat};
use puns::Pun;

#[flat]
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
struct Mine(i32);

// SAFETY: an `i32`, as `i32` is an offset.
unsafe impl Offset for Mine {
  type NonZero = NonZeroMine;
  fn from_isize(n: isize) -> Self {
    Self(n as i32)
  }
  fn to_isize(self) -> isize {
    self.0 as isize
  }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct NonZeroMine(NonZero<i32>);

impl From<NonZeroMine> for Mine {
  fn from(n: NonZeroMine) -> Self {
    Self(n.0.get())
  }
}

#[flat]
#[derive(Pun)]
struct Node {
  next: Option<Near<Node, Mine>>,
}

fn main() {}
