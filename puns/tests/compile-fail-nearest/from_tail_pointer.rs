//! Nor from their end.
use nearest::Near;

fn main() {
  let _ = puns::from_tail::<Option<Near<u32>>>(&[0; 8]);
}
