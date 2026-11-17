//! Validation trusts what implements `Path` (where a value is checked): it is sealed.
struct Anywhere;

impl puns::Path for Anywhere {
  const LEAF: usize = 1000;
}

fn main() {}
