//! A size that walks over the bytes of the value reads its padding, as any rule that reads them.
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
#[pun(size = walk(0, |at| at < 4 && be(at, 1) != 0, |at| at + 1) + 4)]
struct Walked {
  a: u8,
  b: u16,
  rest: [u8],
}

fn main() {
  let plain = [Plain { a: 0, b: 0 }];
  let _ = puns::try_pun_ref::<[Plain], Walked>(&plain[..]);
}
