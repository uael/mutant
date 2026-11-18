//! The two types are the same but for what is two pointers down: a `u8` read as a `bool`. Only
//! the proof that follows the pointers, through the types, sees it.
mod a {
  use nearest::{Near, flat};

  #[flat]
  #[derive(puns::Pun)]
  pub struct List {
    pub head: u32,
    pub tail: Option<Near<List>>,
    pub leaves: Near<[Leaf]>,
  }

  #[flat]
  #[derive(puns::Pun)]
  pub struct Leaf {
    pub flag: u8,
    pub name: Near<str>,
  }
}

mod b {
  use nearest::{Near, flat};

  #[flat]
  #[derive(puns::Pun)]
  pub struct List {
    pub head: u32,
    pub tail: Option<Near<List>>,
    pub leaves: Near<[Leaf]>,
  }

  #[flat]
  #[derive(puns::Pun)]
  pub struct Leaf {
    pub flag: bool,
    pub name: Near<str>,
  }
}

fn main() {
  let region = nearest::Region::new(a::List::new(1, None, ()));
  let _: &b::List = puns::view_ref(&*region);
}
