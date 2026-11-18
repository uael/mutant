//! Nor mutably.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let _ = puns::from_bytes_mut::<Opt<u32>>(&mut [0; 4]);
}
