//! Nor a checked one.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let mut none: Opt<u32> = Opt::none();
  let _ = puns::try_pun_mut::<_, [bool; 4]>(&mut none);
}
