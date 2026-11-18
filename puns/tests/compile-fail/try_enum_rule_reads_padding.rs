//! A rule that reads padding, in the variant of an enum: variants are validated as a whole, so
//! the padding of the struct must be initialized in the source, which in `HoldsPlain` it is not.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum HoldsPlain {
  A(Plain),
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum HoldsReadsPadding {
  A(ReadsPadding),
}

fn main() {
  let _ = puns::try_pun::<HoldsPlain, HoldsReadsPadding>(HoldsPlain::A(Plain {
    a: 0,
    b: 0,
  }));
}
