//! Nor from their end.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let _ = puns::from_tail::<Opt<u32>>(&[0; 8]);
}
