# Design

## Goal

Type punning proven at compile time, on stable Rust: a safe alternative to `transmute`, and to
hand-written parsing of bytes. A pun reads a value of one "plain data" type, the *source*, as a
value of another, the *target*: structs of any `repr`, enums (of any `repr` without fields, of a
specified one with fields), tuples, arrays, slices and slice-tailed structs, `str`. A pun is

- **refused** (a compile error) when it can never be valid,
- **proven** (no code at all) when it is always valid,
- **checked** (only what could not be proven, by a `try_` or a `from_*` function) when it is
  valid for some values,

with sizes, lengths and assertions that are in the data itself, told by `#[derive(Pun)]`.

`tests/` and `examples/` (Ethernet, then services over IP, transports, routing, links, images,
media, archives, executables, storage and encodings) are the reference for what works; each
example says what it does not cover, and why.

## API

The conversions are a grid: what is read (a typed value, or bytes) by how much of it (the whole,
its front, its back).

|               | Whole                                      | Front                          | Back                           |
|---------------|--------------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut` (+ `try_pun*`) | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `from_bytes_mut`, `as_bytes` | `from_head`, `from_head_mut`   | `from_tail`, `from_tail_mut`   |

- The first row is proven (nothing at run time) but for `try_pun*`, which is checked. A part is
  the first or the last `size_of::<D>()` bytes (or, for `head*`, as many elements of a slice as
  fit); `tail*` needs a sized source.
- The second row is checked: the size, the address, then what bytes do not prove of the target.
  `from_bytes*` and `as_bytes` are sugar for `try_pun_ref::<[u8], D>`, `try_pun_mut::<[u8], D>`
  and `pun_ref::<S, [u8]>`, with identical behaviour: they pin one side to bytes (an unsuffixed
  integer literal array would otherwise be an `[i32; N]`) and take one type parameter.
  `from_head*` takes as much as the `size` or `len` rule of the target says; `from_tail*` needs
  a sized target, whose extent cannot be found from the end otherwise.
- A view enum (variants that hold references, see `#[derive(Pun)]`) gets `from_bytes` and
  `from_head`: the first variant that the bytes are.
- Every function is a `const fn`.

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

`Pun::LAYOUT` is a `const` tree built by `const fn`s from what the compiler says of the type
(`size_of`, `offset_of!`, `size_of_val_raw`...):

- **scalar**: 1 to 16 bytes, a byte order, a sorted list of valid ranges. `bool` is `0..=1`,
  `char` is two ranges, `NonZero` is `1..`, an enum without fields is the values of its variants
  (read from the compiler, whatever its `repr`);
- **struct**: fields at their actual offsets (any `repr`: the offsets come from the compiler);
  bytes that no field covers are padding. A struct may have **rules** (see below);
- **enum** with fields: a tag (a scalar whose valid values are the tags of the variants), and
  for each variant a struct of its fields. The representations that Rust specifies (`repr(C)`,
  `repr(u8)`..., RFC 2195) are made of `repr(C)` structs, which the derive declares and asks the
  compiler for; the result is checked against the size and the alignment of the enum;
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
  Two patterns are compared by a product of their automata;
- **enums** are related variant by variant: each variant of the source with the variant of the
  target that has its tag (none: that variant is not valid), as two structs. What an enum
  requires of its bytes depends on its tag, so nothing but an enum proves an enum: from
  anything else, the tag is related as a scalar (a source that never holds a tag is an error),
  the bytes that some variant needs must be initialized, and the rest is checked at run time.
  As a source, an enum is known by its tag and by the bytes that every variant initializes.

It answers:

- *fit*: how sizes map (always / for some slice lengths / never / truncated), with the trailing
  padding of unsized structs accounted for, and checked against `size_of_val_raw`;
- *uninit*: a target byte that must be initialized would be read from padding (always an error);
- *infallible* / *fallible* / *unreachable*;
- *marks*: the scalars, patterns and rules of the target that the source does not prove: exactly
  what a checked pun must check at run time;
- *mutable*: the reverse inclusion, required to hand out a `&mut`.

**Shifted relations** (`relate_at`, `analyze_tail`). The cursors of both layouts count offsets from
the start of the source, and each starts at its own base: the target of `tail` starts at
`shift = size(S) - size(D)`, the source of the reverse relation of `tail_mut` there as well.
Nothing else changes with the shift, since every step only compares what is at the same offset of
both: a run of the target with what lies under it (scalars cut by the shift are decomposed into
byte boxes as misaligned ones are), an enum only with an enum whose base is that offset, a rule
only with the same rule whose struct is at that offset in the source (`mark_rules` from the
shift). Periods stay correct: a period is the least common multiple of the strides of two
repetitions, and is only taken when the target is at the start of one of its elements; what lies
under a run of the target a period later is the same whatever the phase of the two repetitions,
which a shift only changes. The relation reports offsets of the source; messages subtract the
shift. `tail_ref` needs the alignment of the target not to be greater than the one of the
source: the shift is then a multiple of it, as sizes are multiples of alignments.

**Mutable parts** (`mutable_part`). Through `head_mut` or `tail_mut`, any value of the target can
be written over the bytes `shift..shift + size` of the source, and the source must stay valid:

1. the reverse relation over those bytes must be infallible, as `pun_mut` requires of the whole:
   every value of the target, padding included, is valid for what the source has there. A
   scalar of the source that the part covers only in part is not proven (what the rest holds is
   not known), and neither is an enum whose base is outside the part;
2. nothing of the source outside those bytes may depend on them (`cut`): a struct of the source
   with rules (`size`, `len`, `assert`, regions, walks) that covers any of them is refused, since
   its rules read its fields, and so is an enum (or a string) of the source of which they are
   only a part: its tag on one side, fields on the other. A target that takes as many elements
   as fit may end anywhere in the source, and is refused over a source with enums.

This is conservative: a rule that does not read the bytes of the part, or an enum all of whose
variants would accept what is written, is refused all the same. The model check counts these as
imprecise, never as unsound.

Errors are post-monomorphization (`const { .. }` in the generic function), since there are no
const traits to turn them into trait bounds. `cargo check` does not see them, `cargo build` does.

### 3. Run-time validation is type-directed and guarded by the analysis (`validate.rs`)

`try_*` and `from_*` functions recurse over the *types* (associated types: element, list of
fields) in `const fn`s; each check sits behind `if const { marks say so }`. What the analysis
proved never reaches code generation.

Measured on x86-64, `opt-level = 3` (the functions are in `examples/codegen.rs`):

| Function                                           | Machine code                               |
|----------------------------------------------------|--------------------------------------------|
| `&Position -> &[u8; 2]`, `[bool; 4] -> u32`        | `mov rax, rdi; ret`                        |
| `&Frame<Ipv4<[u8]>> -> &[u8]`                      | `lea rdx, [rsi + 34]` (the length)         |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`            |
| `&[u8] -> Option<&Header>`                         | size, 4 bytes compare, `bt`, `cmp`         |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 43 instructions, no loop, no call          |
| `Ipv4::options()` (a region)                       | 16 instructions, no branch                 |
| `&[u8] -> Option<&[bool]>`                         | a loop of one `cmp` per byte               |
| `&[Shape] -> &[Drawing]` (enums with fields)      | `mov rdx, rsi; mov rax, rdi; ret`          |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`: 12   |
| `[u8; 4] -> Option<Shape>`                         | 14 instructions, no loop, no call          |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 362 instructions, two loops, no call       |
| `head_ref::<Ipv6, [u8; 40]>`                       | `mov rax, rdi; ret`                        |
| `tail_ref::<[u32; 4], u32>`                        | `lea rax, [rdi + 12]; ret`                 |
| `from_head::<Position>(&[u8])`                     | the size, one `cmp` for the `bool`: 7      |
| `from_tail::<Trailer>(&[u8])` (an enum, a magic)   | the size, `bt`, one `cmp`: 19, no loop     |

### 4. Rules: what is not a property of bytes

A struct can have rules, which `#[derive(Pun)]` compiles from expressions over its integer
fields to a constant postfix program (`layout::Op`):

- `#[pun(size = EXPR)]`: the size in bytes of the value;
- `#[pun(len = EXPR)]`: the number of elements of its slice;
- `#[pun(assert(EXPR))]`: anything else (bit fields, relations between fields, a minimum size);
- `#[pun(regions(a = EXPR, b))]`: consecutive regions of the tail, with an accessor for each.

An expression can read the value beyond its fields: `size_of_val(self)`, `le(offset, N)` and
`be(offset, N)` (an integer anywhere in the value), `crc32(start, end)` and `sum16(start, end)`
(the frame check sequence of Ethernet, the checksum of IP). So a trailer is a region, and "the 4
last bytes are the CRC of what is before" is an assertion. A checksum that covers several layers
(the one of UDP and TCP covers the addresses of the IP header) is an assertion of the struct that
holds them all: the IPv4 packet.

An expression can name what it computes and choose (`{ let header = ..; .. }`,
`if .. { .. } else { .. }`), and it can **walk**: `walk(START, |at| COND, |at| NEXT)` is what
`START` becomes when it is replaced by `NEXT` for as long as `COND` is true of it. This is how
what lies at an offset that depends on what is before is reached: the checksum of UDP, TCP and
ICMPv6 is checked by the IPv6 packet behind any chain of extension headers, each of which tells
what follows it and how long it is. A value is walked for at most as many steps as it has bytes.

What cannot be computed (an overflow, a read out of the value) makes a rule fail, unless it is
the side of a `&&` or `||` that the other side decides without: `protocol != 17 || be(..) == 0`
holds for a packet that is not UDP, however short.

Two more options are not rules, but save the code that would otherwise be written by hand:

- `#[pun(tag(Key = Type))]` implements `Tag<Key>`: "when carried by something that tells what
  it carries with a `Key`, this type comes with a `Type`". A frame is then
  `struct Frame<P: ?Sized + Tag<EtherType> = [u8]> { .., ether_type: P::Tag, payload: P }`, and
  `Frame<Ipv4>` requires the EtherType of IPv4 while `Frame` takes any (bytes go with any tag);
- `#[pun(get(name: Type = EXPR))]` generates a `const fn` getter, for bit fields. Here `EXPR`
  is plain Rust, in which integer fields are `u128`: any function can be called;
- a region can be given a type, `regions(options = EXPR, payload: P)`: its accessor then returns
  a `&P`, if the bytes of the region are one. This is how a packet whose header has a variable
  size carries a typed payload;
- on an enum whose variants each hold a reference,
  `enum AnyFrame<'a> { Ipv4(&'a Frame<Ipv4>), Arp(&'a Frame<Arp>), Other(&'a Frame) }`, the derive
  generates `from_bytes` and `from_head`, which return the first variant that the bytes are (and
  no `from_tail`: the variants may be unsized): telling one thing from another is a declaration
  too.

Rules are checked when a value comes from something else, tell `from_head` how long a value is, and
are proven only by the same rule, in the same struct, at the same place of the source. The
evaluator (`layout::eval`) is written so that a constant program compiles to its arithmetic: no
loop for the first operations, and a local variable per stack slot rather than an array.

An expression that walks holds two more expressions, which are evaluated in a loop: there, the
operations are not left to the optimizer to find constant. They are constants of the instances
of the functions that apply them (`validate::check_walks`), one instance per operation, reached
by a recursion over a number that is a type. What is compiled is the loop and the arithmetic of
both expressions, never an interpreter; and the list of the rules that walk is itself a type
(`Pun::Walks`), so that a struct without any costs nothing more to compile.

### 5. What the type system enforces, and what it does not

Soundness rests on one distinction (`Spec::valid` and `Spec::known`):

- what a type **guarantees**, whatever safe code does: `bool`, enums (with or without fields),
  `NonZero`, `str`, and library types with a private field and a checked constructor
  (`u16bec<V>`, `u8in<MIN, MAX>`);
- what a struct merely **requires** of what it is made from: `#[pun(valid(..))]` on a plain
  field, and rules. Safe code can write any value to a `pub version: u8`.

A requirement is checked (or proven) on the way in, but it never proves what another type
guarantees: `&[Unchecked] -> &str` needs a check even if `Unchecked` is "`valid(..=0x7f)`", while
`&[u8in<0, 0x7f>] -> &str` does not. For the same reason, the accessors of regions cut regions
short rather than trust the fields that size them. Two layouts are only the same
(`Layout::equals`, by which an enum proves an enum without looking into it) if they require the
same *and* guarantee the same: `V(#[pun(valid(0..=1))] u8)` is not `V(bool)`.

A rule that reads the bytes of its value (`le`, `be`, `crc32`, `sum16`, and `walk` through its
expressions) reads its padding as well. Where such a rule may be checked (the source does not
prove it, or it is in the variant of an enum, which is validated as a whole), the padding of its
struct is a byte that the target needs initialized.

What the type system enforces is all that a type is known by, which has two consequences that
are by design:

- `PhantomData<T>` is a scalar of no byte, `Scalar` for any `T`: two types that differ only by
  phantom parameters or by lifetimes have the same layout, and each converts to the other freely,
  by value and by `&mut`. A type whose parameters are a brand (an invariant lifetime that ties a
  value to a scope, a marker that says what a value was checked against) must not derive `Pun`:
  `pun` would turn one brand into another.
- Deriving `Pun` makes every field as accessible as if it were public: a value of the struct can
  be made from anything whose bytes are valid for its fields and its rules (`pun` and
  `try_pun` check what the derive is told, nothing that a constructor would), read as
  anything its bytes are, and written through a `&mut` to another type. A type whose private
  fields keep an invariant that the derive is not told of must not derive `Pun`.

### 6. What `#[derive(Pun)]` trusts

The derive writes `unsafe impl`s in the crate that uses it, where every path can be made to lead
elsewhere by safe code: `#[pun(crate = ..)]` names any module, `#[pun(crate)]` the root of the
crate, and `extern crate other as core;` makes `::core` another crate. So what the layout is made
of does not depend on any path:

- the functions that the generated code calls are associated functions of the types that the
  implemented traits name (`Layout`, `Spec`, `Field`), or of types that `Layout::PIN` fixes with
  `fn(T)` types (`Rule`, `Op`, `Enum`, `Variant`), which no path can stand for;
- the offsets of the fields are not `offset_of!` (a macro, named by a path): with `repr(C)` they
  are what `repr(C)` specifies, from the sizes and alignments of the fields; otherwise the struct
  is `Sized`, and they are found by projecting a pointer to an uninitialized value, which costs
  nothing in const evaluation whatever the size. The variants of enums with fields are laid out
  as RFC 2195 specifies, from the sizes and alignments of their fields;
- `Path` and `Count` are sealed, and the leaf that a list of fields says a field is at is checked
  against the layout;
- names that start with `__Pun` are reserved, and `Self` in the type of a field is replaced by
  the type it means, since the generated code writes these types where both mean something else.

The rest of `::core` that the derive names (`Option`, `PhantomData`, `usize`...) is checked by the
types of the traits it implements. The derive still trusts the compiler for what it parses
(`repr`), and that the types of the fields are the same wherever it writes them (a procedural
macro in the type of a field that expands to a different type each time it is expanded would break
that).

### 7. Pointers to unsized types (`ptr.rs`)

Splitting and building wide pointers in generic `const fn` code is done by reinterpreting
`*const T` as `*const [u8]`. That representation is not documented, so it is **verified at compile
time for each unsized type** against a pointer made with a genuine `as` cast (`Pun::PROBE`,
`ptr::verify`): if it ever changes, the crate stops compiling instead of misbehaving.

The tests, and the Miri selection of the model check (below), pass under Miri with strict
provenance, with stacked and with tree borrows, on x86-64 and on i686 (a 32-bit `usize`, a `u64`
aligned to 4). With `-Zmiri-symbolic-alignment-check`, Miri reports a misaligned reference where
`try_pun_ref` checked the address of bytes that happened to be aligned: the check reads the
address as an integer (`ptr::is_aligned`), which Miri's symbolic alignment cannot follow, and the
stable `align_offset` is not `const`.

## Model check

`puns-model/` compares every verdict of the analysis with oracles that do not use it,
through the test-only `verdict` feature (`puns::hook`: what each function would do, without
failing to compile): about a hundred types of 1 to 8 bytes, every ordered pair of the same size,
every value of the source over an alphabet (all bytes up to 2 bytes); slices and slice-tailed
structs from every pattern of up to 6 bytes. It tells what a type requires (`valid(..)`, rules)
from what it guarantees, and finds unsound verdicts (a value let through that the type system
does not allow, a byte of padding read), incorrect ones (a valid value rejected at run time) and
imprecise ones (counted).

Parts are checked over the same types, for every pair whose target is not larger than its source
(about four thousand): `head*` and `tail*` against the oracle of the target at offset 0 and at
`size(S) - size(D)`, the bytes and the address that they give at run time, and `head_mut` and
`tail_mut` by writing every value of the target into every value of the source: the result must
be a valid source, and one that still meets its requirements if it did and if what was written
meets those of the target (which is what a rule of the source over the part, or a part of an
enum, would break). A wrong shift, in the analysis or in the pointer, and a `mutable_part`
without its second condition, are caught.

Every check that CI runs on a push takes at most a minute. So `cargo test` runs a sample of the
model check, and Miri a selection of pairs chosen for the unsafe paths they reach, not for the
verdicts: no undefined behaviour is what Miri is there to see (`puns-model/tests/miri_sample.rs`
lists what each pair is for; three slow examples, `archives`, `link` and `routing`, which walk
over hundreds of bytes, are not run under Miri, and other tests run the same paths there). The
exhaustive run, every pair and every part of every type, is
`cargo test --profile model -p puns-model -- --ignored` (optimized, without LTO: minutes to build,
about two to run): the `model.yml` workflow runs it every night and on demand. See
`puns-model/src/lib.rs` for what is checked.

## Not done yet

- **Precise mutable parts**: `head_mut` and `tail_mut` refuse any source with a rule over the
  part, while the operations of a rule tell which fields and bytes it reads (a rule over other
  fields only could be allowed), and any enum only partly covered, while an enum all of whose
  variants accept what is written could be allowed.
- No checked parts of a typed value (`try_head`...): a part is proven or refused; bytes have
  `from_head` and `from_tail`.
- **Enums with fields and no `repr`** (the derive asks for `repr(C)` or `repr(u8)`...; enums
  without fields are supported whatever their `repr`). Their layout is not specified, and stable
  Rust cannot name it, but it can be *observed* in const evaluation from a sample of each
  variant: field offsets by matching on the sample; the tag by "transplanting" bytes of one
  variant over another and matching again. Probed on 1.100: this finds the niche of
  `enum { A(u8, bool), B(u8), C }` (the `bool` byte, values 2 and 3) and the tag of a directly
  tagged enum. An experiment that goes wrong is a compile error (const evaluation rejects invalid
  tags), never undefined behavior. One thing cannot be observed: the *width* of a direct tag
  (its high bytes are zero in every variant, and const evaluation refuses to read bytes that may
  be padding). The engine is ready for it (a niche is a tag whose variants have ranges of
  values); what is missing is a sample of every field type, and the experiments.
- An enum made from something that is not an enum is always checked at run time, even when
  every value of the source happens to be valid.
- Patterns other than `str`: the engine takes any DFA of at most 64 states, but there is no way
  to declare one from the derive.
- `default()`: building a value from the defaults of its fields (literals, sizes).
- An arbitrary function in a rule (getters take any): it could only be called through a trait,
  hence not in `const` contexts on stable. `crc32`, `sum16` and `walk` are built in for that
  reason.
- `walk` is for `assert`: once in an expression, not for sizes, lengths or regions.

## Limits

- The types of the fields of a struct must be at least as visible as the struct: the list of
  fields is an associated type, which the privacy rules of Rust do not allow to name a less
  visible type.
- A struct of more than about a hundred fields may need `#![recursion_limit]` to be raised in
  the crate that validates it.
- A packed struct whose unsized field has trailing padding of its own is rejected.
- An enum with fields of more than about 120 variants needs `#![recursion_limit]` to be raised in
  the crate that validates it (validation recurses over the list of the variants).
- `PhantomData<T>` is `Scalar` for any `T`, and deriving `Pun` exposes private fields: see
  "What the type system enforces" for which types must not derive `Pun`.
- Unions are not planned.
