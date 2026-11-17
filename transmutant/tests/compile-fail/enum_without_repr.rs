//! The layout of an enum with fields that has no `repr` is not specified: the derive asks for one.
#[derive(transmutant::Data)]
enum Unspecified {
  A(u8),
  B(u16),
}

fn main() {}
