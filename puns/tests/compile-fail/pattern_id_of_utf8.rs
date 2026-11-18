//! An automaton of the user with the `id` of UTF-8 (a field that safe code writes) is not UTF-8:
//! two patterns are the same by their states, and bytes that any bytes are, are not a `str`.
use puns::{
  Pun,
  layout::{Dfa, DfaState, Layout},
};

/// Any bytes, with the `id` of UTF-8.
const ANY: &Dfa =
  &Dfa { id: Dfa::UTF8.id, states: &[DfaState { accept: true, edges: &[(0, 0xff, 0)] }] };

#[repr(transparent)]
struct Lax([u8]);

// SAFETY: a type of bytes that `ANY` accepts: any.
unsafe impl Pun for Lax {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: the probe of a slice of bytes.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, ANY) };
}

fn main() {
  let lax: &Lax = puns::from_bytes(&[0xff]).unwrap();
  let _: &str = puns::pun_ref(lax);
}
