use std::collections::HashMap as Map;
use std::collections::HashSet as Set;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
enum Byte {
  Init { start: u8, end: u8 },
  Uninit,
}

impl Byte {
  fn contains(&self, other: &Self) -> bool {
    match (self, other) {
      (Byte::Init { start: al, end: ar }, Byte::Init { start: bl, end: br }) => {
        bl >= al && br <= ar
      }
      _ => false,
    }
  }

  fn intersect(&self, other: &Self) -> bool {
    match (self, other) {
      (Byte::Init { start: al, end: ar }, Byte::Init { start: bl, end: br }) => {
        if al < bl {
          if ar < bl {
            return false;
          }
        } else if br < al {
          return false;
        }
        true
      }
      _ => false,
    }
  }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Tree {
  /// A sequence of successive layouts.
  Seq(Vec<Self>),
  /// A choice between alternative layouts.
  Alt(Vec<Self>),
  /// A byte node.
  Byte(Byte),
}

/// A non-deterministic finite automaton (NFA) that represents the layout of a type.
/// The transmutability of two given types is computed by comparing their `Nfa`s.
#[derive(PartialEq, Debug)]
struct Nfa {
  transitions: Map<usize, Map<Byte, Set<usize>>>,
  start: usize,
  accepting: usize,
}

impl Nfa {
  fn unit(off: usize) -> Self {
    let transitions: Map<usize, Map<Byte, Set<usize>>> = Map::default();
    let start = off;
    let accepting = start;

    Nfa { transitions, start, accepting }
  }

  fn from_byte(byte: Byte, off: &mut usize) -> Self {
    let mut transitions: Map<usize, Map<Byte, Set<usize>>> = Map::default();
    let start = *off;
    *off += 1;
    let accepting = *off;

    let source = transitions.entry(start).or_default();
    let edge = source.entry(byte).or_default();
    edge.insert(accepting);

    Nfa { transitions, start, accepting }
  }

  fn from_tree(tree: Tree, off: &mut usize) -> Self {
    match tree {
      Tree::Byte(b) => Self::from_byte(b, off),
      Tree::Alt(alts) => {
        let mut alts = alts.into_iter();
        let mut off_copy = *off;
        let mut nfa = Self::from_tree(alts.next().unwrap(), off);
        let mut alts = alts.map(|alt| {
          let mut off_copy = off_copy;
          Self::from_tree(alt, &mut off_copy)
        });
        for alt in alts {
          nfa = nfa.union(alt);
        }
        nfa
      }
      Tree::Seq(elts) => {
        let mut nfa = Self::unit(*off);
        for elt in elts.into_iter().map(|elt| Self::from_tree(elt, off)) {
          nfa = nfa.concat(elt);
        }
        nfa
      }
    }
  }

  /// Concatenate two `Nfa`s.
  fn concat(self, other: Self) -> Self {
    if self.start == self.accepting {
      return other;
    } else if other.start == other.accepting {
      return self;
    }

    let start = self.start;
    let accepting = other.accepting;

    let mut transitions: Map<usize, Map<Byte, Set<usize>>> = self.transitions;

    for (source, transition) in other.transitions {
      let fix_usize = |usize| if usize == other.start { self.accepting } else { usize };
      let entry = transitions.entry(fix_usize(source)).or_default();
      for (edge, destinations) in transition {
        let entry = entry.entry(edge).or_default();
        for destination in destinations {
          entry.insert(fix_usize(destination));
        }
      }
    }

    Self { transitions, start, accepting }
  }

  /// Compute the union of two `Nfa`s.
  fn union(self, other: Self) -> Self {
    let start = self.start;
    let accepting = self.accepting;

    let mut transitions: Map<usize, Map<Byte, Set<usize>>> = self.transitions.clone();

    for (&(mut source), transition) in other.transitions.iter() {
      // if source is starting usize of `other`, replace with starting usize of `self`
      if source == other.start {
        source = self.start;
      }
      let entry = transitions.entry(source).or_default();
      for (edge, destinations) in transition {
        let entry = entry.entry(*edge).or_default();
        for &(mut destination) in destinations {
          // if dest is accepting usize of `other`, replace with accepting usize of `self`
          if destination == other.accepting {
            destination = self.accepting;
          }
          entry.insert(destination);
        }
      }
    }
    Self { transitions, start, accepting }
  }

  #[allow(dead_code)]
  fn edges_from(&self, start: usize) -> Option<&Map<Byte, Set<usize>>> {
    self.transitions.get(&start)
  }
}

#[derive(PartialEq, Clone, Debug)]
struct Dfa {
  transitions: Map<usize, Map<Byte, usize>>,
  start: usize,
  accepting: usize,
}

impl Dfa {
  fn from_nfa(nfa: Nfa) -> Self {
    let Nfa { transitions: nfa_transitions, start: nfa_start, accepting: nfa_accepting } = nfa;

    let mut dfa_transitions: Map<usize, Map<Byte, usize>> = Map::default();
    let mut nfa_to_dfa: Map<usize, usize> = Map::default();
    let dfa_start = nfa_start;
    nfa_to_dfa.insert(nfa_start, dfa_start);

    let mut queue = vec![(nfa_start, dfa_start)];

    while let Some((nfa_usize, dfa_usize)) = queue.pop() {
      if nfa_usize == nfa_accepting {
        continue;
      }

      for (nfa_transition, next_nfa_states) in nfa_transitions[&nfa_usize].iter() {
        let dfa_transitions = dfa_transitions.entry(dfa_usize).or_insert_with(Default::default);

        let mapped_state = next_nfa_states.iter().find_map(|x| nfa_to_dfa.get(x).copied());

        let next_dfa_usize = *dfa_transitions.entry(*nfa_transition).or_insert_with(|| {
          mapped_state.unwrap_or_else(|| next_nfa_states.iter().next().copied().unwrap())
        });

        for &next_nfa_usize in next_nfa_states {
          nfa_to_dfa.entry(next_nfa_usize).or_insert_with(|| {
            queue.push((next_nfa_usize, next_dfa_usize));
            next_dfa_usize
          });
        }
      }
    }

    let dfa_accepting = nfa_to_dfa[&nfa_accepting];

    Self { transitions: dfa_transitions, start: dfa_start, accepting: dfa_accepting }
  }

  fn bytes_from(&self, start: usize) -> Option<&Map<Byte, usize>> {
    self.transitions.get(&start)
  }

  fn uninit_from(&self, start: usize) -> Option<usize> {
    self.transitions.get(&start)?.get(&Byte::Uninit).copied()
  }

  fn byte_from(&self, start: usize, byte: Byte) -> Option<usize> {
    self
      .transitions
      .get(&start)?
      .iter()
      .filter_map(|(k, v)| if k.contains(&byte) { Some(*v) } else { None })
      .next()
  }

  fn overlap_from(&self, start: usize, byte: Byte) -> Option<Vec<usize>> {
    let res = self
      .transitions
      .get(&start)?
      .iter()
      .filter_map(|(k, v)| if byte.intersect(k) { Some(*v) } else { None })
      .collect::<Vec<usize>>();
    if res.is_empty() { None } else { Some(res) }
  }
}

/// Answers "why wasn't the source type transmutable into the destination type?"
#[derive(Debug, Hash, Eq, PartialEq, PartialOrd, Ord, Clone)]
pub enum Reason {
  /// The layout of the source type is not yet supported.
  SrcIsNotYetSupported,
  /// The layout of the destination type is not yet supported.
  DstIsNotYetSupported,
  /// The layout of the destination type is bit-incompatible with the source type.
  DstIsBitIncompatible,
  /// The destination type is uninhabited.
  DstUninhabited,
  /// The destination type may carry safety invariants.
  DstMayHaveSafetyInvariants,
  /// `Dst` is larger than `Src`, and the excess bytes were not exclusively uninitialized.
  DstIsTooBig,
  /// Src should have a stricter alignment than Dst, but it does not.
  DstHasStricterAlignment { src_min_align: usize, dst_min_align: usize },
  /// Can't go from shared pointer to unique pointer
  DstIsMoreUnique,
  /// Encountered a type error
  TypeError,
  /// The layout of src is unknown
  SrcLayoutUnknown,
  /// The layout of dst is unknown
  DstLayoutUnknown,
  /// The size of src is overflow
  SrcSizeOverflow,
  /// The size of dst is overflow
  DstSizeOverflow,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Intersection {
  Disjoint = 0b0, // 𝐴 ∩ 𝐵 = ∅ (𝐴 and 𝐵 have no elements in common)
  Overlap = 0b1,  // ∅ ⊂ 𝐴 ∩ 𝐵 ⊂ 𝐴 (𝐴 is not entirely contained within 𝐵)
  Subset = 0b11,  // 𝐴 ⊆ 𝐵 and 𝐴 ∩ 𝐵 = 𝐴 (𝐴 is a subset of 𝐵)
}

impl Intersection {
  const fn and(self, rhs: Self) -> Self {
    use core::mem::transmute;
    unsafe { transmute(transmute::<_, u8>(self) & transmute::<_, u8>(rhs)) }
  }

  const fn or(self, rhs: Self) -> Self {
    use core::mem::transmute;
    unsafe { transmute(transmute::<_, u8>(self) | transmute::<_, u8>(rhs)) }
  }
}

/// Answers whether a `Dfa` is transmutable into another `Dfa`.
pub(crate) fn answer(src: Tree, dst: Tree) -> Intersection {
  let src = Dfa::from_nfa(Nfa::from_tree(src, &mut 0));
  let dst = Dfa::from_nfa(Nfa::from_tree(dst, &mut 0));
  // dbg!((&src, &dst));
  answer_memo(&src, &dst, &mut Map::default(), src.start, dst.start)
}

#[inline(always)]
fn answer_memo(
  src: &Dfa, dst: &Dfa, cache: &mut Map<(usize, usize), Intersection>, src_state: usize,
  dst_state: usize,
) -> Intersection {
  if let Some(answer) = cache.get(&(src_state, dst_state)) {
    *answer
  } else {
    let answer = if dst_state == dst.accepting {
      // truncation: `size_of(Src) >= size_of(Dst)`
      //
      // Why is truncation OK to do? Because even though the Src is bigger, all we care about
      // is whether we have enough data for the Dst to be valid in accordance with what its
      // type dictates.
      // For example, in a u8 to `()` transmutation, we have enough data available from the u8
      // to transmute it to a `()` (though in this case does `()` really need any data to
      // begin with? It doesn't). Same thing with u8 to fieldless struct.
      // Now then, why is something like u8 to bool not allowed? That is not because the bool
      // is smaller in size, but rather because those 2 bits that we are re-interpreting from
      // the u8 could introduce invalid states for the bool type.
      //
      // So, if it's possible to transmute to a smaller Dst by truncating, and we can guarantee
      // that none of the actually-used data can introduce an invalid state for Dst's type, we
      // are able to safely transmute, even with truncation.
      Intersection::Subset
    } else if src_state == src.accepting {
      // extension: `size_of(Src) <= size_of(Dst)`
      if let Some(dst_state_prime) = dst.uninit_from(dst_state) {
        answer_memo(src, dst, cache, src_state, dst_state_prime)
      } else {
        Intersection::Disjoint
      }
    } else {
      // if the compiler cannot assume that the programmer is doing additional validity checks,
      // then for all transitions out of `src_state`, such that the transmute is viable...
      // then there must exist at least one transition out of `dst_state` such that the transmute is viable...
      src
        .bytes_from(src_state)
        .unwrap_or(&Map::default())
        .into_iter()
        .map(|(&src_validity, &src_state_prime)| {
          // ...try to find a matching transition out of `dst_state`.
          if let Some(dst_state_prime) = dst.byte_from(dst_state, src_validity) {
            answer_memo(src, dst, cache, src_state_prime, dst_state_prime)
          } else if let Some(dst_states_prime) =
            // ...try to find overlap transitions out of `dst_state`.
            dst.overlap_from(dst_state, src_validity)
          {
            Intersection::Overlap.and(dst_states_prime.iter().fold(
              Intersection::Disjoint,
              |x, &dst_state_prime| {
                x.or(answer_memo(src, dst, cache, src_state_prime, dst_state_prime))
              },
            ))
          } else if let Some(dst_state_prime) =
            // otherwise, see if `dst_state` has any outgoing `Uninit` transitions
            // (any init byte is a valid uninit byte)
            dst.uninit_from(dst_state)
          {
            answer_memo(src, dst, cache, src_state_prime, dst_state_prime)
          } else {
            // otherwise, we've exhausted our options.
            // the DFAs, from this point onwards, are bit-incompatible.
            Intersection::Disjoint
          }
        })
        .fold(Intersection::Subset, |l, r| match (l, r) {
          (Intersection::Subset, Intersection::Subset) => Intersection::Subset,
          (Intersection::Disjoint, Intersection::Disjoint) => Intersection::Disjoint,
          _ => Intersection::Overlap,
        })
    };
    if let Some(..) = cache.insert((src_state, dst_state), answer.clone()) {
      panic!("failed to correctly cache transmutability")
    }
    answer
  }
}

fn main() {
  let bool = Tree::Byte(Byte::Init { start: 0, end: 1 });
  let u8 = Tree::Byte(Byte::Init { start: 0, end: 0xff });

  println!("{:?}", answer(bool.clone(), bool.clone())); // Subset
  println!("{:?}", answer(u8.clone(), u8.clone())); // Subset
  println!("{:?}", answer(bool.clone(), u8.clone())); // Subset
  println!("{:?}", answer(u8.clone(), bool.clone())); // Overlap

  let e123 = Tree::Alt(vec![
    Tree::Seq(vec![Tree::Byte(Byte::Init { start: 1, end: 1 }), bool.clone(), u8.clone()]),
    Tree::Seq(vec![Tree::Byte(Byte::Init { start: 2, end: 2 }), bool.clone(), u8.clone()]),
    Tree::Seq(vec![Tree::Byte(Byte::Init { start: 3, end: 3 }), bool.clone(), u8.clone()]),
  ]);

  println!("{:?}", answer(e123.clone(), e123.clone())); // Subset

  let e12 = Tree::Alt(vec![
    Tree::Seq(vec![Tree::Byte(Byte::Init { start: 1, end: 1 }), bool.clone(), u8.clone()]),
    Tree::Seq(vec![Tree::Byte(Byte::Init { start: 2, end: 2 }), bool.clone(), u8.clone()]),
  ]);

  println!("{:?}", answer(e12.clone(), e123.clone())); // Subset
  println!("{:?}", answer(e123.clone(), e12.clone())); // Overlap
}
