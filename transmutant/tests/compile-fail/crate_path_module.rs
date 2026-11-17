//! `#[data(crate = ..)]` cannot lead to other types than the ones of the crate: here a module
//! `Op` whose `load` would read the padding byte of the struct, where its field is.
#[expect(unused_imports, reason = "what the derive used to name through the path: it no longer does")]
mod fake {
  pub use transmutant::{At, Data, Fields, Path};
  pub mod layout {
    pub use transmutant::layout::{
      Endian, Enum, Field, Layout, Rule, Spec, Variant, merge_ranges, range, tail_offset,
    };
    #[expect(non_snake_case, reason = "a module that stands for the type `Op`")]
    pub mod Op {
      pub use transmutant::layout::Op::*;
      pub const fn load<T>(_: usize) -> transmutant::layout::Op {
        transmutant::layout::Op::Load { offset: 1, size: 1, big: false }
      }
    }
  }
}

#[derive(transmutant::Data)]
#[data(crate = fake)]
#[repr(C)]
#[data(assert(a == 0))]
struct Ruled {
  a: u8,
  b: u16,
}

fn main() {
  let _ = transmutant::try_transmute_ref::<[u16; 2], Ruled>(&[0, 0]);
}
