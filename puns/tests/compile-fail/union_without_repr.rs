// The fields of a union are at offset 0 with `repr(C)`; without it, the layout is not specified.
#[derive(puns::Pun, Clone, Copy)]
union Word {
  bytes: [u8; 4],
  word: u32,
}

fn main() {}
