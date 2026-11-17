//! A rule that reads the whole value reads its trailing padding: the last byte of `Plain`.
use transmutant::Data;

#[derive(Data)]
#[repr(C)]
struct Plain {
  a: u16,
  b: u8,
}

#[derive(Data)]
#[repr(C)]
#[data(assert(crc32(0, size_of_val(self)) != 0))]
struct Summed {
  a: u16,
  b: u8,
}

fn main() {
  let _ = transmutant::try_transmute_ref::<Plain, Summed>(&Plain { a: 0, b: 0 });
}
