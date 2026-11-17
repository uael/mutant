//! Functions whose machine code is worth looking at:
//!
//! ```text
//! cargo rustc --release --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel
//! ```
//!
//! and look for their names in `target/release/**/codegen*.s`. See `DESIGN.md` for what to expect.
#![allow(clippy::missing_safety_doc)]

use transmutant::{
  Data, Tag,
  num::{u16be, u16bec, u32bec},
  transmute, transmute_ref, try_transmute, try_transmute_ref, unpack,
};

#[derive(Data)]
#[repr(C)]
pub struct Position {
  pub x: u8,
  pub visible: bool,
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
pub enum Kind {
  A = 1,
  B = 2,
  C = 3,
  D = 7,
}

#[derive(Data)]
#[repr(C)]
pub struct Header {
  pub magic: u32bec<0xcafebabe>,
  pub kind: Kind,
  pub flag: bool,
  pub len: u16be,
  pub tail: [u8],
}

#[derive(Data)]
#[repr(C)]
#[data(size = length)]
pub struct Udp {
  pub source: u16be,
  pub destination: u16be,
  pub length: u16be,
  pub checksum: u16be,
  pub payload: [u8],
}

#[derive(Data)]
#[repr(C)]
pub struct Frame<P: ?Sized + Tag<u16be> = [u8]> {
  pub dst: [u8; 6],
  pub src: [u8; 6],
  pub ether_type: P::Tag,
  pub payload: P,
}
#[derive(Data)]
#[repr(C)]
#[data(size = total_length, tag(u16be = u16bec<0x0800>))]
pub struct Ipv4<P: ?Sized + Data = [u8]> {
  #[data(valid(0x45..=0x4f))]
  pub version_ihl: u8,
  pub tos: u8,
  pub total_length: u16be,
  pub id: u16be,
  pub frag: u16be,
  pub ttl: u8,
  pub protocol: u8,
  pub checksum: u16be,
  pub source: [u8; 4],
  pub destination: [u8; 4],
  #[data(regions(options = (version_ihl & 0x0f) * 4 - 20, payload))]
  pub data: P,
}

// The checksum of what is behind a chain of extension headers: a rule that walks.
#[derive(Data)]
#[repr(C)]
#[data(size = payload_length + 40)]
#[data(assert({
  let upper = walk(
    next_header + (40 << 8),
    |at| { let kind = at & 0xff; kind == 0 || kind == 43 || kind == 60 },
    |at| { let start = at >> 8; be(start, 1) + ((start + 8 + be(start + 1, 1) * 8) << 8) },
  );
  let protocol = upper & 0xff;
  let start = upper >> 8;
  let end = 40 + payload_length;
  protocol != 17 || (sum16(8, 40) + protocol + (end - start) + sum16(start, end)) % 0xffff == 0
}))]
pub struct Ipv6 {
  pub version_class_flow: [u8; 4],
  pub payload_length: u16be,
  pub next_header: u8,
  pub hop_limit: u8,
  pub source: [u8; 16],
  pub destination: [u8; 16],
  pub data: [u8],
}

#[derive(Data, Clone, Copy)]
#[repr(u8)]
pub enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
  Empty,
}
#[derive(Data, Clone, Copy)]
#[repr(u8)]
pub enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Empty,
  Line(u8, u8) = 9,
}

// Nothing to check.
#[unsafe(no_mangle)]
pub fn position_bytes(p: &Position) -> &[u8; 2] {
  transmute_ref(p)
}
#[unsafe(no_mangle)]
pub fn bools_to_u32(b: [bool; 4]) -> u32 {
  transmute(b)
}
#[unsafe(no_mangle)]
pub fn frame_bytes(f: &Frame<Ipv4>) -> &[u8] {
  transmute_ref(f)
}
#[unsafe(no_mangle)]
pub fn words_bytes(w: &[u32]) -> &[u8] {
  transmute_ref(w)
}
// One comparison.
#[unsafe(no_mangle)]
pub fn bytes_position(b: &[u8; 2]) -> Option<&Position> {
  try_transmute_ref(b).ok()
}
#[unsafe(no_mangle)]
pub fn u8_bool(b: u8) -> Option<bool> {
  try_transmute(b).ok()
}
// The `bool` only: the `u8` and the `u16` are not looked at.
#[unsafe(no_mangle)]
pub fn mixed(b: &(u8, u8, u16)) -> Option<&(u8, bool, u16)> {
  try_transmute_ref(b).ok()
}
// A size, a magic number, an enum, a `bool`.
#[unsafe(no_mangle)]
pub fn header(b: &[u8]) -> Option<&Header> {
  try_transmute_ref(b).ok()
}
#[unsafe(no_mangle)]
pub fn udp(b: &[u8]) -> Option<&Udp> {
  unpack::<Udp>(b).ok().map(|(u, _)| u)
}
#[unsafe(no_mangle)]
pub fn ipv4(b: &[u8]) -> Option<&Frame<Ipv4>> {
  unpack::<Frame<Ipv4>>(b).ok().map(|(u, _)| u)
}
#[unsafe(no_mangle)]
pub fn options(p: &Ipv4) -> &[u8] {
  p.options()
}
#[unsafe(no_mangle)]
pub fn bools(b: &[u8]) -> Option<&[bool]> {
  try_transmute_ref(b).ok()
}
// Enums with fields: nothing when every variant is one of the target, else the tag, then what
// the variant of that tag requires.
#[unsafe(no_mangle)]
pub fn shapes_drawings(s: &[Shape]) -> &[Drawing] {
  transmute_ref(s)
}
#[unsafe(no_mangle)]
pub fn drawing_shape(d: &Drawing) -> Option<&Shape> {
  try_transmute_ref(d).ok()
}
#[unsafe(no_mangle)]
pub fn bytes_shape(b: &[u8; 4]) -> Option<Shape> {
  try_transmute(*b).ok()
}
// A rule that walks: a loop over the headers, then the sums.
#[unsafe(no_mangle)]
pub fn ipv6(b: &[u8]) -> Option<&Ipv6> {
  try_transmute_ref(b).ok()
}

fn main() {}
