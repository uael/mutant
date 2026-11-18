//! The fields of an enum without its tag: what is written may not be what the tag tells of.
#[derive(puns::Pun)]
#[repr(u8)]
enum Code {
  Short(u8, u16),
  Long([u8; 3]),
}

fn main() {
  let _: &mut [u8; 2] = puns::tail_mut(&mut Code::Long([1, 2, 3]));
}
