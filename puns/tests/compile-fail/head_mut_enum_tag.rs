//! The tag of an enum without the fields it tells of: another tag would make other bytes fields.
#[derive(puns::Pun)]
#[repr(u8)]
enum Code {
  Short(u8, u16),
  Long([u8; 3]),
}

fn main() {
  let _: &mut u8 = puns::head_mut(&mut Code::Long([1, 2, 3]));
}
