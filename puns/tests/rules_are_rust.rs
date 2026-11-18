//! What an expression of `#[pun(..)]` means is what the same Rust expression means where every
//! integer is a `u128`, and where Rust would panic, the rule fails: each expression here is both
//! a rule of a struct and the body of a function, and they must agree on every value.
//!
//! Not built under Miri: this is about what the derive makes of an expression, and the unsafe
//! reads of the evaluator are the ones that `expressions.rs` runs there.

#![cfg(not(miri))]
#![expect(
  unused_variables,
  clippy::bad_bit_mask,
  clippy::eq_op,
  clippy::erasing_op,
  clippy::let_and_return,
  clippy::manual_is_multiple_of,
  clippy::nonminimal_bool,
  clippy::overly_complex_bool_expr,
  reason = "the expressions are there for what they compute, and how: as rules, they are not Rust"
)]

/// `name: |a, b, c, d| expression;`: a struct of four fields with the rule `assert(expression)`,
/// and the function of four `u128`s that the expression is.
macro_rules! rules {
  ($($name:ident: |$a:ident, $b:ident, $c:ident, $d:ident| $e:expr;)+) => {
    $(mod $name {
      #[derive(puns::Pun)]
      #[repr(C)]
      #[pun(assert($e))]
      pub struct Rule {
        pub $a: u8,
        pub $b: puns::num::u16le,
        pub $c: puns::num::u32be,
        pub $d: puns::num::u64le,
      }

      pub const fn rust($a: u128, $b: u128, $c: u128, $d: u128) -> bool {
        $e
      }
    })+

    /// Each expression, as a rule and as Rust, over the same values: `(holds, fails)` counts.
    fn compare(values: &[(u128, u128, u128, u128)]) -> Vec<(&'static str, usize, usize)> {
      let mut counts = vec![];
      $(
        let (mut holds, mut fails) = (0, 0);
        for &(a, b, c, d) in values {
          let mut bytes = vec![a as u8];
          bytes.extend((b as u16).to_le_bytes());
          bytes.extend((c as u32).to_be_bytes());
          bytes.extend((d as u64).to_le_bytes());
          let rule = puns::from_bytes::<$name::Rule>(&bytes).is_ok();
          // A panic of Rust (an overflow, a division by zero, a shift too far) is a rule that fails.
          let rust = std::panic::catch_unwind(|| $name::rust(a, b, c, d)).unwrap_or(false);
          assert_eq!(rule, rust, "{}: a = {a}, b = {b}, c = {c}, d = {d}", stringify!($name));
          if rust { holds += 1 } else { fails += 1 }
        }
        counts.push((stringify!($name), holds, fails));
      )+
      counts
    }
  };
}

/// Values of the four fields: the bounds of each, small ones, the ones around a sign bit, any.
fn values(count: usize) -> Vec<(u128, u128, u128, u128)> {
  let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
  let mut next = move || {
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    seed
  };
  let mut pick = |bits: u32| -> u128 {
    let max = u64::MAX >> (64 - bits);
    let value = match next() % 8 {
      0 => 0,
      1 => 1,
      2 => max,
      3 => max - 1,
      4 => next() % 8,
      5 => 127 + next() % 3,
      _ => next(),
    };
    u128::from(value & max)
  };
  (0..count).map(|_| (pick(8), pick(16), pick(32), pick(64))).collect()
}

#[test]
fn rules_are_rust() {
  let values = values(4000);
  let hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(|_| {}));
  let counts = std::panic::catch_unwind(|| compare(&values));
  std::panic::set_hook(hook);
  let counts = counts.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
  // The expressions are worth something if most of them both hold and fail.
  let both = counts.iter().filter(|(_, holds, fails)| *holds > 0 && *fails > 0).count();
  assert!(both >= 15, "{counts:?}");
}

rules! {
  e01: |a, b, c, d| a + b * 2 == c - d;
  e02: |a, b, c, d| ((a as u8 as u128) << 4) >> 4 == a & 0x0f;
  e03: |a, b, c, d| (!a & 0xff) == 255 - a;
  e04: |a, b, c, d| a < 100 && b / a > 3;
  e05: |a, b, c, d| a == 0 || b % a == 1;
  e06: |a, b, c, d| (if a > 100 { c / (a - a) } else { 5 }) == 5;
  e07: |a, b, c, d| { let x = a + b; let y = x * x; y % 7 } < 4;
  e08: |a, b, c, d| { let unused = c / (a & 0); 1 } == 1;
  e09: |a, b, c, d| (b << (a & 127)) >> (a & 127) == b;
  e10: |a, b, c, d| (d << a) != 0 || a >= 128;
  e11: |a, b, c, d| (a as u16 as u128) + (b as u8 as u128) == (c as u32 as u128) % 300;
  e12: |a, b, c, d| d - c > b + a;
  e13: |a, b, c, d| (a ^ b | c & d) == ((a ^ b) | (c & d));
  e14: |a, b, c, d| a + b + c - d * 0 >= a;
  e15: |a, b, c, d| !(a < b) == (a >= b);
  e16: |a, b, c, d| ((if a < 128 { a } else { 255 - a }) < 64) != (b > 1000);
  e17: |a, b, c, d| d * d * d < 1000 || d * d * d >= 1000;
  e18: |a, b, c, d| 1u128 << a < 5;
  e19: |a, b, c, d| (a as usize) + 1 > 0;
  e20: |a, b, c, d| { let a = b; ({ let b = a + 1; b }) + a } == 2 * b + 1;
  e21: |a, b, c, d| a - b - c == 0 || a > b;
  e22: |a, b, c, d| a / (b + 1) / (c + 1) == a / ((b + 1) * (c + 1));
  e23: |a, b, c, d| (a < b) == (b > a) && (a <= b) != (a > b);
  e24: |a, b, c, d| b >> a == 0 || a < 16;
  e25: |a, b, c, d| (c as u8 as u16 as u8 as u128) == (c & 0xff) && (d as u32 as u128) == d % 0x1_0000_0000;
  e26: |a, b, c, d| a % 2 == 0 && b % 2 == 0 || c % 2 == 0 && d % 2 == 0;
  e27: |a, b, c, d| a > 0 && (b / a > 0 || c / (a - 1) > 0);
  e28: |a, b, c, d| (if a == 0 { 0 } else if a == 1 { b } else if a < 128 { c } else { d }) & 1 == 0;
  e29: |a, b, c, d| !!a == a && !(!(a == 1)) == (a == 1);
  e30: |a, b, c, d| { let s = a + b + c; let t = s - a; t - b } == c;
  e31: |a, b, c, d| a * b * c * d / (a + 1) < d || a == 255;
  e32: |a, b, c, d| (a | 1) != 0 && (d >> 63 == 1 || d < 0x8000_0000_0000_0000);
}
