//! Every verdict of `puns` without the `unstable` feature, on pairs of types of which Rust keeps
//! some layouts and not others, and which bytes each type accepts: the same whatever layout
//! `rustc` chooses where Rust lets it choose.
//!
//! The record is compared with `seeds.txt`, which CI requires under `-Zrandomize-layout` with
//! several seeds, and without (`PUNS_BLESS=1` writes it). It is the record of `x86_64`: the
//! alignment of `u16` and the width of `usize` are the ones of the target.
//!
//! Not for Miri: it runs no unsafe code of its own, and what it reaches of the library is what
//! the tests of `puns` reach.

#![cfg(not(miri))]

use core::{any::type_name, fmt::Write as _, num::NonZero};

use puns::{Pun, hook};

/// A struct of the `Rust` representation of which Rust keeps the size and that its bytes are
/// initialized, not where its `bool` is.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// A struct of the `Rust` representation of which Rust keeps every byte: two of the same.
#[derive(Pun, Clone, Copy)]
struct Twins {
  a: bool,
  b: bool,
}

/// A struct of the `Rust` representation of which Rust keeps every byte: bytes of any value,
/// without padding in any order.
#[derive(Pun, Clone, Copy)]
struct Bytes {
  a: u8,
  b: [u8; 3],
}

/// A `repr(C)` struct that holds a struct whose bytes Rust does not keep.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Holds {
  mixed: Mixed,
  word: u16,
}

/// A newtype of the `Rust` representation: its field is at 0.
#[derive(Pun, Clone, Copy)]
struct Newtype(u16);

/// An enum with fields and without `repr`: nothing is kept of its tag.
#[derive(Pun, Clone, Copy)]
enum Tagged {
  A(u8),
  B(bool),
}

/// An enum without fields and without `repr`: nothing is kept of its values.
#[derive(Pun, Clone, Copy)]
enum Plain {
  X,
  Y,
}

/// The same enum, `repr(u8)`: it is kept.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Kept {
  X,
  Y,
}

/// The values a byte takes in the patterns: the edges of `bool`, of `NonZero` and of the tags.
const BYTES: [u8; 5] = [0, 1, 2, 0x7f, 0xff];

/// Bytes aligned for every type here.
#[repr(align(16))]
struct Aligned([u8; 16]);

/// The verdicts of every function on a pun from `S` to `D`, and why `try_pun_ref` does not
/// compile.
fn line<S: Pun, D: Pun>() -> String {
  let v = hook::verdict::<S, D>();
  let flags =
    [v.pun, v.head, v.try_pun, v.pun_ref, v.head_ref, v.try_pun_ref, v.pun_mut, v.try_pun_mut];
  let flags: String = flags.iter().map(|&ok| if ok { '+' } else { '-' }).collect();
  let why = v.why().trim().replace('\n', " ");
  format!("{} -> {}: {flags} {why}\n", type_name::<S>(), type_name::<D>())
}

/// Which byte patterns of the size of `D` it accepts from bytes (`try_pun_ref`): how many of
/// them, and a hash of which.
fn accepts<D: Pun>() -> String {
  let name = type_name::<D>();
  if !hook::tries_view::<[u8], D>(0) {
    return format!("[u8] -> {name}: refused\n");
  }
  let size = size_of::<D>();
  let total = BYTES.len().pow(u32::try_from(size).expect("a small type"));
  let (mut count, mut hash) = (0u64, 0xcbf2_9ce4_8422_2325_u64);
  for index in 0..total {
    let mut buffer = Aligned([0; 16]);
    let mut rest = index;
    for byte in &mut buffer.0[..size] {
      *byte = BYTES[rest % BYTES.len()];
      rest /= BYTES.len();
    }
    let accepted = matches!(hook::try_view_ref::<[u8], D, 0>(&buffer.0[..size]), Some(Ok(_)));
    if accepted {
      count += 1;
      hash = (hash ^ index as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
  }
  format!("[u8] -> {name}: {count} of {total} ({hash:016x})\n")
}

/// The record of every pair of the types, then of what each accepts.
macro_rules! record {
  ($($t:ty),* $(,)?) => {{
    let mut out = String::new();
    record!(@rows out [$($t),*] $($t),*);
    $(out.push_str(&accepts::<$t>());)*
    out
  }};
  (@rows $out:ident $all:tt $($s:ty),*) => {
    $(record!(@row $out $s; $all);)*
  };
  (@row $out:ident $s:ty; [$($d:ty),*]) => {
    $($out.push_str(&line::<$s, $d>());)*
  };
}

#[test]
fn the_same_whatever_the_layout() {
  if puns::layout::UNSTABLE {
    // Built with the rest of the workspace: `puns-model` turned the feature on.
    eprintln!("`puns` has the `unstable` feature: nothing to check (`cargo test -p puns-stable`)");
    return;
  }
  let record = record!(
    u8,
    u16,
    bool,
    [u8; 2],
    [u8; 4],
    [bool; 2],
    NonZero<u16>,
    (u8, u8),
    (bool, bool),
    (bool, u8),
    (u8, u16),
    (u8, u16, u8),
    Mixed,
    Twins,
    Bytes,
    Holds,
    Newtype,
    Tagged,
    Plain,
    Kept,
    Option<bool>,
    Option<NonZero<u16>>,
    Result<NonZero<u8>, ()>,
  );
  let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/seeds.txt");
  if std::env::var_os("PUNS_BLESS").is_some() {
    std::fs::write(path, &record).expect("seeds.txt is written");
    return;
  }
  let expected = std::fs::read_to_string(path).expect("seeds.txt: run with `PUNS_BLESS=1`");
  let mut diff = String::new();
  for (line, (got, want)) in record.lines().zip(expected.lines()).enumerate() {
    if got != want {
      writeln!(diff, "line {}:\n  got  {got}\n  want {want}", line + 1).unwrap();
    }
  }
  if record.lines().count() != expected.lines().count() {
    writeln!(diff, "{} lines, {} expected", record.lines().count(), expected.lines().count())
      .unwrap();
  }
  assert!(diff.is_empty(), "the verdicts are not the ones of `seeds.txt`:\n{diff}");
}
