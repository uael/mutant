//! Pointers are followed through the lists of a type (`Fields`, `Variants`, `Elem`, `Pointee`),
//! which must be what its layout holds: here an implementation of `Pun` whose layout has a field,
//! a pointer, that its list of fields does not have. Were the list believed, nothing would look
//! at what that pointer aims at: a `u8` read as a `bool`.
use nearest::Near;
use puns::{
  Pun,
  layout::{Field, Layout},
};

#[derive(Pun)]
#[repr(C)]
struct Old {
  flag: Near<Near<u8>>,
}

#[repr(C)]
struct New {
  flag: Near<Near<bool>>,
}

// SAFETY: not kept: `Fields` is not the fields of the layout.
unsafe impl Pun for New {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Walks = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  // SAFETY: a sized type.
  const LAYOUT: Layout = unsafe {
    Layout::structure::<Self>(
      Self::PROBE,
      const { &Layout::sort_fields([Field::new(0, &<Near<Near<bool>> as Pun>::LAYOUT)]) },
      &[],
    )
  };
}

fn view(old: &Old) -> &New {
  puns::view_ref(old)
}

fn main() {
  let _: fn(&Old) -> &New = view;
}
