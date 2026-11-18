//! An assertion is a `bool`, as the condition of an `if` is in Rust: an integer is not one.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(flags & 1))]
struct Flags {
  flags: u8,
}

/// Neither is arithmetic on a `bool`.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(size = 2 + (kind == 1) * 4)]
struct Varying {
  kind: u8,
  data: [u8],
}

fn main() {}
