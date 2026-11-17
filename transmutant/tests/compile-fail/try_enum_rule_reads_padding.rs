//! A rule that reads padding, in the variant of an enum: variants are validated as a whole, so
//! the padding of the struct must be initialized in the source, which in `HoldsPlain` it is not.
use transmutant::Data;

#[derive(Data, Clone, Copy)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Data, Clone, Copy)]
#[repr(C)]
#[data(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum HoldsPlain {
  A(Plain),
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
enum HoldsReadsPadding {
  A(ReadsPadding),
}

fn main() {
  let _ = transmutant::try_transmute::<HoldsPlain, HoldsReadsPadding>(HoldsPlain::A(Plain {
    a: 0,
    b: 0,
  }));
}
