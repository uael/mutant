//! Nor more aligned.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

fn view(source: &One<[u8; 4]>) -> &One<u32> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<[u8; 4]>) -> &One<u32> = view;
}
