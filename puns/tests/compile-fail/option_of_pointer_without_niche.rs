//! `Option` of a pointer whose offset has no niche at zero (a byte array) has a tag of its own: it
//! is not the pointer that may be null, which `Layout::nullable` finds where it is used.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

#[derive(puns::Pun)]
#[repr(C)]
struct Maybe {
  id: u32,
  next: Option<One<u32>>,
}

fn main() {
  let _ = puns::from_bytes::<Maybe>(&[0; 12]);
}
