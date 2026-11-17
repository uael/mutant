//! An expression of `#[pun(..)]` means what the same Rust expression means, where every integer
//! is a `u128`: each test is a case where it did not.

use puns::{Pun, num::u16be, try_pun_ref};

/// `as` truncates, as in Rust.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((a + b) as u8 == 0))]
#[pun(assert((a * 0x101) as u16 == a * 0x101 && (a << 60) as u64 == (a % 16) << 60))]
struct Cast {
  a: u8,
  b: u8,
}

#[test]
fn casts_truncate() {
  assert!(try_pun_ref::<_, Cast>(&[200u8, 56]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[0u8, 0]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[200u8, 55]).is_err());
  assert!(try_pun_ref::<_, Cast>(&[16u8, 240]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[1u8, 254]).is_err());
}

/// A length in a field, rounded up to a multiple of 4.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + ((length + 3) & !3))]
struct Padded {
  kind: u16be,
  length: u16be,
  data: [u8],
}

#[test]
fn not_of_an_integer_is_bitwise() {
  let (padded, rest) = puns::from_head::<Padded>(&[0, 1, 0, 5, 1, 2, 3, 4, 5, 0, 0, 0, 9]).unwrap();
  assert_eq!((padded.kind.get(), padded.data.len(), rest), (1, 8, &[9][..]));
}

/// Constants are unsigned integers, of any type, read as `u128`s; `b'a'` is one.
const SMALL: u8 = 0xff;
const WIDE: u64 = 0x1_0000_0000;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(x + SMALL as u128 > 0xff && x + SMALL > 0xff || x == 0))]
#[pun(assert(x < u8::MAX && (x as u64 + WIDE) as u32 == x as u32 && x != b'!' as u128))]
#[pun(assert(1u8 + x > 0 && 0x1_0000 * x != 1))]
struct Constants {
  x: u8,
}

#[test]
fn constants_are_unsigned_integers() {
  assert!(puns::from_bytes::<Constants>(&[1]).is_ok());
  assert!(puns::from_bytes::<Constants>(&[0]).is_ok());
  assert!(puns::from_bytes::<Constants>(b"!").is_err());
  assert!(puns::from_bytes::<Constants>(&[0xff]).is_err());
}

/// `<<` drops the bits shifted out, as Rust does; a shift by 128 or more fails, where Rust would
/// panic (or, without debug assertions, shift by the amount modulo 128).
#[derive(Pun)]
#[repr(C)]
#[pun(assert((x << 127) << 1 == 0 && x << y > 0 && x >> y < 0x100))]
struct Shifts {
  x: u8,
  y: u8,
}

#[test]
fn shifts_drop_bits_and_fail_beyond_the_width() {
  assert!(puns::from_bytes::<Shifts>(&[1, 0]).is_ok());
  assert!(puns::from_bytes::<Shifts>(&[1, 127]).is_ok());
  // `2 << 127` is `0`.
  assert!(puns::from_bytes::<Shifts>(&[2, 127]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[1, 128]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[0xff, 200]).is_err());
}

/// `&&` and `||` compute their left side first, as Rust does: the right side is only computed if
/// the left one does not decide, and what fails on the left fails.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(1, 2) == 0xcafe || self.0 != 7))]
#[pun(assert(10 - self.0 > 0 && self.0 != 8 || true))]
struct LeftFirst(u8, [u8]);

#[test]
fn and_or_compute_the_left_side_first() {
  assert!(puns::from_bytes::<LeftFirst>(&[7, 0xca, 0xfe]).is_ok());
  assert!(puns::from_bytes::<LeftFirst>(&[1, 0xca, 0xfe]).is_ok());
  assert!(puns::from_bytes::<LeftFirst>(&[1, 0, 0]).is_ok());
  // Out of the value, on the left: as Rust would panic, the rule fails.
  assert!(puns::from_bytes::<LeftFirst>(&[1]).is_err());
  // `10 - 11` fails, whatever the right side.
  assert!(puns::from_bytes::<LeftFirst>(&[11, 0xca, 0xfe]).is_err());
  assert!(puns::from_bytes::<LeftFirst>(&[8, 0xca, 0xfe]).is_ok());
}

/// A name that ends with a zero, and whose end matters only for some flags: what a `let` binds is
/// computed, whether it is used or not, as in Rust.
#[derive(Pun)]
#[repr(C)]
#[pun(assert({
  let end = walk(1, |at| be(at, 1) != 0, |at| at + 1);
  flags & 2 == 0 || end + 1 == size_of_val(self)
}))]
struct Named {
  flags: u8,
  name: [u8],
}

/// A `let` in the branch of an `if` that is not taken is not computed.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(if flags & 4 != 0 { let x = be(1, 1); x == 0 } else { true }))]
struct Branch {
  flags: u8,
  rest: [u8],
}

/// `let`s that the value does not need, which may or may not fail: `x - 1` and `x / y` may, `x +
/// 40` cannot (`x` is a byte).
#[derive(Pun)]
#[repr(C)]
#[pun(assert({ let less = x - 1; let ratio = x / y; let more = x + 40; x == 0 || less + ratio + more > 0 }))]
struct Unused {
  x: u8,
  y: u8,
}

#[test]
fn what_a_let_binds_is_computed() {
  assert!(puns::from_bytes::<Unused>(&[1, 1]).is_ok());
  assert!(puns::from_bytes::<Unused>(&[0, 1]).is_err());
  assert!(puns::from_bytes::<Unused>(&[1, 0]).is_err());
  assert!(puns::from_bytes::<Named>(&[0, b'a', 0]).is_ok());
  assert!(puns::from_bytes::<Named>(&[2, b'a', 0]).is_ok());
  // No end: the walk fails, whatever the flags.
  assert!(puns::from_bytes::<Named>(&[2, b'a']).is_err());
  assert!(puns::from_bytes::<Named>(&[0, b'a']).is_err());
  assert!(puns::from_bytes::<Branch>(&[0]).is_ok());
  assert!(puns::from_bytes::<Branch>(&[4, 0]).is_ok());
  assert!(puns::from_bytes::<Branch>(&[4]).is_err());
}

/// A netmask: ones, then zeros. The walk takes a step per bit, more than the value has bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(assert({
  let ones = walk(0, |i| i < 32 && (mask >> (31 - i)) & 1 == 1, |i| i + 1);
  (mask << ones) as u32 == 0
}))]
struct Netmask {
  mask: puns::num::u32be,
}

#[test]
fn a_walk_takes_a_step_per_bit() {
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0xff, 0]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0xff, 0xff]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0, 0, 0, 0]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0, 0xff]).is_err());
}

/// Getters are Rust, where an integer field, wherever it is named, is its value as a `u128`.
#[derive(Pun)]
#[repr(C)]
#[pun(get(
  magnitude: u32 = if height >> 31 != 0 { 0x1_0000_0000 - height } else { height },
  value: f64 = f64::from_bits(bits as u64),
  low: u8 = { let bits = bits & 0xff; bits as u8 },
  wide: u32 = self.height.wrapping_add(0) as u32,
))]
struct Getters {
  height: puns::num::u32le,
  bits: puns::num::u64le,
}

/// A getter over a field of a generic type, which is not `Copy`.
#[derive(Pun)]
#[repr(C)]
#[pun(get(is_two: bool = kind == 2))]
struct Generic<H> {
  kind: H,
}

#[test]
fn getters_are_rust() {
  let mut bytes = [0u8; 12];
  bytes[..4].copy_from_slice(&(-3i32).to_le_bytes());
  bytes[4..].copy_from_slice(&1.5f64.to_bits().to_le_bytes());
  let getters = puns::from_bytes::<Getters>(&bytes).unwrap();
  assert_eq!((getters.magnitude(), getters.value(), getters.low()), (3, 1.5, 0));
  assert_eq!(getters.wide(), 0xffff_fffd);
  assert!(puns::from_bytes::<Generic<u8>>(&[2]).unwrap().is_two());
  assert!(!puns::from_bytes::<Generic<u8>>(&[3]).unwrap().is_two());
}

/// Bounds that the derive needs, written by the user: they are not repeated, which clippy would
/// see (`trait_duplication_in_bounds`).
#[derive(Pun)]
#[repr(C)]
struct Bounded<T: Pun, U>
where
  U: puns::Pun,
{
  value: T,
  other: U,
  again: T,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Choice<T: Pun + Copy> {
  A(T),
  B(T, T),
}

#[test]
fn bounds_are_not_repeated() {
  let bounded = puns::from_bytes::<Bounded<u8, bool>>(&[1, 1, 2]).unwrap();
  assert_eq!((bounded.value, bounded.other, bounded.again), (1, true, 2));
  assert!(matches!(puns::try_pun::<_, Choice<bool>>([1u8, 1, 0]), Ok(Choice::B(true, false))));
  assert!(matches!(puns::try_pun::<_, Choice<bool>>([0u8, 1, 0]), Ok(Choice::A(true))));
  for choice in [Choice::A(true), Choice::B(false, true)] {
    let same: Choice<bool> = puns::pun(choice);
    assert!(puns::try_pun::<_, Choice<u8>>(same).is_ok());
  }
}

/// A parameter that only a typed region names is used by a `PhantomData`, as Rust requires.
#[derive(Pun)]
#[repr(C)]
struct Packet<P: ?Sized + Pun> {
  kind: u8,
  payload: core::marker::PhantomData<P>,
  #[pun(regions(body: P))]
  rest: [u8],
}

#[test]
fn a_typed_region_of_a_parameter() {
  let packet = puns::from_bytes::<Packet<[bool]>>(&[7, 1, 0]).unwrap();
  assert_eq!((packet.kind, packet.body()), (7, Ok(&[true, false][..])));
  assert!(puns::from_bytes::<Packet<[bool]>>(&[7, 2]).unwrap().body().is_err());
}
