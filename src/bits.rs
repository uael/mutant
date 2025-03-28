pub trait Idx {
  type Inc: Idx;
  const USIZE: usize;
  const U128: u128 = Self::USIZE as u128;
}

pub trait Word {
  const BITS: usize;
  const U128: u128;
  type Push<const BIT: bool>: Word;
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

  pub struct Bit<const BIT: bool>;

  impl Word for () {
    const BITS: usize = 0;
    const U128: u128 = 0;
    type Push<const BIT: bool> = Bit<BIT>;
  }

  impl<const B: bool> Word for Bit<B> {
    const BITS: usize = 1;
    const U128: u128 = B as u128;
    type Push<const BIT: bool> = (Self, Bit<BIT>);
  }

  impl<Head: Word, const B: bool> Word for (Head, Bit<B>) {
    const BITS: usize = 1 + Head::BITS;
    const U128: u128 = ((B as u128) << Head::BITS) | Head::U128;
    type Push<const BIT: bool> = (Self, Bit<BIT>);
  }
};

pub struct Bit<T0, T1, const START: u8>(T0, T1);
pub struct Repeat<const N: usize, T, I = ()>(T, I);
pub struct Union<A, B>(A, B);

pub trait BitsPattern {
  const BITS: u128;
  const START: Option<u8>;
  const ANY: bool;
  type Push<U: BitsPattern>: BitsPattern;
  type Head: BitsPattern;
  type Tail: BitsPattern;
  type Jmp0: BitsPattern;
  type Jmp1: BitsPattern;
}

const _: () = {
  const fn max(a: u128, b: u128) -> u128 {
    if a > b { a } else { b }
  }

  impl BitsPattern for () {
    const BITS: u128 = 0;
    const START: Option<u8> = None;
    const ANY: bool = true;
    type Push<U: BitsPattern> = U;
    type Head = ();
    type Tail = ();
    type Jmp0 = ();
    type Jmp1 = ();
  }

  impl<T0: BitsPattern, T1: BitsPattern, const START: u8> BitsPattern for Bit<T0, T1, START> {
    const BITS: u128 = 1 + max(T0::BITS, T1::BITS);
    const START: Option<u8> = Some(START);
    const ANY: bool = START == 0b11 && T0::ANY && T1::ANY;
    type Push<U: BitsPattern> = (Self, U);
    type Head = ();
    type Tail = ();
    type Jmp0 = T0;
    type Jmp1 = T1;
  }

  impl<Head: BitsPattern, Tail: BitsPattern> BitsPattern for (Head, Tail) {
    const BITS: u128 = Head::BITS + Tail::BITS;
    const START: Option<u8> = Head::START;
    const ANY: bool = Head::ANY && Tail::ANY;
    type Push<U: BitsPattern> = (Head, Tail::Push<U>);
    type Head = Head;
    type Tail = Tail;
    type Jmp0 = (Head::Jmp0, Tail);
    type Jmp1 = (Head::Jmp1, Tail);
  }

  impl<const N: usize, T: BitsPattern, I: Idx> BitsPattern for Repeat<N, T, I> {
    const BITS: u128 = T::BITS * (N - I::USIZE) as u128;
    const START: Option<u8> = T::START;
    const ANY: bool = T::ANY;
    type Push<U: BitsPattern> = (Self, U);
    type Head = T;
    type Tail = Repeat<N, T, I::Inc>;
    type Jmp0 = (T::Jmp0, Repeat<N, T, I::Inc>);
    type Jmp1 = (T::Jmp1, Repeat<N, T, I::Inc>);
  }

  impl<A: BitsPattern, B: BitsPattern> BitsPattern for Union<A, B> {
    const BITS: u128 = max(A::BITS, B::BITS);
    const START: Option<u8> = match (A::START, B::START) {
      (Some(a), Some(b)) => Some(a | b),
      (Some(x), _) | (_, Some(x)) => Some(x),
      _ => None,
    };
    const ANY: bool = A::ANY && B::ANY;
    type Push<U: BitsPattern> = (Self, U);
    type Head = Union<A::Head, B::Head>;
    type Tail = Union<A::Tail, B::Tail>;
    type Jmp0 = Union<A::Jmp0, B::Jmp0>;
    type Jmp1 = Union<A::Jmp1, B::Jmp1>;
  }
};

type Any1 = Bit<(), (), 0b11>;
type Any2 = Bit<Any1, Any1, 0b11>;
type Any3 = Bit<Any2, Any2, 0b11>;
type Any4 = Bit<Any3, Any3, 0b11>;
type Any5 = Bit<Any4, Any4, 0b11>;
type Any6 = Bit<Any5, Any5, 0b11>;
type Any7 = Bit<Any6, Any6, 0b11>;
type Any8 = Bit<Any7, Any7, 0b11>;

type NonZero1 = Bit<(), (), 0b10>;
type NonZero2 = Bit<NonZero1, Any1, 0b11>;
type NonZero3 = Bit<NonZero2, Any2, 0b11>;
type NonZero4 = Bit<NonZero3, Any3, 0b11>;
type NonZero5 = Bit<NonZero4, Any4, 0b11>;
type NonZero6 = Bit<NonZero5, Any5, 0b11>;
type NonZero7 = Bit<NonZero6, Any6, 0b11>;
type NonZero8 = Bit<NonZero7, Any7, 0b11>;

type Zero1 = Bit<(), (), 0b01>;
type Zero2 = Bit<Zero1, (), 0b01>;
type Zero3 = Bit<Zero2, (), 0b01>;
type Zero4 = Bit<Zero3, (), 0b01>;
type Zero5 = Bit<Zero4, (), 0b01>;
type Zero6 = Bit<Zero5, (), 0b01>;
type Zero7 = Bit<Zero6, (), 0b01>;
// type Zero8 = Bit<Zero7, (), 0b01>;

type Bool = Bit<Zero7, Zero7, 0b11>;
// type Bool = Union<Equ8<0>, Equ8<1>>;

struct Equ8<const VAL: u8, I = ()>(I);
struct Equ16<const VAL: u16, I = ()>(I);
struct Gte8<const VAL: u8, I = ()>(I);
struct Gte16<const VAL: u16, I = ()>(I);

impl<const VAL: u8, I: Idx> BitsPattern for Equ8<VAL, I> {
  const BITS: u128 = 8 - I::U128;
  const START: Option<u8> = match I::USIZE {
    0..=7 => Some(1 << (VAL & (1 << I::USIZE) != 0) as u8),
    _ => None,
  };
  const ANY: bool = false;
  type Push<U: BitsPattern> = (Self, U);
  type Head = ();
  type Tail = ();
  type Jmp0 = Equ8<VAL, I::Inc>;
  type Jmp1 = Equ8<VAL, I::Inc>;
}

impl<const VAL: u16, I: Idx> BitsPattern for Equ16<VAL, I> {
  const BITS: u128 = 16 - I::U128;
  const START: Option<u8> = match I::USIZE {
    0..=15 => Some(1 << (VAL & (1 << I::USIZE) != 0) as u8),
    _ => None,
  };
  const ANY: bool = false;
  type Push<U: BitsPattern> = (Self, U);
  type Head = ();
  type Tail = ();
  type Jmp0 = Equ16<VAL, I::Inc>;
  type Jmp1 = Equ16<VAL, I::Inc>;
}

impl<const VAL: u8, W: Word> BitsPattern for Gte8<VAL, W> {
  const BITS: u128 = 8 - W::BITS as u128;
  const START: Option<u8> = match W::BITS {
    0..=7 => Some({
      let min = (<W::Push<false>>::U128 | (!0u128 << (W::BITS + 1))) as u8;
      let max = (<W::Push<true>>::U128 | (!0u128 << (W::BITS + 1))) as u8;
      match (min >= VAL, max >= VAL) {
        (false, false) => 0b00,  // accept no bits
        (false, true)  => 0b10,  // accept only 1
        (true, true)   => 0b11,  // accept both 0 and 1
        // Note: `(true, false)` is logically impossible
        _ => unreachable!(),
      }
    }),
    _ => None,
  };
  const ANY: bool = VAL == 0;
  type Push<U: BitsPattern> = (Self, U);
  type Head = ();
  type Tail = ();
  type Jmp0 = Gte8<VAL, W::Push<false>>;
  type Jmp1 = Gte8<VAL, W::Push<true>>;
}

impl<const VAL: u16, W: Word> BitsPattern for Gte16<VAL, W> {
  const BITS: u128 = 16 - W::BITS as u128;
  const START: Option<u8> = match W::BITS {
    0..=15 => Some({
      let min = (<W::Push<false>>::U128 | (!0u128 << (W::BITS + 1))) as u16;
      let max = (<W::Push<true>>::U128 | (!0u128 << (W::BITS + 1))) as u16;
      match (min >= VAL, max >= VAL) {
        (false, false) => 0b00,  // accept no bits
        (false, true)  => 0b10,  // accept only 1
        (true, true)   => 0b11,  // accept both 0 and 1
        // Note: `(true, false)` is logically impossible
        _ => unreachable!(),
      }
    }),
    _ => None,
  };
  const ANY: bool = VAL == 0;
  type Push<U: BitsPattern> = (Self, U);
  type Head = ();
  type Tail = ();
  type Jmp0 = Gte16<VAL, W::Push<false>>;
  type Jmp1 = Gte16<VAL, W::Push<true>>;
}

#[inline(always)]
pub const fn intersect<A: BitsPattern, B: BitsPattern>() -> u8 {
  const {
    if B::ANY {
      0b11
    } else if A::Head::BITS == 0 && A::Tail::BITS > 0 {
      // sequence: advance to the next state of `A`.
      intersect::<A::Tail, B>()
    } else if B::Head::BITS == 0 && B::Tail::BITS > 0 {
      // sequence: advance to the next state of `B`.
      intersect::<A, B::Tail>()
    } else if A::Head::BITS == B::Head::BITS && A::Tail::BITS > 0 {
      // sequence: non-overlapping states.
      intersect::<A::Head, B::Head>() & intersect::<A::Tail, B::Tail>()
    } else {
      match (A::START, B::START) {
        (None, _) | (_, None) => 0b11,
        (Some(a), Some(b)) => match a & b {
          0b00 => 0b00,
          i => {
            let mut res = if i == a { 0b11 } else { 0b01 };
            if i & (1 << 0) != 0 {
              res &= intersect::<A::Jmp0, B::Jmp0>();
            }
            if i & (1 << 1) != 0 {
              let tmp = intersect::<A::Jmp1, B::Jmp1>();
              res = (res & tmp) | (tmp != 0) as u8;
            }
            res
          }
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

  type Boolx2 = <Bool as BitsPattern>::Push<Bool>;
  type Boolx4 = <Boolx2 as BitsPattern>::Push<Boolx2>;
  type Boolx8 = <Boolx4 as BitsPattern>::Push<Boolx4>;

  #[test]
  fn test_intersect() {
    assert_eq!(0b11, intersect::<U8, U8>());
    assert_eq!(0b11, intersect::<Bool, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, Bool>());
    assert_eq!(0b11, intersect::<Bool, U8>());
    assert_eq!(0b01, intersect::<U8, NonZeroU8>());
    assert_eq!(0b11, intersect::<NonZeroU8, U8>());
    assert_eq!(0b01, intersect::<Bool, NonZeroU8>());
    assert_eq!(0b01, intersect::<NonZeroU8, Bool>());
    assert_eq!(0b11, intersect::<(Bool, Bool), (Bool, Bool)>());
    assert_eq!(0b01, intersect::<(Bool, (U8, ())), (Bool, (Bool, ()))>());
    assert_eq!(0b11, intersect::<Boolx8, Boolx8>());
    assert_eq!(0b11, intersect::<Equ16<0xabcd>, Equ16<0xabcd>>());

    assert_eq!(0b11, intersect::<Repeat<234324234, U8>, Repeat<234324234, U8>>());
    assert_eq!(0b11, intersect::<Repeat<12342, Bool>, Repeat<12342, Bool>>());
    assert_eq!(0b01, intersect::<Repeat<12342, U8>, Repeat<12342, Bool>>());
    assert_eq!(0b11, intersect::<Repeat<12342, Equ16<0xabcd>>, Repeat<12342, Equ16<0xabcd>>>());
    assert_eq!(0b11, intersect::<Repeat<1024, Boolx8>, Repeat<1024, Boolx8>>());
    assert_eq!(0b01, intersect::<(Repeat<1024, U8>, U8), (Repeat<1024, U8>, Bool)>());
  }

  #[derive(Debug, Copy, Clone)]
  enum Tree {
    Ok,
    It(&'static Tree),
    J0(&'static Tree),
    J1(&'static Tree),
    Br(&'static Tree, &'static Tree),
  }

  impl Tree {
    const fn equals(&self, other: &Self) -> bool {
      match (self, other) {
        (Self::Ok, Self::Ok) => true,
        (Self::It(a), Self::It(b)) => a.equals(b),
        (Self::J0(a), Self::J0(b)) => a.equals(b),
        (Self::J1(a), Self::J1(b)) => a.equals(b),
        (Self::Br(a0, a1), Self::Br(b0, b1)) => a0.equals(b0) && a1.equals(b1),
        _ => false,
      }
    }
  }

  #[derive(Copy, Clone)]
  enum Lut {
    Ok,
    Ko,
    It([&'static Lut; 256])
  }
  
  impl std::fmt::Debug for Lut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      match self {
        Self::Ok => write!(f, "Ok"),
        Self::Ko => write!(f, "Ko"),
        Self::It(jumps) => { 
          let mut d = f.debug_struct("Jmp");
          for (i, jump) in jumps.iter().enumerate() {
            if !matches!(jump, Self::Ko) {
              d.field(&format!("{:02x}", i), jump);
            }
          }
          d.finish()
        },
      }
    }
  }

  trait ConstTree {
    const TREE: &'static Tree;
  }

  struct TreeOf<A>(A);
  struct Intersection<A, B>(A, B);
  struct Next<A>(A);
  struct Lhs<A>(A);
  struct Rhs<A>(A);

  impl<A: BitsPattern> ConstTree for TreeOf<A> {
    const TREE: &'static Tree = tree::<A>();
  }

  impl<A: BitsPattern, B: BitsPattern> ConstTree for Intersection<A, B> {
    const TREE: &'static Tree = intersection_tree::<A, B>();
  }

  impl<A: ConstTree> ConstTree for Next<A> {
    const TREE: &'static Tree = match A::TREE {
      Tree::It(a) => a,
      Tree::J0(a) => a,
      Tree::J1(a) => a,
      _ => &Tree::Ok,
    };
  }

  impl<A: ConstTree> ConstTree for Lhs<A> {
    const TREE: &'static Tree = match A::TREE {
      Tree::Br(a, _) => a,
      _ => &Tree::Ok,
    };
  }

  impl<A: ConstTree> ConstTree for Rhs<A> {
    const TREE: &'static Tree = match A::TREE {
      Tree::Br(_, b) => b,
      _ => &Tree::Ok,
    };
  }

  const fn concat<A: ConstTree, B: ConstTree>() -> &'static Tree {
    &const {
      match A::TREE {
        Tree::Ok => *B::TREE,
        Tree::It(_) => Tree::It(concat::<Next<A>, B>()),
        Tree::J0(_) => Tree::J0(concat::<Next<A>, B>()),
        Tree::J1(_) => Tree::J1(concat::<Next<A>, B>()),
        Tree::Br(_, _) => Tree::Br(concat::<Lhs<A>, B>(), concat::<Rhs<A>, B>()),
      }
    }
  }

  const fn lut_inner<T: ConstTree, I: Idx, const BYTE: u8>() -> &'static Lut {
    &const {
      if I::USIZE == 8 {
        match T::TREE {
          Tree::Ok => Lut::Ok,
          _ => *lut::<T>()
        }
      } else {
        match T::TREE {
          Tree::Ok => Lut::Ok,
          Tree::It(a) => *lut_inner::<Next<T>, I::Inc, BYTE>(),
          Tree::J0(a) => if (BYTE >> I::USIZE) & 0b1 == 0 {
            *lut_inner::<Next<T>, I::Inc, BYTE>()
          } else {
            Lut::Ko
          },
          Tree::J1(a) => if (BYTE >> I::USIZE) & 0b1 == 1 {
            *lut_inner::<Next<T>, I::Inc, BYTE>()
          } else {
            Lut::Ko
          },
          Tree::Br(l, r) => if (BYTE >> I::USIZE) & 0b1 == 0 {
            *lut_inner::<Lhs<T>, I::Inc, BYTE>()
          } else {
            *lut_inner::<Rhs<T>, I::Inc, BYTE>()
          },
        }
      }
    }
  }

  const fn lut<T: ConstTree>() -> &'static Lut {
    &const {
      Lut::It([
        lut_inner::<T, (), 0x00>(), lut_inner::<T, (), 0x01>(), lut_inner::<T, (), 0x02>(), lut_inner::<T, (), 0x03>(),
        lut_inner::<T, (), 0x04>(), lut_inner::<T, (), 0x05>(), lut_inner::<T, (), 0x06>(), lut_inner::<T, (), 0x07>(),
        lut_inner::<T, (), 0x08>(), lut_inner::<T, (), 0x09>(), lut_inner::<T, (), 0x0a>(), lut_inner::<T, (), 0x0b>(),
        lut_inner::<T, (), 0x0c>(), lut_inner::<T, (), 0x0d>(), lut_inner::<T, (), 0x0e>(), lut_inner::<T, (), 0x0f>(),
        lut_inner::<T, (), 0x10>(), lut_inner::<T, (), 0x11>(), lut_inner::<T, (), 0x12>(), lut_inner::<T, (), 0x13>(),
        lut_inner::<T, (), 0x14>(), lut_inner::<T, (), 0x15>(), lut_inner::<T, (), 0x16>(), lut_inner::<T, (), 0x17>(),
        lut_inner::<T, (), 0x18>(), lut_inner::<T, (), 0x19>(), lut_inner::<T, (), 0x1a>(), lut_inner::<T, (), 0x1b>(),
        lut_inner::<T, (), 0x1c>(), lut_inner::<T, (), 0x1d>(), lut_inner::<T, (), 0x1e>(), lut_inner::<T, (), 0x1f>(),
        lut_inner::<T, (), 0x20>(), lut_inner::<T, (), 0x21>(), lut_inner::<T, (), 0x22>(), lut_inner::<T, (), 0x23>(),
        lut_inner::<T, (), 0x24>(), lut_inner::<T, (), 0x25>(), lut_inner::<T, (), 0x26>(), lut_inner::<T, (), 0x27>(),
        lut_inner::<T, (), 0x28>(), lut_inner::<T, (), 0x29>(), lut_inner::<T, (), 0x2a>(), lut_inner::<T, (), 0x2b>(),
        lut_inner::<T, (), 0x2c>(), lut_inner::<T, (), 0x2d>(), lut_inner::<T, (), 0x2e>(), lut_inner::<T, (), 0x2f>(),
        lut_inner::<T, (), 0x30>(), lut_inner::<T, (), 0x31>(), lut_inner::<T, (), 0x32>(), lut_inner::<T, (), 0x33>(),
        lut_inner::<T, (), 0x34>(), lut_inner::<T, (), 0x35>(), lut_inner::<T, (), 0x36>(), lut_inner::<T, (), 0x37>(),
        lut_inner::<T, (), 0x38>(), lut_inner::<T, (), 0x39>(), lut_inner::<T, (), 0x3a>(), lut_inner::<T, (), 0x3b>(),
        lut_inner::<T, (), 0x3c>(), lut_inner::<T, (), 0x3d>(), lut_inner::<T, (), 0x3e>(), lut_inner::<T, (), 0x3f>(),
        lut_inner::<T, (), 0x40>(), lut_inner::<T, (), 0x41>(), lut_inner::<T, (), 0x42>(), lut_inner::<T, (), 0x43>(),
        lut_inner::<T, (), 0x44>(), lut_inner::<T, (), 0x45>(), lut_inner::<T, (), 0x46>(), lut_inner::<T, (), 0x47>(),
        lut_inner::<T, (), 0x48>(), lut_inner::<T, (), 0x49>(), lut_inner::<T, (), 0x4a>(), lut_inner::<T, (), 0x4b>(),
        lut_inner::<T, (), 0x4c>(), lut_inner::<T, (), 0x4d>(), lut_inner::<T, (), 0x4e>(), lut_inner::<T, (), 0x4f>(),
        lut_inner::<T, (), 0x50>(), lut_inner::<T, (), 0x51>(), lut_inner::<T, (), 0x52>(), lut_inner::<T, (), 0x53>(),
        lut_inner::<T, (), 0x54>(), lut_inner::<T, (), 0x55>(), lut_inner::<T, (), 0x56>(), lut_inner::<T, (), 0x57>(),
        lut_inner::<T, (), 0x58>(), lut_inner::<T, (), 0x59>(), lut_inner::<T, (), 0x5a>(), lut_inner::<T, (), 0x5b>(),
        lut_inner::<T, (), 0x5c>(), lut_inner::<T, (), 0x5d>(), lut_inner::<T, (), 0x5e>(), lut_inner::<T, (), 0x5f>(),
        lut_inner::<T, (), 0x60>(), lut_inner::<T, (), 0x61>(), lut_inner::<T, (), 0x62>(), lut_inner::<T, (), 0x63>(),
        lut_inner::<T, (), 0x64>(), lut_inner::<T, (), 0x65>(), lut_inner::<T, (), 0x66>(), lut_inner::<T, (), 0x67>(),
        lut_inner::<T, (), 0x68>(), lut_inner::<T, (), 0x69>(), lut_inner::<T, (), 0x6a>(), lut_inner::<T, (), 0x6b>(),
        lut_inner::<T, (), 0x6c>(), lut_inner::<T, (), 0x6d>(), lut_inner::<T, (), 0x6e>(), lut_inner::<T, (), 0x6f>(),
        lut_inner::<T, (), 0x70>(), lut_inner::<T, (), 0x71>(), lut_inner::<T, (), 0x72>(), lut_inner::<T, (), 0x73>(),
        lut_inner::<T, (), 0x74>(), lut_inner::<T, (), 0x75>(), lut_inner::<T, (), 0x76>(), lut_inner::<T, (), 0x77>(),
        lut_inner::<T, (), 0x78>(), lut_inner::<T, (), 0x79>(), lut_inner::<T, (), 0x7a>(), lut_inner::<T, (), 0x7b>(),
        lut_inner::<T, (), 0x7c>(), lut_inner::<T, (), 0x7d>(), lut_inner::<T, (), 0x7e>(), lut_inner::<T, (), 0x7f>(),
        lut_inner::<T, (), 0x80>(), lut_inner::<T, (), 0x81>(), lut_inner::<T, (), 0x82>(), lut_inner::<T, (), 0x83>(),
        lut_inner::<T, (), 0x84>(), lut_inner::<T, (), 0x85>(), lut_inner::<T, (), 0x86>(), lut_inner::<T, (), 0x87>(),
        lut_inner::<T, (), 0x88>(), lut_inner::<T, (), 0x89>(), lut_inner::<T, (), 0x8a>(), lut_inner::<T, (), 0x8b>(),
        lut_inner::<T, (), 0x8c>(), lut_inner::<T, (), 0x8d>(), lut_inner::<T, (), 0x8e>(), lut_inner::<T, (), 0x8f>(),
        lut_inner::<T, (), 0x90>(), lut_inner::<T, (), 0x91>(), lut_inner::<T, (), 0x92>(), lut_inner::<T, (), 0x93>(),
        lut_inner::<T, (), 0x94>(), lut_inner::<T, (), 0x95>(), lut_inner::<T, (), 0x96>(), lut_inner::<T, (), 0x97>(),
        lut_inner::<T, (), 0x98>(), lut_inner::<T, (), 0x99>(), lut_inner::<T, (), 0x9a>(), lut_inner::<T, (), 0x9b>(),
        lut_inner::<T, (), 0x9c>(), lut_inner::<T, (), 0x9d>(), lut_inner::<T, (), 0x9e>(), lut_inner::<T, (), 0x9f>(),
        lut_inner::<T, (), 0xa0>(), lut_inner::<T, (), 0xa1>(), lut_inner::<T, (), 0xa2>(), lut_inner::<T, (), 0xa3>(),
        lut_inner::<T, (), 0xa4>(), lut_inner::<T, (), 0xa5>(), lut_inner::<T, (), 0xa6>(), lut_inner::<T, (), 0xa7>(),
        lut_inner::<T, (), 0xa8>(), lut_inner::<T, (), 0xa9>(), lut_inner::<T, (), 0xaa>(), lut_inner::<T, (), 0xab>(),
        lut_inner::<T, (), 0xac>(), lut_inner::<T, (), 0xad>(), lut_inner::<T, (), 0xae>(), lut_inner::<T, (), 0xaf>(),
        lut_inner::<T, (), 0xb0>(), lut_inner::<T, (), 0xb1>(), lut_inner::<T, (), 0xb2>(), lut_inner::<T, (), 0xb3>(),
        lut_inner::<T, (), 0xb4>(), lut_inner::<T, (), 0xb5>(), lut_inner::<T, (), 0xb6>(), lut_inner::<T, (), 0xb7>(),
        lut_inner::<T, (), 0xb8>(), lut_inner::<T, (), 0xb9>(), lut_inner::<T, (), 0xba>(), lut_inner::<T, (), 0xbb>(),
        lut_inner::<T, (), 0xbc>(), lut_inner::<T, (), 0xbd>(), lut_inner::<T, (), 0xbe>(), lut_inner::<T, (), 0xbf>(),
        lut_inner::<T, (), 0xc0>(), lut_inner::<T, (), 0xc1>(), lut_inner::<T, (), 0xc2>(), lut_inner::<T, (), 0xc3>(),
        lut_inner::<T, (), 0xc4>(), lut_inner::<T, (), 0xc5>(), lut_inner::<T, (), 0xc6>(), lut_inner::<T, (), 0xc7>(),
        lut_inner::<T, (), 0xc8>(), lut_inner::<T, (), 0xc9>(), lut_inner::<T, (), 0xca>(), lut_inner::<T, (), 0xcb>(),
        lut_inner::<T, (), 0xcc>(), lut_inner::<T, (), 0xcd>(), lut_inner::<T, (), 0xce>(), lut_inner::<T, (), 0xcf>(),
        lut_inner::<T, (), 0xd0>(), lut_inner::<T, (), 0xd1>(), lut_inner::<T, (), 0xd2>(), lut_inner::<T, (), 0xd3>(),
        lut_inner::<T, (), 0xd4>(), lut_inner::<T, (), 0xd5>(), lut_inner::<T, (), 0xd6>(), lut_inner::<T, (), 0xd7>(),
        lut_inner::<T, (), 0xd8>(), lut_inner::<T, (), 0xd9>(), lut_inner::<T, (), 0xda>(), lut_inner::<T, (), 0xdb>(),
        lut_inner::<T, (), 0xdc>(), lut_inner::<T, (), 0xdd>(), lut_inner::<T, (), 0xde>(), lut_inner::<T, (), 0xdf>(),
        lut_inner::<T, (), 0xe0>(), lut_inner::<T, (), 0xe1>(), lut_inner::<T, (), 0xe2>(), lut_inner::<T, (), 0xe3>(),
        lut_inner::<T, (), 0xe4>(), lut_inner::<T, (), 0xe5>(), lut_inner::<T, (), 0xe6>(), lut_inner::<T, (), 0xe7>(),
        lut_inner::<T, (), 0xe8>(), lut_inner::<T, (), 0xe9>(), lut_inner::<T, (), 0xea>(), lut_inner::<T, (), 0xeb>(),
        lut_inner::<T, (), 0xec>(), lut_inner::<T, (), 0xed>(), lut_inner::<T, (), 0xee>(), lut_inner::<T, (), 0xef>(),
        lut_inner::<T, (), 0xf0>(), lut_inner::<T, (), 0xf1>(), lut_inner::<T, (), 0xf2>(), lut_inner::<T, (), 0xf3>(),
        lut_inner::<T, (), 0xf4>(), lut_inner::<T, (), 0xf5>(), lut_inner::<T, (), 0xf6>(), lut_inner::<T, (), 0xf7>(),
        lut_inner::<T, (), 0xf8>(), lut_inner::<T, (), 0xf9>(), lut_inner::<T, (), 0xfa>(), lut_inner::<T, (), 0xfb>(),
        lut_inner::<T, (), 0xfc>(), lut_inner::<T, (), 0xfd>(), lut_inner::<T, (), 0xfe>(), lut_inner::<T, (), 0xff>(),
      ])
    }
  }
  
  #[inline(always)]
  const fn intersection_tree<A: BitsPattern, B: BitsPattern>() -> &'static Tree {
    &const {
      if A::Head::BITS == 0 && A::Tail::BITS > 0 {
        // sequence: advance to the next `A` state.
        *intersection_tree::<A::Tail, B>()
      } else if B::Head::BITS == 0 && B::Tail::BITS > 0 {
        // sequence: advance to the next `B` state.
        *intersection_tree::<A, B::Tail>()
      } else if A::Head::BITS == B::Head::BITS && A::Tail::BITS > 0 {
        // sequence: non-overlapping states.
        *concat::<Intersection<A::Head, B::Head>, Intersection<A::Tail, B::Tail>>()
      } else {
        match (A::START, B::START) {
          (None, _) | (_, None) => Tree::Ok,
          (Some(a), Some(b)) => loop {
            let i = a & b;
            if i == 0 {
              break Tree::Ok;
            }
            if i == 0b11 {
              let l = intersection_tree::<A::Jmp0, B::Jmp0>();
              let r = intersection_tree::<A::Jmp1, B::Jmp1>();
              if l.equals(r) {
                break Tree::It(l);
              }
              break Tree::Br(l, r);
            }
            if i & (1 << 0) != 0 {
              break Tree::J0(intersection_tree::<A::Jmp0, B::Jmp0>());
            }
            break Tree::J1(intersection_tree::<A::Jmp1, B::Jmp1>());
          },
        }
      }
    }
  }
  
  #[inline(always)]
  const fn tree<A: BitsPattern>() -> &'static Tree {
    &const {
      if A::Head::BITS == 0 && A::Tail::BITS > 0 {
        // sequence: advance to the next `A` state.
        *tree::<A::Tail>()
      } else if A::Head::BITS > 0 && A::Tail::BITS > 0 {
        // sequence: non-overlapping states.
        *concat::<TreeOf<A::Head>, TreeOf<A::Tail>>()
      } else {
        match A::START {
          None => Tree::Ok,
          Some(i) => loop {
            if i == 0 {
              break Tree::Ok;
            }
            if i == 0b11 {
              let l = tree::<A::Jmp0>();
              let r = tree::<A::Jmp1>();
              if l.equals(r) {
                break Tree::It(l);
              }
              break Tree::Br(l, r);
            }
            if i & (1 << 0) != 0 {
              break Tree::J0(tree::<A::Jmp0>());
            }
            break Tree::J1(tree::<A::Jmp1>());
          },
        }
      }
    }
  }

  #[test]
  fn test_tree() {
    // println!("{:?}", intersection_tree::<Any1, Any1>());
    // println!("{:?}", intersection_tree::<Zero1, Any1>());
    // println!("{:?}", intersection_tree::<Repeat<8, Any2>, Repeat<8, Any2>>());
    // println!("{:?}", intersection_tree::<Repeat<8, Zero2>, Repeat<8, Any2>>());
    // println!("{:?}", intersection_tree::<Repeat<8, Bool>, Repeat<8, U8>>());
    // println!("{:?}", intersection_tree::<Bool, Bool>());
    // println!(
    //   "{:b} <=> {:?}",
    //   0xab,
    //   intersection_tree::<Repeat<3, Equ8<0xab>>, Repeat<3, Equ8<0xab>>>()
    // );
    // println!("{:?}", intersection_tree::<Repeat<66, U8>, Repeat<66, U8>>());
    println!("{:?}", tree::<Repeat<4, Bool>>());
    println!("{:#?}", lut::<TreeOf<Repeat<4, Bool>>>());
    println!("{:#?}", lut::<TreeOf<Equ8<0xab>>>());
    println!("{:#?}", lut::<TreeOf<Equ16<0xabcd>>>());
    println!("{:#?}", lut::<TreeOf<Gte8<0xab>>>());
    println!("{:#?}", lut::<TreeOf<Gte8<0xff>>>());
    println!("{:#?}", lut::<TreeOf<Gte8<0xfa>>>());
    println!("{:#?}", lut::<TreeOf<Gte16<0xfffd>>>());
    // println!("{:?}", intersection_tree::<Bool, U8>());
    // println!("{:?}", intersection_tree::<U8, Bool>());
    // println!("{:?}", intersection_tree::<Bool, U8>());
    // println!("{:?}", intersection_tree::<U8, NonZeroU8>());
    // println!("{:?}", intersection_tree::<NonZeroU8, U8>());
    // println!("{:?}", intersection_tree::<Bool, NonZeroU8>());
    // println!("{:?}", intersection_tree::<NonZeroU8, Bool>());
    // println!("{:?}", intersection_tree::<(Bool, Bool), (Bool, Bool)>());
    // println!("{:?}", intersection_tree::<(Bool, (U8, ())), (Bool, (Bool, ()))>());
    // println!("{:?}", intersection_tree::<Boolx8, Boolx8>());
  }
}
