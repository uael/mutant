//! Validation trusts what implements `Count` (how many operations an expression has): it is
//! sealed.
struct Many;

impl puns::Count for Many {
  const VALUE: usize = 1000;
  type Less = Many;
}

fn main() {}
