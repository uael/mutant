// Nothing is known of what a union holds, not even that it is initialized: not a `u32`, whatever
// its fields.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Word {
  bytes: [u8; 4],
  word: u32,
}

fn main() {
  let _: u32 = puns::pun(Word { word: 1 });
}
