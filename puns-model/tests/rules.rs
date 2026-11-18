//! Rules proven by the same rules in another struct: whole values and parts, from a struct, an
//! enum (with a `repr`, and without: found by discovery), an array, a union and a struct with a
//! byte that may be uninitialized, which have a struct with the same rules as the target at the
//! same place, or not (another rule, another extent). Every pair of these types is in the full
//! check (`size1`, `size2`, `parts2`); this is a sample of them, for every push.
#![cfg(not(miri))]
// Not for Miri: verdicts, which `miri_sample` reaches the unsafe paths of.

use puns_model::types::*;

#[test]
fn rules() {
  puns_model::pairs!(check_pair, usize::MAX;
    (ThirdSoft, Third), (Third, ThirdSoft), (Third, Fifth), (Fifth, Third), (u8, Third),
    (NotOneOneNz, NotOneOne), (NotOneOne, NotOneOneNz), (LastNotFf2, NotOneOne),
    (ThirdSoftIn, ThirdIn), (ThirdIn, ThirdSoftIn), (RustThirdSoft, RustThird),
    (RustThird, RustThirdSoft), ([ThirdSoft; 2], [Third; 2]), ([Third; 2], [ThirdSoft; 2]),
    (ThirdSoftUninit, ThirdUninit), (ThirdUninit, ThirdSoftUninit), (ThirdSoft, ThirdU),
    (ThirdU, Third),
  );
  puns_model::pairs!(check_part, usize::MAX;
    (LastNotFf2, LastNotFf1), (LastNotFf2, Third), ([ThirdSoft; 2], Third),
    ([Third; 2], ThirdSoft), ([ThirdSoft; 2], [Third; 2]), (ThirdSoftUninit, Third),
    (ThirdUninit, ThirdSoft), (ThirdSoftIn, ThirdIn), (NotOneOneNz, NotOneOne),
    (NotOneOne, NotOneOneNz), (Third, Fifth), ([Third; 2], Fifth),
    (Third, u8), ([Third; 2], u8), ([ThirdSoft; 2], [u8; 2]),
  );
  puns_model::finish("rules proven by the same rules");
}
