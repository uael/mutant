//! The elements of a sequence are as aligned as its declarer says, which `view_ref` reads from
//! the description of the pointer: here the larger of the alignment of the elements and the width
//! of the offset, 2 for `[u8; 4]` behind an `i16le`, too little for `u32`s (behind an `i32le` it
//! is 4, and the same view compiles).
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use puns::num::i16le;
use relative::Seq;

fn view(source: &Seq<[u8; 4], i16le>) -> &Seq<u32, i16le> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&Seq<[u8; 4], i16le>) -> &Seq<u32, i16le> = view;
}
