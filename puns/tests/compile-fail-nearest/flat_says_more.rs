//! The loaders compare what `nearest` is told of a type with its layout: here an implementation
//! of `Flat` that says that every bit pattern is valid, of a type that holds a `bool`.
use nearest::{
  Error, Flat,
  __private::{Buf, Check, Sink},
};

#[derive(puns::Pun, Clone, Copy)]
#[repr(transparent)]
struct Flag(bool);

// SAFETY: not kept: `ANY` is not true of a `bool`.
unsafe impl Flat for Flag {
  type Edit<'e> = Self;
  const PLAIN: bool = true;
  const ANY: bool = true;
  unsafe fn store(&self, sink: &mut Sink, at: usize, shift: Option<isize>) {
    // SAFETY: forwarded.
    unsafe { self.0.store(sink, at, shift) }
  }
  unsafe fn edit<'a, 'e: 'a>(_: *mut Buf, _: usize) -> &'a mut Self {
    unimplemented!()
  }
  fn check(_: &mut Check<'_>, _: usize) -> Result<(), Error> {
    Ok(())
  }
}

fn main() {
  let _ = puns::nearest::view::<Flag>(&[1]);
}
