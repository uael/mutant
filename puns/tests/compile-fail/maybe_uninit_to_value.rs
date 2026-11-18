// Nothing is known of the bytes of a `MaybeUninit`: not even that they are initialized.
fn main() {
  let _: u32 = puns::pun(core::mem::MaybeUninit::<u32>::uninit());
}
