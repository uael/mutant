//! What exceeds the limits of the evaluator is said where it is written, with the numbers.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a + (a + (a + (a + (a + (a + (a + (a + a))))))) > 0))]
struct Deep {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < 4, |at| at + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
  + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1) == 4))]
struct LongStep {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < 4, |at| at + 1) == a + a + a + a + a + a + a + a + a + a + a + a
  + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a))]
struct LongWalk {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ let b = 1; let c = 2; let d = 3; let e = 4; let f = 5; a + b + c + d + e + f > 0 }))]
struct Variables {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(size = magic)]
struct NotAnInteger {
  magic: [u8; 2],
  data: [u8],
}

fn main() {}
