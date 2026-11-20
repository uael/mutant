// What the discovery of the layout of an enum without `repr` rests on (`puns/src/discover.rs`, R2
// and R4): a value made in const evaluation has its padding uninitialized, and reading it is an
// error. Here the bytes 1 to 3 of `E::B`, between its one-byte tag and its `u32`. If this ever
// compiles, a byte of padding would be taken for a byte of a tag.
use core::mem::MaybeUninit;

#[expect(dead_code, reason = "only its layout matters")]
enum E {
  A(u8),
  B(u32),
}

const SAMPLE: MaybeUninit<E> = MaybeUninit::new(E::B(7));

// SAFETY: Not sound, on purpose: byte 2 of `E::B` is padding. Const evaluation must report it.
const PADDING: u8 = unsafe { *SAMPLE.as_ptr().cast::<u8>().add(2) };

fn main() {
  let _ = PADDING;
}
