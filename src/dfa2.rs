use crate::byteset::ByteSet;

pub trait Idx {
  type Inc: Idx;
  const USIZE: usize;
}

pub trait Mem {
  const U128: u128;
  const DIGITS: usize;
  type Push<const DIGIT: u8>: Mem;
}

const _: () = {
  impl Idx for () {
    type Inc = ((),);
    const USIZE: usize = 0;
  }

  impl Idx for ((),) {
    type Inc = (((),), ());
    const USIZE: usize = 1;
  }

  impl<U: Idx> Idx for (U, ()) {
    type Inc = (U, ((),));
    const USIZE: usize = 0 | (U::USIZE << 1);
  }

  impl<U: Idx> Idx for (U, ((),)) {
    type Inc = (U::Inc, ());
    const USIZE: usize = 1 | (U::USIZE << 1);
  }

  pub struct Digit<const DIGIT: u8>;

  impl Mem for () {
    const U128: u128 = 0;
    const DIGITS: usize = 0;
    type Push<const DIGIT: u8> = Digit<DIGIT>;
  }

  impl<const A: u8> Mem for Digit<A> {
    const U128: u128 = A as u128;
    const DIGITS: usize = 1;
    type Push<const DIGIT: u8> = (Self, Digit<DIGIT>);
  }

  impl<I: Mem, const A: u8> Mem for (I, Digit<A>) {
    const U128: u128 = ((A as u128) << (I::DIGITS * 8)) | I::U128;
    const DIGITS: usize = 1 + I::DIGITS;
    type Push<const DIGIT: u8> = (Self, Digit<DIGIT>);
  }
};

pub trait Dfa {
  const DIGITS: usize;
  const START: Option<ByteSet>;
  const ANY: bool;
  type Transition<const BYTE: u8>: Dfa;
  type Push<U: Dfa>: Dfa;
  type Head: Dfa;
  type Tail: Dfa;
}

pub struct Any8;
pub struct Equ8<const VAL: u8>;
pub struct Equ16<const VAL: u16>;
pub struct Neq8<const VAL: u8>;
pub struct Neq16<const VAL: u8>;
pub struct Union<A, B>(A, B);
pub struct Intersection<A, B>(A, B);
pub struct Repeat<const N: usize, T, I = ()>(T, I);

const _: () = {
  impl Dfa for () {
    const DIGITS: usize = 0;
    const START: Option<ByteSet> = None;
    const ANY: bool = true;
    type Transition<const BYTE: u8> = ();
    type Push<U: Dfa> = U;
    type Head = ();
    type Tail = ();
  }

  impl<Head: Dfa, Tail: Dfa> Dfa for (Head, Tail) {
    const DIGITS: usize = Head::DIGITS + Tail::DIGITS;
    const START: Option<ByteSet> = Head::START;
    const ANY: bool = Head::ANY && Tail::ANY;
    type Transition<const BYTE: u8> = (Head::Transition<BYTE>, Tail);
    type Push<U: Dfa> = (Head, Tail::Push<U>);
    type Head = Head;
    type Tail = Tail;
  }

  impl<const N: usize, T: Dfa, I: Idx> Dfa for Repeat<N, T, I> {
    const DIGITS: usize = T::DIGITS * (N - I::USIZE);
    const START: Option<ByteSet> = T::START;
    const ANY: bool = T::ANY;
    type Transition<const BYTE: u8> = (T::Transition<BYTE>, Repeat<N, T, I::Inc>);
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }

  impl Dfa for Any8 {
    const DIGITS: usize = 1;
    const START: Option<ByteSet> = Some(ByteSet::from_parts([!0; 2]));
    const ANY: bool = true;
    type Transition<const BYTE: u8> = ();
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }

  impl<const VAL: u8> Dfa for Equ8<VAL> {
    const DIGITS: usize = 1;
    const START: Option<ByteSet> = Some(ByteSet::from_bytes(&[VAL]));
    const ANY: bool = false;
    type Transition<const BYTE: u8> = ();
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }

  impl<const VAL: u8> Dfa for Neq8<VAL> {
    const DIGITS: usize = 1;
    const START: Option<ByteSet> = Some(ByteSet::from_bytes(&[VAL]).not());
    const ANY: bool = false;
    type Transition<const BYTE: u8> = ();
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }
  
  impl<A: Dfa, B: Dfa> Dfa for Union<A, B> {
    const DIGITS: usize = {
      assert!(A::DIGITS == B::DIGITS);
      A::DIGITS
    };
    const START: Option<ByteSet> = match (A::START, B::START) {
      (Some(a), Some(b)) => Some(a.or(b)),
      (Some(x), _) | (_, Some(x)) => Some(x),
      _ => None,
    };
    const ANY: bool = A::ANY && B::ANY;
    type Transition<const BYTE: u8> = Union<A::Transition<BYTE>, B::Transition<BYTE>>;
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }

  impl<A: Dfa, B: Dfa> Dfa for Intersection<A, B> {
    const DIGITS: usize = {
      assert!(A::DIGITS == B::DIGITS);
      A::DIGITS
    };
    const START: Option<ByteSet> = match (A::START, B::START) {
      (Some(a), Some(b)) => Some(a.and(b)),
      _ => None,
    };
    const ANY: bool = A::ANY && B::ANY;
    type Transition<const BYTE: u8> = Intersection<A::Transition<BYTE>, B::Transition<BYTE>>;
    type Push<U: Dfa> = (Self, U);
    type Head = ();
    type Tail = ();
  }
};

type NonZero8 = Neq8<8>;

type Bool = Union<Equ8<0>, Equ8<1>>;

macro_rules! inline {
  { $( ($( $args:tt )*) ),* $(,)?; ($( $params:tt )*) $( $code:tt )+ } => {
    macro_rules! __dummy__ { ($( $params )*) => { $( $code )+ }; }
    $( __dummy__! { $( $args )* } )*
  }
}

#[inline(always)]
pub const fn intersect<A: Dfa, B: Dfa>() -> u8 {
  #[inline(always)]
  const fn or(a: u8, b: u8) -> u8 {
    (a & b) | (a != 0) as u8 | (b != 0) as u8
  }

  const {
    if B::ANY {
      0b11
    } else if A::Head::DIGITS == 0 && A::Tail::DIGITS > 0 {
      // sequence: advance to the next `A` state.
      intersect::<A::Tail, B>()
    } else if B::Head::DIGITS == 0 && B::Tail::DIGITS > 0 {
      // sequence: advance to the next `B` state.
      intersect::<A, B::Tail>()
    } else if A::Head::DIGITS == B::Head::DIGITS && A::Tail::DIGITS > 0 {
      // sequence: non-overlapping states.
      intersect::<A::Head, B::Head>() & intersect::<A::Tail, B::Tail>()
    } else {
      match (A::START, B::START) {
        (None, _) | (_, None) => 0b11,
        (Some(a), Some(b)) => loop {
          let i = a.and(b);
          if i.is_empty() { break 0b00 }
          let mut res = if i.equals(&a) { 0b11 } else { 0b01 };
          inline! {
            (0x00), (0x01), (0x02), (0x03), (0x04), (0x05), (0x06), (0x07), (0x08), (0x09), (0x0A),
            (0x0B), (0x0C), (0x0D), (0x0E), (0x0F), (0x10), (0x11), (0x12), (0x13), (0x14), (0x15),
            (0x16), (0x17), (0x18), (0x19), (0x1A), (0x1B), (0x1C), (0x1D), (0x1E), (0x1F), (0x20),
            (0x21), (0x22), (0x23), (0x24), (0x25), (0x26), (0x27), (0x28), (0x29), (0x2A), (0x2B),
            (0x2C), (0x2D), (0x2E), (0x2F), (0x30), (0x31), (0x32), (0x33), (0x34), (0x35), (0x36),
            (0x37), (0x38), (0x39), (0x3A), (0x3B), (0x3C), (0x3D), (0x3E), (0x3F), (0x40), (0x41),
            (0x42), (0x43), (0x44), (0x45), (0x46), (0x47), (0x48), (0x49), (0x4A), (0x4B), (0x4C),
            (0x4D), (0x4E), (0x4F), (0x50), (0x51), (0x52), (0x53), (0x54), (0x55), (0x56), (0x57),
            (0x58), (0x59), (0x5A), (0x5B), (0x5C), (0x5D), (0x5E), (0x5F), (0x60), (0x61), (0x62),
            (0x63), (0x64), (0x65), (0x66), (0x67), (0x68), (0x69), (0x6A), (0x6B), (0x6C), (0x6D),
            (0x6E), (0x6F), (0x70), (0x71), (0x72), (0x73), (0x74), (0x75), (0x76), (0x77), (0x78),
            (0x79), (0x7A), (0x7B), (0x7C), (0x7D), (0x7E), (0x7F), (0x80), (0x81), (0x82), (0x83),
            (0x84), (0x85), (0x86), (0x87), (0x88), (0x89), (0x8A), (0x8B), (0x8C), (0x8D), (0x8E),
            (0x8F), (0x90), (0x91), (0x92), (0x93), (0x94), (0x95), (0x96), (0x97), (0x98), (0x99),
            (0x9A), (0x9B), (0x9C), (0x9D), (0x9E), (0x9F), (0xA0), (0xA1), (0xA2), (0xA3), (0xA4),
            (0xA5), (0xA6), (0xA7), (0xA8), (0xA9), (0xAA), (0xAB), (0xAC), (0xAD), (0xAE), (0xAF),
            (0xB0), (0xB1), (0xB2), (0xB3), (0xB4), (0xB5), (0xB6), (0xB7), (0xB8), (0xB9), (0xBA),
            (0xBB), (0xBC), (0xBD), (0xBE), (0xBF), (0xC0), (0xC1), (0xC2), (0xC3), (0xC4), (0xC5),
            (0xC6), (0xC7), (0xC8), (0xC9), (0xCA), (0xCB), (0xCC), (0xCD), (0xCE), (0xCF), (0xD0),
            (0xD1), (0xD2), (0xD3), (0xD4), (0xD5), (0xD6), (0xD7), (0xD8), (0xD9), (0xDA), (0xDB),
            (0xDC), (0xDD), (0xDE), (0xDF), (0xE0), (0xE1), (0xE2), (0xE3), (0xE4), (0xE5), (0xE6),
            (0xE7), (0xE8), (0xE9), (0xEA), (0xEB), (0xEC), (0xED), (0xEE), (0xEF), (0xF0), (0xF1),
            (0xF2), (0xF3), (0xF4), (0xF5), (0xF6), (0xF7), (0xF8), (0xF9), (0xFA), (0xFB), (0xFC),
            (0xFD), (0xFE), (0xFF)
            ; ($byte:tt) if i.is_set($byte) {
              res = or(res, intersect::<A::Transition<$byte>, B::Transition<$byte>>());
            }
          }
          break res
        },
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  type U8 = Any8;
  type NonZeroU8 = NonZero8;

  type Boolx2 = <Bool as Dfa>::Push<Bool>;
  type Boolx4 = <Boolx2 as Dfa>::Push<Boolx2>;
  type Boolx8 = <Boolx4 as Dfa>::Push<Boolx4>;

  #[test]
  fn test_intersect() {
    assert_eq!(0b11, intersect::<U8, U8>());
    assert_eq!(0b11, intersect::<Bool, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    assert_eq!(0b11, intersect::<NonZeroU8, U8>());
    assert_eq!(0b11, intersect::<Bool, NonZeroU8>());
    assert_eq!(0b01, intersect::<NonZeroU8, Bool>());
    assert_eq!(0b11, intersect::<(Bool, Bool), (Bool, Bool)>());
    assert_eq!(0b01, intersect::<(Bool, (U8, ())), (Bool, (Bool, ()))>());
    assert_eq!(0b11, intersect::<Boolx8, Boolx8>());

    assert_eq!(0b11, intersect::<Repeat<234324234, U8>, Repeat<234324234, U8>>());
    assert_eq!(0b11, intersect::<Repeat<12342, Bool>, Repeat<12342, Bool>>());
    assert_eq!(0b11, intersect::<Repeat<1024, Boolx8>, Repeat<1024, Boolx8>>());
    assert_eq!(0b01, intersect::<(Repeat<1024, U8>, U8), (Repeat<1024, U8>, Bool)>());
  }

  // #[derive(Debug, Copy, Clone)]
  // enum Tree {
  //   Ok,
  //   It(&'static Tree),
  //   J0(&'static Tree),
  //   J1(&'static Tree),
  //   Br(&'static Tree, &'static Tree),
  //   // And(&'static Tree, &'static Tree),
  // }
  // 
  // impl Tree {
  //   const fn equals(&self, other: &Self) -> bool {
  //     match (self, other) {
  //       (Self::Ok, Self::Ok) => true,
  //       (Self::It(a), Self::It(b)) => a.equals(b),
  //       (Self::J0(a), Self::J0(b)) => a.equals(b),
  //       (Self::J1(a), Self::J1(b)) => a.equals(b),
  //       (Self::Br(a0, a1), Self::Br(b0, b1)) => a0.equals(b0) && a1.equals(b1),
  //       // (Self::And(a0, a1), Self::And(b0, b1)) => a0.equals(b0) && a1.equals(b1),
  //       _ => false,
  //     }
  //   }
  // }
  // 
  // trait ConstTree {
  //   const TREE: &'static Tree;
  // }
  // 
  // struct Intersection<A, B>(A, B);
  // struct Next<A>(A);
  // struct Lhs<A>(A);
  // struct Rhs<A>(A);
  // 
  // impl<A: Dfa, B: Dfa> ConstTree for Intersection<A, B> {
  //   const TREE: &'static Tree = intersection_tree::<A, B>();
  // }
  // 
  // impl<A: ConstTree> ConstTree for Next<A> {
  //   const TREE: &'static Tree = match A::TREE {
  //     Tree::It(a) => a,
  //     Tree::J0(a) => a,
  //     Tree::J1(a) => a,
  //     _ => &Tree::Ok,
  //   };
  // }
  // 
  // impl<A: ConstTree> ConstTree for Lhs<A> {
  //   const TREE: &'static Tree = match A::TREE {
  //     Tree::Br(a, _) => a,
  //     _ => &Tree::Ok,
  //   };
  // }
  // 
  // impl<A: ConstTree> ConstTree for Rhs<A> {
  //   const TREE: &'static Tree = match A::TREE {
  //     Tree::Br(_, b) => b,
  //     _ => &Tree::Ok,
  //   };
  // }
  // 
  // pub const fn concat<A: ConstTree, B: ConstTree>() -> &'static Tree {
  //   &const {
  //     match A::TREE {
  //       Tree::Ok => *B::TREE,
  //       Tree::It(_) => Tree::It(concat::<Next<A>, B>()),
  //       Tree::J0(_) => Tree::J0(concat::<Next<A>, B>()),
  //       Tree::J1(_) => Tree::J1(concat::<Next<A>, B>()),
  //       Tree::Br(_, _) => Tree::Br(concat::<Lhs<A>, B>(), concat::<Rhs<A>, B>()),
  //     }
  //   }
  // }
  // 
  // #[inline(always)]
  // pub const fn intersection_tree<A: Dfa, B: Dfa>() -> &'static Tree {
  //   &const {
  //     if A::Head::DIGITS == 0 && A::Tail::DIGITS > 0 {
  //       // sequence: advance to the next `A` state.
  //       *intersection_tree::<A::Tail, B>()
  //     } else if B::Head::DIGITS == 0 && B::Tail::DIGITS > 0 {
  //       // sequence: advance to the next `B` state.
  //       *intersection_tree::<A, B::Tail>()
  //     } else if A::Head::DIGITS == B::Head::DIGITS && A::Tail::DIGITS > 0 {
  //       // sequence: non-overlapping states.
  //       *concat::<Intersection<A::Head, B::Head>, Intersection<A::Tail, B::Tail>>()
  //     } else {
  //       match (A::START, B::START) {
  //         (None, _) | (_, None) => Tree::Ok,
  //         (Some(a), Some(b)) => loop {
  //           let i = a & b;
  //           if i == 0 { break Tree::Ok }
  //           if i == 0b11 {
  //             let l = intersection_tree::<A::Jmp0, B::Jmp0>();
  //             let r = intersection_tree::<A::Jmp1, B::Jmp1>();
  //             if l.equals(r) {
  //               break Tree::It(l)
  //             }
  //             break Tree::Br(l, r)
  //           }
  //           if i & (1 << 0) != 0 {
  //             break Tree::J0(intersection_tree::<A::Jmp0, B::Jmp0>())
  //           }
  //           break Tree::J1(intersection_tree::<A::Jmp1, B::Jmp1>())
  //         },
  //       }
  //     }
  //   }
  // }
  // 
  // #[test]
  // fn test_tree() {
  //   println!("{:?}", intersection_tree::<Any1, Any1>());
  //   println!("{:?}", intersection_tree::<Zero1, Any1>());
  //   println!("{:?}", intersection_tree::<Repeat<8, Any2>, Repeat<8, Any2>>());
  //   println!("{:?}", intersection_tree::<Repeat<8, Zero2>, Repeat<8, Any2>>());
  //   println!("{:?}", intersection_tree::<Repeat<8, Bool>, Repeat<8, U8>>());
  //   println!("{:?}", intersection_tree::<Bool, Bool>());
  //   // println!("{:?}", intersection_tree::<Bool, U8>());
  //   // println!("{:?}", intersection_tree::<U8, Bool>());
  //   // println!("{:?}", intersection_tree::<Bool, U8>());
  //   // println!("{:?}", intersection_tree::<U8, NonZeroU8>());
  //   // println!("{:?}", intersection_tree::<NonZeroU8, U8>());
  //   // println!("{:?}", intersection_tree::<Bool, NonZeroU8>());
  //   // println!("{:?}", intersection_tree::<NonZeroU8, Bool>());
  //   // println!("{:?}", intersection_tree::<(Bool, Bool), (Bool, Bool)>());
  //   // println!("{:?}", intersection_tree::<(Bool, (U8, ())), (Bool, (Bool, ()))>());
  //   // println!("{:?}", intersection_tree::<Boolx8, Boolx8>());
  // }
}
