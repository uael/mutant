# Design

## Goal

Safe transmutation checked at compile time, on stable Rust, between any two "plain data" types:
structs and enums of any `repr`, tuples, arrays, slices and slice-tailed structs, `str`, with

- a **compile error** when a conversion can never be valid,
- **no code at all** when it is always valid,
- **only the checks that could not be proven** when it is valid for some values (`try_*`),
- sizes, lengths and assertions that are in the data itself, told by `#[derive(Data)]`.

`tests/` and `examples/ethernet.rs` are the reference for what works.

## What stable Rust (1.100) gives us

Probed, not remembered:

| Needed                                   | Stable?                                            |
|------------------------------------------|----------------------------------------------------|
| `TransmuteFrom`, `ptr_metadata`          | no                                                 |
| const traits, `generic_const_exprs`      | no                                                 |
| reflection, `offset_of!` on enums/slices | no                                                 |
| `const fn` with loops, `&mut`, raw ptrs  | yes                                                |
| inline `const { }` in generic code       | yes                                                |
| `size_of_val_raw`, `align_of_val_raw`    | yes, and `const`                                   |
| `offset_of!` on sized fields             | yes, even when the struct is unsized               |

Const evaluation runs about 400 000 loop iterations per second and is stopped by a deny-by-default
lint after 2 000 000 basic blocks. Any analysis whose cost grows with the *size* of a type (one
step per byte) or with its *length* (one step, or one type instantiation, per array element) is
therefore out. This is what both previous attempts ran into.

## Architecture

### 1. The layout of a type is a value, not a type (`layout.rs`)

`Data::LAYOUT` is a `const` tree built by `const fn`s from what the compiler says of the type
(`size_of`, `offset_of!`, `size_of_val_raw`...):

- **scalar**: 1 to 16 bytes, a byte order, a sorted list of valid ranges. `bool` is `0..=1`,
  `char` is two ranges, `NonZero` is `1..`, an enum without fields is the values of its variants
  (read from the compiler, whatever its `repr`);
- **struct**: fields at their actual offsets (any `repr`: the offsets come from the compiler);
  bytes that no field covers are padding. A struct may have **rules** (see below);
- **array** and **slice**: an element layout and a count (or none);
- **pattern**: a tail of bytes that a DFA accepts. `str` is the UTF-8 automaton (8 states).

This tree *is* an automaton, in a compact hierarchical form: a position in it is a state.

### 2. The analysis is ordinary `const fn` code over those values (`analysis.rs`)

For a source `S` and a target `D`, `analyze` decides language inclusion, `L(S) ⊆ L(D)`, by walking
both trees in lockstep (a product automaton), with what keeps it cheap:

- **runs**: a subtree without constraint is one step, whatever its size (`[u8; 1 << 40]`);
- **scalars are compared as sets, not byte by byte**: aligned scalars compare their range lists;
  misaligned ones, or ones of different byte orders, go through an exact decomposition of integer
  ranges into byte boxes (`digit_box`, `box_meets`), so `NonZero<u16>` little-endian is known to
  be `NonZero<u16>` big-endian, and `u32 >= 0x100` is known not to be;
- **periods**: arrays and slices are not unrolled. When the target is at the start of an element
  and the source is inside a repetition, the least common multiple of both strides is analysed
  once and the rest is skipped. Two slice tails, which never end, are settled in one period;
- **patterns** are run with sets of states over the runs of the other side, each box of a scalar
  on its own; a repetition is left as soon as one of its elements changes nothing to the set.
  Two patterns are compared by a product of their automata.

It answers:

- *fit*: how sizes map (always / for some slice lengths / never / truncated), with the trailing
  padding of unsized structs accounted for, and checked against `size_of_val_raw`;
- *uninit*: a target byte that must be initialized would be read from padding (always an error);
- *infallible* / *fallible* / *unreachable*;
- *marks*: the scalars, patterns and rules of the target that the source does not prove: exactly
  what a fallible transmutation must check at run time;
- *mutable*: the reverse inclusion, required to hand out a `&mut`.

Errors are post-monomorphization (`const { .. }` in the generic function), since there are no
const traits to turn them into trait bounds. `cargo check` does not see them, `cargo build` does.

### 3. Run-time validation is type-directed and guarded by the analysis (`validate.rs`)

`try_*` functions and `unpack` recurse over the *types* (associated types: element, list of
fields) in `const fn`s; each check sits behind `if const { marks say so }`. What the analysis
proved never reaches code generation.

Measured on x86-64, `opt-level = 3` (the functions are in `examples/codegen.rs`):

| Function                                           | Machine code                               |
|----------------------------------------------------|--------------------------------------------|
| `&Position -> &[u8; 2]`, `[bool; 4] -> u32`        | `mov rax, rdi; ret`                        |
| `&Frame<Ipv4<[u8]>> -> &[u8]`                      | `lea rdx, [rsi + 34]` (the length)         |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`            |
| `&[u8] -> Option<&Header>`                         | size, 4 bytes compare, `bt`, `cmp`         |
| `unpack::<Frame<Ipv4>>(&[u8])`                     | 43 instructions, no loop, no call          |
| `Ipv4::options()` (a region)                       | 19 instructions, no branch                 |
| `&[u8] -> Option<&[bool]>`                         | a loop of one `cmp` per byte               |

### 4. Rules: what is not a property of bytes

A struct can have rules, which `#[derive(Data)]` compiles from expressions over its integer
fields to a constant postfix program (`layout::Op`):

- `#[data(size = EXPR)]`: the size in bytes of the value;
- `#[data(len = EXPR)]`: the number of elements of its slice;
- `#[data(assert(EXPR))]`: anything else (bit fields, relations between fields, a minimum size);
- `#[data(regions(a = EXPR, b))]`: consecutive regions of the tail, with an accessor for each.

An expression can read the value beyond its fields: `size_of_val(self)`, `le(offset, N)` and
`be(offset, N)` (an integer anywhere in the value), `crc32(start, end)` and `sum16(start, end)`
(the frame check sequence of Ethernet, the checksum of IP). So a trailer is a region, and "the 4
last bytes are the CRC of what is before" is an assertion. A checksum that covers several layers
(the one of UDP and TCP covers the addresses of the IP header) is an assertion of the struct that
holds them all: the IPv4 packet.

What cannot be computed (an overflow, a read out of the value) makes a rule fail, unless it is
the side of a `&&` or `||` that the other side decides without: `protocol != 17 || be(..) == 0`
holds for a packet that is not UDP, however short.

Two more options are not rules, but save the code that would otherwise be written by hand:

- `#[data(tag(Key = Type))]` implements `Tag<Key>`: "when carried by something that tells what
  it carries with a `Key`, this type comes with a `Type`". A frame is then
  `struct Frame<P: ?Sized + Tag<EtherType> = [u8]> { .., ether_type: P::Tag, payload: P }`, and
  `Frame<Ipv4>` requires the EtherType of IPv4 while `Frame` takes any (bytes go with any tag);
- `#[data(get(name: Type = EXPR))]` generates a `const fn` getter, for bit fields. Here `EXPR`
  is plain Rust, in which integer fields are `u128`: any function can be called;
- a region can be given a type, `regions(options = EXPR, payload: P)`: its accessor then returns
  a `&P`, if the bytes of the region are one. This is how a packet whose header has a variable
  size carries a typed payload;
- on an enum whose variants each hold a reference,
  `enum AnyFrame<'a> { Ipv4(&'a Frame<Ipv4>), Arp(&'a Frame<Arp>), Other(&'a Frame) }`, the derive
  generates `parse` and `unpack`, which return the first variant that the bytes are: telling one
  thing from another is a declaration too.

Rules are checked when a value comes from something else, tell `unpack` how long a value is, and
are proven only by the same rule, in the same struct, at the same place of the source. The
evaluator (`layout::eval`) is written so that a constant program compiles to its arithmetic: no
loop for the first operations, and a local variable per stack slot rather than an array.

### 5. What the type system enforces, and what it does not

Soundness rests on one distinction (`Spec::valid` and `Spec::known`):

- what a type **guarantees**, whatever safe code does: `bool`, enums, `NonZero`, `str`, and
  library types with a private field and a checked constructor (`u16bec<V>`, `u8in<MIN, MAX>`);
- what a struct merely **requires** of what it is made from: `#[data(valid(..))]` on a plain
  field, and rules. Safe code can write any value to a `pub version: u8`.

A requirement is checked (or proven) on the way in, but it never proves what another type
guarantees: `&[Unchecked] -> &str` needs a check even if `Unchecked` is "`valid(..=0x7f)`", while
`&[u8in<0, 0x7f>] -> &str` does not. For the same reason, the accessors of regions cut regions
short rather than trust the fields that size them.

### 6. Pointers to unsized types (`ptr.rs`)

Splitting and building wide pointers in generic `const fn` code is done by reinterpreting
`*const T` as `*const [u8]`. That representation is not documented, so it is **verified at compile
time for each unsized type** against a pointer made with a genuine `as` cast (`Data::PROBE`,
`ptr::verify`): if it ever changes, the crate stops compiling instead of misbehaving.

The tests pass under Miri, with strict provenance and with tree borrows.

## Not done yet

- **Enums with fields, as data** (enums of references are views, see above). In the engine: a source enum is analysed variant by variant, a target
  enum by relating what the source has under the tag, then each variant it can select.
  `repr(C)` and `repr(u8)`... enums have a specified layout (RFC 2195). `repr(Rust)` ones do not,
  and stable Rust cannot name it, but it can be *observed* in const evaluation from a sample of
  each variant: field offsets by matching on the sample; the tag by "transplanting" bytes of one
  variant over another and matching again. Probed on 1.100: this finds the niche of
  `enum { A(u8, bool), B(u8), C }` (the `bool` byte, values 2 and 3) and the tag of a directly
  tagged enum. An experiment that goes wrong is a compile error (const evaluation rejects invalid
  tags), never undefined behavior. One thing cannot be observed: the *width* of a direct tag
  (its high bytes are zero in every variant, and const evaluation refuses to read bytes that may
  be padding); those bytes would be "must be zero" in a target and "may be uninitialized" in a
  source, which is sound either way.
- Patterns other than `str`: the engine takes any DFA of at most 64 states, but there is no way
  to declare one from the derive.
- `default()`: building a value from the defaults of its fields (literals, sizes).
- An arbitrary function in a rule (getters take any): it could only be called through a trait,
  hence not in `const` contexts on stable. `crc32` and `sum16` are built in for that reason.

## Limits

- The types of the fields of a struct must be at least as visible as the struct: the list of
  fields is an associated type, which the privacy rules of Rust do not allow to name a less
  visible type.
- A struct of more than about a hundred fields may need `#![recursion_limit]` to be raised in
  the crate that validates it.
- A packed struct whose unsized field has trailing padding of its own is rejected.
- Unions are not planned.
