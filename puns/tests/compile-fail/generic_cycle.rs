//! Two generic types that hold each other: the bounds that the derive finds for each (the type
//! of each field that depends on a parameter is `Pun`) go round in a circle. They are told with
//! `#[pun(bound())]` on both (the ones that the types declare are enough), which this lacks.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use puns::{Pun, num::i32le};
use relative::{Offset, Opt};

#[derive(Pun)]
#[repr(C)]
struct Ping<O: Offset> {
  pong: Opt<Pong<O>, O>,
}

#[derive(Pun)]
#[repr(C)]
struct Pong<O: Offset> {
  ping: Opt<Ping<O>, O>,
}

fn view(ping: &Ping<i32le>) -> &Ping<i32le> {
  puns::view_ref(ping)
}

fn main() {
  let _: fn(&Ping<i32le>) -> &Ping<i32le> = view;
}
