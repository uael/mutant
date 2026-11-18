//! Nor mutably.
use nearest::Near;

fn main() {
  let _ = puns::from_bytes_mut::<Option<Near<u32>>>(&mut [0; 4]);
}
