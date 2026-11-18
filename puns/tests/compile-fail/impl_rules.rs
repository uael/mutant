//! Validation goes by what implements `Rules` and `Steps` (the rules of a struct, and the
//! operations of each, which it applies): they are sealed, and what they say is checked against
//! the layout where they are used. A list that left a rule out would leave it unchecked.
struct Nothing;

impl puns::Rules for Nothing {
  const RULE: usize = usize::MAX;
  type Expr = ();
  type Next = ();
}

fn main() {}
