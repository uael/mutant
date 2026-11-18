//! `view_ref` is not a `const fn`: what pointers aim at is proven by instances of functions,
//! which a constant would not make.
use nearest::Near;

const NONE: Option<Near<u32>> = None;
const VIEW: &Option<Near<[u8; 4]>> = puns::view_ref(&NONE);

fn main() {
  let _ = VIEW;
}
