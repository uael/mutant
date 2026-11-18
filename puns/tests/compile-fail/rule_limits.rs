//! What exceeds the limits of the evaluator is said where it is written, with the numbers.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a + (a)))))))))))))))) > 0))]
struct Deep {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({
  let b = 1; let c = 2; let d = 3; let e = 4; let f = 5; let g = 6; let h = 7; let i = 8; let j = 9;
  a + b + c + d + e + f + g + h + i + j > 0
}))]
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
