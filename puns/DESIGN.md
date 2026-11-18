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
media, archives, executables, storage and encodings; and with the feature `nearest`, a compiler IR
that is its own file format) are the reference for what works; each example says what it does
not cover, and why.

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

A value that holds self-relative pointers (a region of `nearest`, see "Self-relative pointers")
is read by these functions as plain data, and made by none of them. It has its own:
`view_ref` (proven, pointers followed, by reference only) and `try_view_ref`, and with the
feature `nearest`, `nearest::view` and `nearest::from_bytes`, which check the bytes of a whole
region. These are not `const fn`s.

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
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 363 instructions, two loops, no call       |
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

An expression means what the same Rust expression means where every integer (a field, a
constant, a literal) is a `u128`, or is refused at compile time: never something else. The derive
types it as Rust does (an integer or a `bool`: `!` is bitwise on one and logical on the other, an
integer is not a condition), reads literals itself (no float, no negative number, no signed
suffix), converts constants through a trait that only unsigned integer types implement, and makes
`as uN` keep the low bits. Where Rust would panic (an overflow, a division by zero, a shift by
128 or more), the rule fails; what Rust does not compute (the right side of a `&&` that the left
side decides, the branch of an `if` that is not taken) cannot make it fail, and what it does (a
`let`, used or not) can: a block ends with `Op::Let`, which fails it if one of its `let`s did,
for the ones that its value may be computed without and that may fail, which const evaluation
tells from the widths of what they read (`Op::fallible`), so that it usually costs nothing. The
derive also counts what the evaluator has room for (the depth of its
stack, the operations of a walk, the variables) and reports an excess at the expression, with the
numbers; the evaluator checks it again.

An expression can read the value beyond its fields: `size_of_val(self)`, `le(offset, N)` and
`be(offset, N)` (an integer anywhere in the value), `crc32(start, end)` and `sum16(start, end)`
(the frame check sequence of Ethernet, the checksum of IP), and variable-length integers:
`leb128(offset)`, `quic(offset)` and `vlq(offset)`, and with `_len` the number of bytes of each
(`Op::Varint`: an offset is replaced by what is there, as `be` does; a read never goes past the
value). So a trailer is a region, and "the 4 last bytes are the CRC of what is before" is an
assertion; an MQTT packet is `size = 1 + leb128_len(1) + leb128(1)`.

Checksums other than these two are told by their parameters (`Op::Crc`, `Op::CrcZeros`,
`Op::Checksum`): `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)` is any CRC of up to 64
bits, whose table of 256 entries (2 KiB) is made in const evaluation, once for each parameters;
with two more offsets, the bytes between them are taken as zeros, for a CRC over the header that
holds it (GPT). `fletcher16`, `adler32` and `sum` are the other checksums of the formats of the
examples. As every read of the value, they are bounded by it: a range out of it fails. A size or a length reads the value too
(not its size, which it defines): `from_head` evaluates it over the bytes that there are, which
are initialized and in bounds whatever it reads, and the value it takes is then validated as a
whole, the rule included, over its own bytes; a rule that reads beyond what it sizes fails there. A checksum that covers several layers
(the one of UDP and TCP covers the addresses of the IP header) is an assertion of the struct that
holds them all: the IPv4 packet.

An expression can name what it computes and choose (`{ let header = ..; .. }`,
`if .. { .. } else { .. }`; what a `let` binds is computed, as in Rust, whether it is used or not),
and it can **walk**: `walk(START, |at| COND, |at| NEXT)` is what
`START` becomes when it is replaced by `NEXT` for as long as `COND` is true of it. This is how
what lies at an offset that depends on what is before is reached: the checksum of UDP, TCP and
ICMPv6 is checked by the IPv6 packet behind any chain of extension headers, each of which tells
what follows it and how long it is; at the end of a string, in a `size`. A walk can be in any rule,
in another walk, several in an expression. A walk takes at most a step for each bit of the value, plus
128 (the bits of a `u128`), so that it ends: a walk over the bits of a field fits, whatever the
size of the value.

What cannot be computed (an overflow, a division by zero, a shift by 128 or more, a read out of
the value) makes a rule fail where Rust would panic, unless it is the right side of a `&&` or
`||` that the left side decides without, as Rust does not compute it: `protocol != 17 ||
be(..) == 0` holds for a packet that is not UDP, however short. `<<` drops the bits that it
shifts out, as Rust does.

Two more options are not rules, but save the code that would otherwise be written by hand:

- `#[pun(tag(Key = Type))]` implements `Tag<Key>`: "when carried by something that tells what
  it carries with a `Key`, this type comes with a `Type`". A frame is then
  `struct Frame<P: ?Sized + Tag<EtherType> = [u8]> { .., ether_type: P::Tag, payload: P }`, and
  `Frame<Ipv4>` requires the EtherType of IPv4 while `Frame` takes any (bytes go with any tag);
- `#[pun(get(name: Type = EXPR))]` generates a `const fn` getter, for bit fields. Here `EXPR`
  is plain Rust, in which an integer field, wherever it is named, is its value as a `u128`: any
  `const fn` can be called;
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
evaluator (`layout::eval`) has a stack of 16 values and 8 variables, each a variable of the
machine rather than an element of an array, reached by a `match` whose cases are all values (the
last one is the default case: a `match` with one more case costs instructions); it is written so
that a constant program compiles to its arithmetic: no
loop for the first operations, and a local variable per stack slot rather than an array.

An expression that walks holds two more expressions, which are evaluated in a loop. The evaluator
applies a walk as it applies anything else (`Machine::walk`): it runs both expressions in a loop,
which the optimizer makes constant where it inlines the evaluator, at the first level of walks.
The first walk of an `assert` is applied otherwise, where it must be fast (the IPv6 packet): there,
the operations are not left to the optimizer to find constant. They are constants of the
instances of the functions that apply them (`validate::check_walks`), one instance per operation,
reached by a recursion over a number that is a type. What is compiled is the loop and the
arithmetic of both expressions, never an interpreter; and the list of the rules that walk is
itself a type (`Pun::Walks`), so that a struct without any costs nothing more to compile. The
recursions are as long as the expressions: this is for an `assert` of at most 72 operations whose
first walk has expressions of at most 56, and the derive leaves anything longer to the evaluator.
Both apply the same operations, with the same limit of steps.

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

### 8. Self-relative pointers (`layout::Pointer`, `deep.rs`)

A value may hold a *self-relative pointer*: the distance from its own address to a target in the
same buffer, as the `Near<T, O>` of the crate `nearest` is (below). The core knows the kind, and
depends on nothing for it:

- a pointer is its **offset**, a signed integer of 2, 4 or 8 bytes in a given byte order, which
  holds distances and lengths up to a limit. It **aims** at one value, at a list (a length, an
  integer like the offset, followed by the elements and preceded by a capacity) or at text (a
  length, then UTF-8). It may be **nullable**: an offset of 0 is then no pointer;
- what it aims at is *not* in the layout: the layout of `struct Node { next: Option<Near<Node>> }`
  would have no end, and const evaluation has no identity of a type by which to stop where it
  has been (`TypeId` is not compared in `const`, pointers neither, and a static reached through
  an associated constant is a cycle). It is a type, `Pun::Pointee` (the element, for a list).
  The layout keeps the size and the alignment of the target, which the compiler gives without
  looking into it.

**What a pointer guarantees is not a property of its bytes**: its target is where it says,
valid, and its own. Three rules follow, which every function keeps.

1. **No bytes prove a pointer.** A pointer of a target is proven only by a pointer of the source
   at the same place whose target it can read (`Pointer::reads_as`): the same offset (width and
   byte order: one is read as the other), the same aim, a target that is not larger nor more
   aligned (a list: elements of the same size, in a block aligned for them), and not nullable
   where the target's is not, unless that is checked. Anything else is `Relation::pointer`,
   which no function accepts and no check makes up for. An enum that holds pointers is proven by
   an enum, variant by variant, or not at all.
2. **Only what proves, or checks, what pointers aim at makes a value that holds one.** `view_ref`
   and `try_view_ref` prove it (below); the loaders of `puns::nearest` check the whole buffer.
   Every other function refuses a target that holds a pointer (`Plan::gate`).
3. **No `&mut` to a pointer, nor next to one.** `mutable` and `mutable_part` refuse a source or a
   target that holds a pointer, wherever it is: through a `&mut`, safe code could write over a
   pointer, swap two, or move one to where it aims at something else.

As a *source*, a pointer is initialized bytes of which nothing more is known: a value that holds
pointers is read as plain data by any function that reads (`as_bytes`, `head_ref`...).

Every public function, and why it is safe with a pointer in its target or in its source:

- `pun`, `head`, `tail`, `try_pun`, `zeroed`: they take and make `Copy` values, and no value that
  holds a pointer is `Copy`. The target is gated all the same (2), and `zeroed` asserts it.
- `pun_ref`, `head_ref`, `tail_ref`, `try_pun_ref`: a target that holds a pointer is refused (2).
  Of a source that does, the bytes are read, and nothing is made of them.
- `pun_mut`, `head_mut`, `tail_mut`, `try_pun_mut`: refused with a pointer on either side (3),
  after the target is (2).
- `as_bytes`: `pun_ref::<S, [u8]>`: the bytes of a pointer are read.
- `from_bytes`, `from_head`, `from_tail`: `try_pun_ref` from bytes (`Plan::TRY_REF`, `FROM_TAIL`):
  the target is refused (2).
- `from_bytes_mut`, `from_head_mut`, `from_tail_mut`: the same plans, then `MUTABLE`: refused (2),
  (3).
- `from_bytes` and `from_head` of a view enum (the derive): calls of `puns::from_bytes` and
  `puns::from_head` for the type of each variant: refused (2).
- getters (`#[pun(get(..))]`), `int`: they read an integer field of a `&self`; a pointer is not one
  (`Layout::as_integer`), which fails to compile.
- accessors of regions, `region`: they need `pun_ref::<S, [u8]>`, and return bytes of the tail:
  read.
- typed regions (`regions(name: P)`): `from_bytes::<P>` of the bytes of the region: refused (2).
- rules (`size`, `len`, `assert`, `regions`): an expression reads unsigned integer fields (a
  pointer is not one: an error of the derive's code) or bytes (`le`, `be`, `crc32`...: read).
- `Tag`, `num::*`, `crc32`, `sum16`: no pointer.
- `view_ref`, `try_view_ref`: a pointer of the target is a pointer of the source (1), what it aims
  at is proven (`deep`, below), and the result is a `&`.
- `nearest::view`, `nearest::view_with`: the buffer is checked whole; the result is a `&` into it.
- `nearest::from_bytes`, `nearest::from_bytes_with`: the bytes are checked whole, then copied by
  `nearest` into a region of its own.
- `layout::*`: values that describe layouts, for the code that the derive writes. They make no
  value of a type, and a layout only stands for a type through `Pun`, an `unsafe trait`.
- `hook::*` (the test-only feature `verdict`): the same plans, answered instead of enforced. Its
  `view_ref` follows pointers a bounded number of times: it is for the model check, whose types
  that bound covers.

**`view_ref`: what pointers aim at, proven by instances of functions (`deep.rs`).** `view_ref::<S,
D>` runs the plan of `pun_ref` with pointers allowed, then proves, for every pointer of `D`, that
the `X` its source pointer aims at can be read as the `Y` it aims at itself: `Y` is the first
bytes of `X`, every `X` is valid for them, and nothing is left to check (nothing can be, behind a
pointer). The pointers of `Y` are then pointers of `X`, and so on. For a recursive type this
never ends as an analysis; as *instances* it does:

- `target::<X, Y>` holds, in a `const` block, the proof of one pair up to the next pointers
  (`behind`: the analysis of `X` as `Y`, and `local`, below);
- `pair::<A, B>` *names* `target::<A::Pointee, B::Pointee>` for the pointers that `A` and `B` hold
  at the same place, through their fields, variants and elements. A named function is an
  instance, which exists once however often it is named: a recursive type closes a cycle, and
  the proof is an induction over the pairs of types that can be met.

`view_ref` calls `proof`, and with it `pair` over everything up to the first pointers: `const`
blocks of code that runs, which the language evaluates. Behind a pointer, `target` is named and
never called (calling it would not end): its `const` block is evaluated because **the compiler
makes every function that code it compiles names**, whether that code runs or not and whatever
the optimizations (it has to build the function pointer; `rustc` collects what a function
mentions before it optimizes, so that errors of const evaluation do not depend on the
optimization level). This is what the compiler does, more than what the Reference promises of a
`const` block that is not executed: the compile-fail test `view_behind_pointers` (a `u8` read as
a `bool` two pointers down a recursive type) holds the compiler to it in a debug build, and
`model.yml` builds the same program optimized, with LTO, every night: it must be refused there
too (to run when `rust-toolchain` changes). For the same reason `view_ref` is not a `const fn`: a
call in a constant would make no instance. An interpreter that only evaluates the constants of
the functions it runs, as Miri does, would not refuse such a view: the refusal is one of a build.

The compiler also makes the instances that a branch names when a constant rules the branch out,
so `pair` cannot choose what it names: it pairs by position (fields with fields and variants with
variants, in the order of their declarations; elements with elements), which names more pairs
than matter. Where the source has nothing, its side is `()`, and `target` has nothing to prove
of a type without size (no pointer aims at one: `Pointer::new`). The pairs that matter are
decided exactly by `local`, which is `const` code: every pointer of the target must be, in the
source, a pointer at the same offset that `pair` pairs it with, or the build fails. So what
`local` accepts, `pair` names, by the same walk of the same lists; and a pair that is named
without mattering has one side without pointers. Those lists (`Fields`, `Variants`, `Elem`,
`Pointee`) are types, written next to the layout that the analysis went by: `local` checks, on
its way, that they are what the layouts hold (`listed`: the same fields in the order of their
declarations, the same variants, the same elements, a target of the same size), so that a
pointer is not missed, nor taken for another, by an implementation of `Pun` whose lists and
layout differ (compile-fail test `lists_not_the_layout`). Hence the limits, which are refusals:
a field that holds a pointer keeps its position among the fields of its struct, a variant that
does keeps its position among the variants of its enum (new ones go last: a variant found by its
tag at another position is refused, compile-fail test `view_variant_moved`), and a struct that
only holds a pointer is not that pointer.

`try_view_ref` checks what is not proven of the value itself (`validate`, with the marks of the
analysis: a variant that the target does not have, a restricted scalar, a pointer that may be
null read as one that is not), and requires of what it points to what `view_ref` does.

### 9. Regions of `nearest` (`nearest.rs`, feature `nearest`)

[`nearest`](https://docs.rs/nearest) keeps a value and everything it points to in one buffer, a
region, with `Near<T, O>` pointers whose offset `O` is an `unsafe trait` (`Offset`). The feature
makes a type of a region a `Pun` type, declared once (`#[flat]`, then `#[derive(Pun)]`). It lives
in this crate, not in `nearest` nor in a third one: `Pun` for `Option<Near<T, O>>` can only be
implemented where `Pun` is (`Option` is foreign to `nearest`, everything is to a third crate),
`nearest` has no dependency and an older minimum compiler, and the invariants at stake (bytes
that become a `Near`, a `&mut` to one) are broken or kept by the functions of *this* crate. Without
the feature there is no dependency, and the crate stays `no_std` with it (`nearest` without its
`std`, with `alloc`).

**Offsets of a given byte order.** `i16le`, `i32le`, `i64le` and their big-endian counterparts
(`num.rs`: byte arrays, of no alignment) implement `Offset`, so that the bytes of a region do not
depend on the machine: `from_isize` and `to_isize` are `new` and `get`. The contract of `Offset`
asks for a `NonZero`: the offset without zero, of its size and alignment, of which `Option` has
the size with `None` as zeros. It is `#[repr(C, packed)] struct(NonZero<i32>)`: packing gives the
alignment of the byte array (1) and keeps the niche; `Option` of it is asserted to have its size,
and since every other bit pattern is a value, `None` can only be zeros. `nearest` checks the
layout half of the contract where a region uses the offset; the tests run those of `nearest` for
its own offsets (`tests/width.rs`) with these, and check the rest on values (`contract`).
`Order` (an `unsafe trait`) tells the byte order of an offset to the layout: `to_isize` must be
the integer that the bytes are; it is implemented for `i16`, `i32`, `i64` (the byte order of the
target) and for the six above.

**`Flat` for numbers.** A field of a region is `Flat`. The numbers of a byte order implement it
by forwarding to the byte array they are (`store`, `edit`, `check` of `[u8; N]`; `edit` gives a
`&mut` through `pun_mut`, proven), and the numbers of given values (`u32lec<V>`, `u8in`) the same
way with a `check` that is `try_pun` from the bytes: a magic number is checked by `nearest`'s own
loader as by this one. This uses the hidden items of `Flat` (`PLAIN`, `ANY`, `store`, `edit`,
`check`) and `__private::{Buf, Sink, Check, fits}`, which are the plumbing of `#[flat]`.

**Loaders.** `view` (in place, at an address that is a multiple of 8) and `from_bytes` (a copy in
a region), and `view_with` and `from_bytes_with` with the memory of the caller, check that bytes
are the *compact image* of a value, as `nearest::view` does, by a walk that recurses over the
type as `validate` does (what the layout says cannot be invalid is not compiled, nor walked):

- the walk mirrors the one of `nearest` (`bytes.rs`), so that both stop at the same place: the
  root, then every target in the order a deep copy writes it, each at the next position aligned
  for it. A pointer must aim at the very position where the copy puts its target (`claim`), which
  gives at once bounds, alignment, a tree (no target shared, no cycle) and no gap; lists have
  `cap == len`, lengths are not negative, text is UTF-8, nothing follows the last target, and the
  whole is no longer than the pointers of the root reach;
- it is safe code that does not recurse through pointers (what is left to check waits on a
  stack, a vector or the scratch of the caller: `Error::ScratchFull` when that runs out) and does
  not panic, but for the reads of scalars and of offsets, which are in bounds;
- what `#[derive(Pun)]` is told of the types is checked on the way: `valid(..)` on a field,
  `assert(..)` and the other rules of a struct (`validate::check_rules`). These are requirements
  of this crate, which `nearest` does not know: with them the loaders accept *fewer* bytes than
  `nearest`'s; without, the same bytes, with the same errors (tests: a corpus of images and
  their corruptions, both loaders compared, also with every size of scratch);
- then the bytes are cast by `nearest::view_unchecked`, or copied by `Region::from_bytes_unchecked`
  (into the heap, which holds any image): no `Near` is made here.

`agrees` compares, at compile time, what `nearest` is told of a type with its layout: a type that
`nearest` copies as bytes (`PLAIN`) or does not check (`ANY`) must be one, and the largest region
(`REACH`) must be the same for both. An `unsafe impl Flat` that says more is refused.

**What this relies on of `nearest`**, 0.7.1: its byte format (the blocks of lists and of text,
the order of a deep copy), which `nearest.rs` and `Aim` state again; that its unchecked loaders
accept bytes that are the compact image of a valid value, which is what its own loaders give
them; the hidden items above. A release of `nearest` that changes one of them is one that this
crate has to follow, so the dependency is on that very release (`=0.7.1`): with another one, the
build fails instead of a loader checking bytes for a format that is no longer the one they are
read with. The corpus tests and the model check compare the two loaders on every push.

**The invariants of `nearest`** (I1 to I9 of its specification), with this crate in the build:

- I1, a `Near` only exists in a region, where its target is at its offset: rules 1 and 2. No
  function makes a pointer from bytes or from another value; a view is of a value that is in a
  region already, or of bytes checked whole; a pointer read as another is the same offset, to a
  target proven to be what the other aims at.
- I2, no `&mut` to a pointer: rule 3: every `&mut` refuses a pointer on either side.
- I3, one handle chain: untouched: nothing here makes a handle; a view borrows what it is made
  from, as a `&`.
- I4, a tree: checked by `claim` when bytes are loaded; a view reads the same tree.
- I5 to I8, append only, copies, panics, provenance: untouched: nothing here writes to a region,
  copies inside one or reaches its buffer.
- I9, an edit stays on one thread: untouched: only `&T` and `Region` are made, which are `Send` and
  `Sync` as `nearest` makes them.
- the buffer, `len <= cap <= mem <= max`: a region is only made by `Region::from_bytes_unchecked`,
  from bytes no longer than the limit.

**Invariants that this adds**, each kept where it is said:

- P1. A `Kind::Pointer` layout is made by `Layout::pointer` only, for a type that is the offset
  and nothing else (its size is asserted), aiming at a type with a size (`Pointer::new`).
- P2. `Relation::pointer` is set for every pointer of a target that a pointer of the source does
  not prove (`relate_range`, `relate_enums`, the two places that give up on proving), and every
  plan refuses it, fallible or not (`check`).
- P3. Every plan but `VIEW` and `TRY_VIEW` refuses a target that holds a pointer (`Plan::gate`,
  called by `Plan::check` and `Plan::check_tail`, which every plan goes through).
- P4. `mutable` and `mutable_part` are false with a pointer on either side.
- P5. `view_ref` and `try_view_ref` are the only functions with the plans of P3, and both call
  `deep::proof` for the same pair of types; neither is a `const fn`.
- P6. What `deep::local` accepts, `deep::pair` names: both walk the fields, the variants and
  the elements of the two types by position, and `local` refuses whatever is not at the
  position where `pair` looks for it, and a type whose lists are not what its layout holds
  (`listed`).
- P7. The offsets: `to_isize` of an `Order` type is the integer of its bytes in `ENDIAN`
  (an `unsafe trait`; for the nine types here, by construction and by the test `contract`).
- P8. A loader casts or copies only bytes that its walk accepted whole, for the very type.
- P9. The `Flat` implementations of this crate forward to the bytes of the type, and say `ANY`
  only of types of which every bit pattern is a value (`agrees` compares them with the layouts
  wherever a loader is used).

Measured on x86-64, `opt-level = 3` (the functions are in `examples/codegen_nearest.rs`):

| Function                                            | Machine code                               |
|-----------------------------------------------------|--------------------------------------------|
| `view_ref::<Old, New>` (a recursive enum)           | `mov rax, rdi; ret`                        |
| `view_ref::<[Old], [New]>`                          | `mov rdx, rsi; mov rax, rdi; ret`          |
| `try_view_ref::<Maybe, Owned>` (a pointer not null) | one `cmp dword ptr [rdi + 4], 0`: 4        |
| `view_with::<Table>`: the entry                     | 149 instructions (`nearest`'s: 145)        |
| `view_with::<Table>`: its steps (lists, text, rows) | 469 instructions (`nearest`'s: 455)        |
| the step of a list of numbers                       | 83, no loop: its elements are not walked   |

The tests of the feature pass under Miri with tree borrows (which `nearest` needs: a
self-relative pointer is resolved from its own address, which stacked borrows do not allow) and
strict provenance, on x86-64, on i686 and on s390x, a big-endian target, where the image that
`examples/ir.rs` builds is, byte for byte, the one that is committed (`examples/ir.image`).

An offset or a length is one load. On a tree of 2^19 enums the check takes 1.09 times the time
of `nearest`'s (4.1 ms against 3.8 ms); on a table of 4 million numbers and 100 000 names, the
same time (1.7 ms): what cannot be invalid is not looked at by either.

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
lists what each pair is for). The examples other than `ethernet` are not run under Miri either:
they are declarations, and the unsafe code they reach is that of the library, which its own
tests and `ethernet` run there. The
exhaustive run, every pair and every part of every type, is behind the `full` feature, which
`cargo test` does not build: `cargo test --profile model -p puns-model --features full`
(optimized, without LTO: minutes to build, two to run). The `model.yml` workflow runs it every
night and on demand. See
`puns-model/src/lib.rs` for what is checked.

Self-relative pointers have a model of their own (`puns_model::pointers`, with the feature
`nearest` of the model, which `cargo test` does not turn on). Sixteen types that hold pointers
(recursive structs, through an optional pointer and through a list; enums; one pointer to a
`bool`, a `u8`, two bytes, a list, text; another byte order; bytes) are each described by hand,
and the oracle is a reference validator that walks concrete bytes with such a description:
every byte it reads is in bounds and was written to, every value is valid, every pointer leads
to a place aligned for its target, which holds one. Every value of a corpus of regions of a
source is read as each target: what `view_ref` accepts must be valid (else it is unsound), and
is then read through under Miri; `try_view_ref` must accept the values that are valid and only
them; and the loaders of `puns` and of `nearest` are given every image and its corruptions,
must agree, and must only accept what the oracle finds valid. What the oracle alone says of
pairs whose answer is known is asserted, so that it is not an oracle that accepts everything.
`hook::views` answers with the `const` code that decides the pairing (`deep::local`, pointers
followed a bounded number of times); the functions themselves are called for pairs that it
accepts, and the compile-fail tests are the pairs that they refuse. A sample runs on a push
(`cargo test -p puns-model --features nearest`), every pair of every type and every value of
every byte with `full`, at night. It counts as imprecise what is refused by design: a target of
another size, a list or text read as something else, a check that would be behind a pointer.

## Not done yet

- **Checks behind pointers**: `try_view_ref` checks the value it is given, and asks of what that
  value points to what `view_ref` does. Checking behind pointers is a walk of the region with
  the marks of each pair of types, which the loaders are close to (they walk with every mark).
- **A region as a region of another type**: `view_ref` reads a value where it is; making a
  `Region<B>` of a `Region<A>` without a copy needs the two images to be the same bytes (the
  same sizes behind every pointer), which the analysis does not tell yet. Today it is a deep
  copy, `Region::new(view)`.
- **`from_bytes` into a storage other than the heap** (`nearest`'s `Inline<N>`...): the loader
  of `nearest` that copies without checking panics when the storage is full, where the checked
  one returns an error, so only the heap is offered. See also "Limits" for 32-bit targets.
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
- Patterns other than `str` as types: the engine takes any DFA of at most 64 states, as a type
  that guarantees it, but such a type cannot come from the derive, whose fields safe code can
  write: a struct requires a pattern of its bytes with a rule, `matches(DFA, start, end)`
  (`Op::Matches`), proven only by the same rule, as any rule. Automata are compared by their
  states and edges, not by their `id`, which a user can choose.
- A byte order that a value tells (`EI_DATA` of ELF, the byte order mark of TIFF): a type has a
  byte order of its own (`u32be`, `i16le`, `f64be`...), so this is a choice between two types,
  which an enum of references makes.
- `default()`: building a value from the defaults of its fields (literals, sizes).
- A rule names the integer fields of its struct, not a field of a field (`header.length`) or an
  element of an array (`magic[0]`), which it reads with `be` or `le` at their offsets. Loading
  them would be sound if the offset were checked against the layout of the field (a scalar
  there, of that size), as it is for the fields of the struct.
- Rules compute on unsigned integers: a signed or floating-point field is refused, and read as
  bits with `le` or `be`. A getter reads fields, not bytes: a value that only the bytes hold
  (a variable-length integer) is decoded by use code.
- A size that is not in the value: "to the end of what holds it" (an MP4 box of size 0).
- Sums of types in sequences, and recursion: `OPTIONAL` and `CHOICE` of ASN.1, the containers of
  CBOR and MessagePack. A chain of types (`Then<A, B>`) is one sequence of types.
- An arbitrary function in a rule (getters take any). The validator is `const fn` code, generic
  over the type it validates, and on stable 1.100 such code cannot call a function that the type
  chooses: a call through a function pointer is refused ("function pointer calls are not allowed
  in constant functions"), a trait method too (E0015, "cannot call non-const associated
  function"), and const traits are unstable (E0658), all probed. A user function could only be
  called if validation stopped being `const`, which every function of the crate is, or behind
  a check that only runs outside of `const`, which stable Rust cannot tell. So what a rule
  computes is built in, and described by data where it varies: walks, variable-length integers,
  CRCs by their parameters, sums. The data is all the analysis sees, and it does not reason about
  it (a rule is proven only by the same rule): a checksum only makes a check stricter.

## Limits

- Pointers are paired by position: reading a value that holds pointers as another type
  (`view_ref`) needs the fields that hold pointers at the same position in both structs, the
  variants that do at the same position in both enums, and does not look through a struct that
  only holds a pointer. See "Self-relative pointers".
- The derive bounds its implementation by "the type of each field that depends on a parameter is
  `Pun`", but for the fields that name the type itself (a recursive type would be `Pun` if it
  is). So the parameter that is the offset of a recursive type is declared with what its
  pointers need, `O: puns::nearest::Order`; and two generic types that hold each other, whose
  bounds would be found in a circle (`overflow evaluating the requirement`), tell theirs:
  `#[pun(bound())]` on both when the bounds that the types declare are enough, as `O: Order` is.
- `nearest` keeps the buffer of a region in `u64`s, which are aligned to 4 on some 32-bit targets
  (i686): there, the bytes of a region are not always at a multiple of 8, which `view` requires
  (give it a copy that is), and `nearest` 0.7.1 itself reads a type aligned to 8 misaligned.
  This crate's loaders are not concerned (they check the address, and cast what they checked),
  the regions that `nearest` builds are.
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
- The evaluator of rules has a stack of 16 values and 8 variables (the derive says when an
  expression needs more, and how much), and a walk takes at most `8 * size + 128` steps.
- Unions are not planned.
