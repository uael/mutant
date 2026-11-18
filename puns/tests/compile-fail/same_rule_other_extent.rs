//! A rule is proven by the same rule of a struct of the same extent: here the rule reads
//! `size_of_val(self)`, which is 4 in `Wide` and 2 in `Narrow`, so it reads another byte. The
//! first two bytes of a `Wide` that meets its rule may not meet the one of `Narrow` (`[0, 0xff,
//! 0, 0]`), and the rule is checked: `head_ref`, which checks nothing, is refused. (`Narrow` from
//! `Narrow`, at the same place, is proven: see `compile-pass/same_rules.rs`.)
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(size_of_val(self) - 1, 1) != 0xff))]
struct Narrow {
  a: u8,
  b: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(size_of_val(self) - 1, 1) != 0xff))]
struct Wide {
  a: u8,
  b: u8,
  c: u16,
}

fn main() {
  let wide: &Wide = puns::from_bytes(&[0, 0xff, 0, 0]).unwrap();
  let _: &Narrow = puns::head_ref(wide);
}
