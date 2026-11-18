//! No value that holds a pointer is taken from the start of bytes.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let _ = puns::from_head::<Opt<u32>>(&[0; 8]);
}
