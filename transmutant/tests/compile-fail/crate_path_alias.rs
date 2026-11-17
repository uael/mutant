//! `#[data(crate = ..)]` cannot lead to an alias of `At` that names another place than the field
//! (here, one leaf further, which validation would skip: the `bool` would not be checked).
mod fake {
  pub use transmutant::{Data, Fields, Path};
  pub type At<P, F> =
    transmutant::At<transmutant::At<P, transmutant::TupleField<(bool, bool), 1>>, F>;
  pub mod layout {
    pub use transmutant::layout::{Enum, Field, Layout, Op, Rule, Variant};
  }
}

#[derive(transmutant::Data)]
#[data(crate = fake)]
#[repr(C)]
struct Flag {
  a: u8,
  on: bool,
}

fn main() {
  let _ = transmutant::try_transmute_ref::<[u8; 2], Flag>(&[0, 7]);
}
