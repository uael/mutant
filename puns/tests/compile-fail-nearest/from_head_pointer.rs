//! No value that holds a pointer is taken from the start of bytes.
use nearest::Near;

fn main() {
  let _ = puns::from_head::<Option<Near<u32>>>(&[0; 8]);
}
