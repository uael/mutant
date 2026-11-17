//! The valid counterparts of the programs in `compile-fail/`.
use transmutant::{Data, num::u8c};

#[derive(Data, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Data, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

#[derive(Data, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Unspecified {
  A(u8),
  B(u16),
}

fn main() {
  let _: &[u8; 4] = transmutant::transmute_ref(&(0u8, 0u8, 0u16));
  let _: &str = transmutant::try_transmute_ref(&[b'a'; 4][..]).unwrap();
  let _ = transmutant::try_transmute::<transmutant::num::u8c<1>, bool>(u8c::new());
  let _: bool = transmutant::try_transmute(1u8).unwrap();
  let _: &mut u8 = transmutant::transmute_mut(&mut 0u8);
  let text = String::from("text");
  let _: &[u8] = transmutant::transmute_ref(text.as_str());
  let _: &[u8; 4] = transmutant::transmute_ref(&0u32);
  let _ = transmutant::zeroed::<Option<core::num::NonZero<u8>>>();
  let _: Option<Shape> = transmutant::try_transmute([1u8, 3, 1, 0]).ok();
  let _: Drawing = transmutant::transmute(Shape::Rect(3, true));
  let mut drawing = Drawing::Nothing;
  let _: Result<&mut Shape, _> = transmutant::try_transmute_mut(&mut drawing);
  let _ = Unspecified::A(1);
}
