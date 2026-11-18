//! `view_ref` is not a `const fn`: what pointers aim at is proven by instances of functions,
//! which a constant would not make.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

const NONE: Opt<u32> = Opt::NONE;
const VIEW: &Opt<[u8; 4]> = puns::view_ref(&NONE);

fn main() {
  let _ = VIEW;
}
