//! Two generic types that hold each other: the bounds that the derive finds for each (the type
//! of each field that depends on a parameter is `Pun`) go round in a circle. They are told with
//! `#[pun(bound())]` on both (the ones that the types declare are enough), which this lacks.
use nearest::{Near, Region, flat};
use puns::{Pun, nearest::Order, num::i32le};

#[flat]
#[derive(Pun)]
struct Ping<O: Order> {
  pong: Option<Near<Pong<O>, O>>,
}

#[flat]
#[derive(Pun)]
struct Pong<O: Order> {
  ping: Option<Near<Ping<O>, O>>,
}

fn main() {
  let region = Region::new(Ping::<i32le>::new(None));
  let _ = puns::nearest::view::<Ping<i32le>>(region.as_bytes().unwrap());
}
