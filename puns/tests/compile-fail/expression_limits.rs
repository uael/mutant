//! An expression has at most 256 operations, and nests at most 4 walks: the validation applies
//! each operation in an instance of a function of its own. The derive says where an expression
//! goes beyond, and by how much (here 257 operations: `a`, 126 times `+ 1`, `== a + 126`).
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(a + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 == a + 126))]
struct TooLong {
  a: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(
  walk(0, |i| i < 1 && walk(0, |j| j < 1 && walk(0, |k| k < 1 && walk(0, |l| l < 1
    && walk(0, |m| m < a, |m| m + 1) == a, |l| l + 1) == 1, |k| k + 1) == 1, |j| j + 1) == 1,
    |i| i + 1) == 1
))]
struct TooDeep {
  a: u8,
}

fn main() {}
