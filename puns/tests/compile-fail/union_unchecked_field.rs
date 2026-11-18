// No field of the union is proven, and checking any of them would read the second byte, which is
// padding in the source: refused, rather than read it.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Flags {
  first: [bool; 2],
  both: u16,
}

/// A `bool`, then a byte of padding.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C, align(2))]
struct Padded {
  flag: bool,
}

fn main() {
  let _ = puns::try_pun::<Padded, Flags>(Padded { flag: true });
}
