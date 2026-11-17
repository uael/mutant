//! A rule of the source holds over the bytes that the `&mut` covers.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

fn main() {
  let _: &mut u8 = puns::tail_mut(&mut Ordered { a: 1, b: 2 });
}
