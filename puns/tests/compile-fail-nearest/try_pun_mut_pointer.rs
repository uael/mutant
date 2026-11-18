//! Nor a checked one.
use nearest::Near;

fn main() {
  let mut none: Option<Near<u32>> = None;
  let _ = puns::try_pun_mut::<_, [bool; 4]>(&mut none);
}
