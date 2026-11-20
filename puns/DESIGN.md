# Design

This document explains how `puns` works and what its soundness rests on. The documentation of
the crate states what each function guarantees. This document says how the code keeps those
guarantees, and why it is built that way.

## Goal

`puns` is type punning proven at compile time, on stable Rust. It is a safe alternative to
`transmute` and to parsing bytes by hand. A *pun* reads the bytes of a value of one type, the
*source* `S`, as a value of another type, the *target* `D`. For each pun, the analysis decides
at compile time which of three cases holds.

- The pun is **proven** if every value of the source is a [valid] value of the target. It
  compiles to no code.
- The pun is **checked** if it is valid for some values of the source. A `try_` or `from_`
  function then tests at run time what the analysis could not prove, and nothing else.
- The pun is **refused** if it is never valid, or if what it needs can be neither proven nor
  checked. It does not compile.

The types are plain data. They are structs and enums of any representation, `repr(C)` unions,
tuples, arrays, slices, structs that end with a slice, `str`, `MaybeUninit`, and types without
values. For an enum without `repr`, `puns` finds the layout that `rustc` chose (section 10).
`#[derive(Pun)]` declares what the data itself says of a value: its size, the length of its
slice, and the conditions that it meets.

The tests (`tests/`) and the examples (`examples/`) are the reference for what works. The
examples cover Ethernet, services over IP, transports, routing, links, images, media, archives,
executables, storage and encodings. Each example says what it does not cover, and why.

## API

The conversions form a grid. A row says what is read: a typed value, or bytes. A column says
how much of it is read: the whole, its front, or its back.

|               | Whole                                         | Front                          | Back                           |
|---------------|-----------------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut` (and `try_pun*`)  | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `from_bytes_mut`, `as_bytes`    | `from_head`, `from_head_mut`   | `from_tail`, `from_tail_mut`   |

- The functions of the first row are proven, except `try_pun`, `try_pun_ref` and
  `try_pun_mut`, which are checked.
- A part is the first or the last `size_of::<D>()` bytes. If the target of `head`, `head_ref`
  or `head_mut` is a slice, the part is as many elements as fit. `tail`, `tail_ref` and
  `tail_mut` need a sized source.
- The functions of the second row are checked, in the following order. `from_bytes` and
  `from_bytes_mut` check the address, then the size. `from_head` and `from_head_mut` check that
  the bytes hold the sized part of the target, then the address, then the extent that its
  rules give. `from_tail` and `from_tail_mut` check the size first, since where the last bytes
  start depends on it, then the address. All of them then check what the bytes do not prove of
  the target.
- `from_bytes`, `from_bytes_mut` and `as_bytes` are `try_pun_ref::<[u8], D>`,
  `try_pun_mut::<[u8], D>` and `pun_ref::<S, [u8]>`, with the same behavior. They fix one side
  to bytes, so they take one type parameter. Without them, an array of unsuffixed integer
  literals would be inferred as an `[i32; N]`.
- `from_head` and `from_head_mut` take as many bytes as the `size` or `len` rule of the target
  says, or else as many elements as fit. `from_tail` and `from_tail_mut` need a sized target,
  because the extent of an unsized target cannot be found from the end.
- A view enum is an enum whose variants each hold a reference. The derive gives it `from_bytes`
  and `from_head`, which return the first variant that the bytes are ("Views", section 4).
- Every function of the grid is a `const fn`.

A value that holds self-relative pointers (section 8) is read by these functions as plain data.
None of them makes such a value. `view_ref` and `try_view_ref` make it. `view_ref` is proven and
follows the pointers. `try_view_ref` also checks at run time what is not proven of the value
itself. Both work by reference only, and neither is a `const fn` (section 8 says why).

## What stable Rust gives us

The design depends on what stable Rust can do in `const`. The table lists what `puns` needs, and
the first stable release that has it.

| Needed                                                     | Stable                               |
|------------------------------------------------------------|--------------------------------------|
| [`TransmuteFrom`], [`ptr::metadata`]                       | no                                   |
| [const traits], [`generic_const_exprs`]                    | no                                   |
| reflection, [`offset_of!` on enums and slices][offset_of]  | no                                   |
| [`offset_of!`][offset_of] on sized fields                  | [1.77][1.77], also in unsized structs |
| inline `const { .. }` in generic code                      | [1.79][1.79]                         |
| `&mut` in a `const fn`, with loops and raw pointers        | [1.83][1.83]                         |
| [`size_of_val_raw`] and [`align_of_val_raw`] in `const`    | 1.99                                 |

The last row sets `rust-version` to 1.99. `puns` reads the layout of an unsized type (a struct
that ends with a slice or a `str`) in `const`, from a raw pointer with a made-up length
(`Pun::PROBE`). `size_of_val` cannot do this. It needs a reference, and a reference needs a
valid value. On 1.98, `puns` does not build, because `layout_for_ptr` is unstable there and so
is its use in `const`. The CI job "Minimum Rust version" builds the crate and its examples on
1.99.

Const evaluation is bounded. The deny-by-default lint [`long_running_const_eval`] stops it after
2,000,000 basic blocks ([`LINT_TERMINATOR_LIMIT`] in `rustc`). So the analysis cannot take a step
for each byte of a type, nor a step or an instance for each element of an array. Its cost grows
with the number of fields of the types, not with their size (section 2).

## Architecture

### 1. The layout of a type is a value, not a type (`layout.rs`)

`Pun::LAYOUT` is a `const` tree. `const fn`s build it from what the compiler says of the type:
`size_of`, the offsets of the fields, `size_of_val_raw`, and so on. A node is one of the
following.

- A **scalar** is at most 16 bytes, in a byte order, with a sorted list of valid ranges. A
  scalar that restricts its values is 1 to 16 bytes. `bool` is `0..=1`, `char` is two ranges,
  and `NonZero` is `1..`. An enum without fields is the values of its variants, read from the
  compiler whatever its `repr`. A scalar has two sets of values. `valid` is what it requires,
  and `known` is what every value of it is. The two are the same, with two exceptions. A field
  restricted by `#[pun(valid(..))]` requires values that it is not known to hold (section 5). A
  scalar made by `Spec::maybe_uninit` requires some values, and is known to be anything,
  padding included.
- A **struct** has its fields at their offsets, which come from the compiler for any `repr`.
  The bytes that no field covers are padding. A struct may have **rules** (section 4).
- An **enum** with fields has a tag, which is a scalar at a fixed place. Each variant has its
  tags and a struct of its fields. The representations that Rust specifies (`repr(C)`,
  `repr(u8)` and the other primitive representations, [RFC 2195]) are made of `repr(C)` structs.
  The derive declares those structs and asks the compiler for their layouts. The result is
  checked against the size and the alignment of the enum. For an enum without `repr`,
  experiments in const evaluation find the layout that `rustc` chose (section 10). That layout
  has a direct tag, or a *niche* in the fields of one variant. That variant then has several
  tags, the values of the scalar that holds the niche, and holds the tag in its fields
  (`Variant::holds_tag`). A variant that holds a type without values has no tag, since no value
  of it exists.
- A **union** (`repr(C)`) is an enum without a tag, whose variants are its fields, each at
  offset 0. A value of it must be one of its fields (`Enum::union`, section 5).
- A type **without values** is an enum without variants (`Layout::uninhabited`). `Infallible`
  and enums without variants are such types. No value has their layout
  (`Layout::is_inhabited`), so as a source such a type proves any target.
- `MaybeUninit<T>` is padding of the size of `T`.
- An **array** or a **slice** is the layout of an element and a count. A slice has no count.
- A **pattern** is a tail of bytes that an automaton (a DFA) accepts. `str` is the automaton of
  UTF-8, of 8 states. An automaton is known by its states and its edges. Its `id`, a field that
  safe code can write, only tells two automata apart. Two patterns are the same layout only if
  their automata are the same, state by state. A value is checked with `str::from_utf8` only
  where the automaton is the one of UTF-8, which is decided at compile time.

This tree is an automaton in a compact, hierarchical form. A position in it is a state.

### 2. The analysis is ordinary `const fn` code over those values (`analysis.rs`)

For a source `S` and a target `D`, `analyze` decides the inclusion of languages,
`L(S) ⊆ L(D)`. It walks both trees in step, as the product of two automata. The following keeps
it cheap.

- **Runs.** A subtree without constraints is one step, whatever its size. `[u8; 1 << 40]` is one
  step.
- **Scalars are compared as sets, not byte by byte.** Two aligned scalars compare their lists of
  ranges. Scalars that are misaligned, or of different byte orders, go through an exact
  decomposition of ranges of integers into boxes of bytes (`digit_box`, `box_meets`). So
  `NonZero<u16>` in little endian is known to be `NonZero<u16>` in big endian, and
  `u32 >= 0x100` is known not to be.
- **Periods.** Arrays and slices are not unrolled. When the target is at the start of an element
  and the source is inside a repetition, the analysis relates one period, the least common
  multiple of both strides, and skips the rest (`find_skip`). Two slice tails, which never end,
  are settled in one period.
- **Patterns** run with sets of states over the runs of the other side, each box of a scalar on
  its own. A repetition ends as soon as one of its elements leaves the set unchanged. Two
  patterns are compared by the product of their automata.
- **Enums** are related variant by variant. Each variant of the source is related to the
  variants of the target whose tags it has, as two structs, knowing the tag (`Known`). Knowing
  the tags of both proves a niche of the target that the source variant has as padding. A
  variant of the source with no tag of the target is not valid. A variant of the source is
  proven if all its tags are tags of one variant of the target, which it proves.
- **An enum from something else.** The source may not be an enum, or be an enum whose tag is
  elsewhere. The analysis then finds what the source holds at the place of the tag
  (`tag_values`): what a scalar of the same size and byte order is known to be, or else any
  value. Each variant whose tags meet those values is related to the source over the bytes of
  the enum, knowing the tag. The enum is proven if every value that the source may hold there is
  the tag of a variant that has values, and if each such variant is proven (`relate_enum_from`).
  Otherwise the enum is checked as a whole at run time. A variant that the source can never be
  is not looked at. A byte that a possible variant requires to be initialized must be
  initialized in the source. A source that may be padding at the tag is an error.
- **An enum as a source** is known by its tag, and by the bytes that every variant with values
  initializes.
- **A union as a target** is proven by one of its fields that the source proves. Otherwise it is
  checked field by field at run time, if the check of no field reads a byte that may be padding
  in the source. Otherwise it is refused (`relate_union_from`). As a source, a union is padding.
  From a union, only the same union proves a union (`Layout::equals`).
- **Types without values** are no value of any type that has some (`relate_at` reports the pun
  as never valid). A variant or a struct that holds one has no value either.

The analysis answers the following questions.

- **Fit.** How the sizes map: always, for some lengths of a slice, never, or truncated. It
  accounts for the trailing padding of unsized structs, and is checked against
  `size_of_val_raw`.
- **Uninitialized bytes.** Whether a byte that the target requires to be initialized would be
  read from padding. This is always an error.
- **Validity.** Whether the pun is infallible, fallible, or never valid.
- **Marks.** Which scalars, patterns and rules of the target the source does not prove. These
  are exactly what a checked pun must check at run time.
- **Mutability.** Whether the reverse inclusion holds, which is what a `&mut` requires.

**Shifted relations** (`relate_at`, `analyze_tail`). The cursors of both layouts count offsets
from the start of the source, and each starts at its own base. The target of `tail` starts at
`shift = size(S) - size(D)`. The source of the reverse relation of `tail_mut` starts there as
well. Nothing else changes with the shift, because every step compares only what is at the same
offset of both sides. A run of the target is compared with what lies under it. Scalars that the
shift cuts are decomposed into boxes of bytes, as misaligned scalars are. An enum is related
only with an enum whose base is at the same offset. The rules of a struct are proven only by the
same rules of a struct of the same extent at that offset of the source (`mark_rules` from the
shift, section 4). Periods stay correct. A period is taken only when the target is at the start
of one of its elements. What lies under a run of the target one period later does not depend on
the phase of the two repetitions, and a shift only changes that phase. The relation reports
offsets of the source, and messages subtract the shift. `tail_ref` requires the target to be no
more aligned than the source. The shift is then a multiple of the alignment of the target,
because sizes are multiples of alignments.

**Mutable parts** (`mutable_part`). Through `head_mut` or `tail_mut`, safe code may write any
value of the target over the bytes `shift..shift + size` of the source. The source must stay
valid, which requires two conditions.

1. The reverse relation over those bytes must be infallible, as `pun_mut` requires of the whole.
   Every value of the target, padding included, must be valid for what the source has there. A
   scalar of the source that the part covers only in part is not proven, since what its other
   bytes hold is not known. Neither is an enum whose base is outside the part.
2. Nothing of the source outside those bytes may depend on them (`cut`). A struct of the source
   with rules (`size`, `len`, `assert`, regions, walks) that covers any of the bytes is refused,
   because its rules read its fields. So is an enum, or a string, of the source of which the
   bytes are only a part, such as its tag on one side and fields on the other. A target that
   takes as many elements as fit may end anywhere in the source, so it is refused over a source
   with enums. One exception holds for a struct with rules that lies within the bytes. It is
   accepted if the target has the same rules over the same extent there (`proves_rules`, the
   test of section 4 in the other direction). What is written through the target then meets the
   rules of the source if it meets those of the target. The same holds of the elements of an
   array that lie within the bytes. They are accepted at once where the target has elements of
   the same size at the same place, or else one by one, up to 64 elements.

This is conservative. A rule that does not read the bytes of the part is refused all the same,
and so is an enum all of whose variants would accept what is written. The model check counts
these refusals as imprecise, never as unsound.

The errors of the analysis are panics of `const { .. }` blocks in the generic functions. Rust has
no const traits on stable, which would let these conditions be trait bounds. A `const` block
that depends on type parameters is evaluated after monomorphization. So `cargo check` does not
report these errors, and `cargo build` does.

### 3. Run-time validation is type-directed and guarded by the analysis (`validate.rs`)

The `try_` and `from_` functions recurse over the *types*, in `const fn`s, through associated
types: the element of an array, the list of the fields of a struct. Each check sits behind an
`if const { .. }` on the marks of the analysis. What the analysis proved never reaches code
generation.

The table gives the machine code of the functions of `examples/codegen.rs`. It was measured on
`x86_64-unknown-linux-gnu` with `rustc` 1.100.0, the release profile of the workspace
(`opt-level = 3`, LTO, one codegen unit) and the `unstable` feature, with the command at the top
of `examples/codegen.rs`. A count of instructions excludes labels and directives.

| Function                                           | Machine code                                  |
|----------------------------------------------------|-----------------------------------------------|
| `&Position -> &[u8; 2]`                            | `mov rax, rdi; ret`                           |
| `[bool; 4] -> u32`                                 | `mov eax, edi; ret`                           |
| `&Frame<Ipv4<[u8]>> -> &[u8]`                      | `lea rdx, [rsi + 34]` (the length)            |
| `&Packet<Datagram> -> &Packet` (the same rules)    | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`               |
| `&[u8] -> Option<&Header>`                         | the size, a 4-byte compare, `bt`, `cmp`       |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 29 instructions, no loop, no call             |
| `Ipv4::options()` (a region)                       | 10 instructions, no branch                    |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, no branch in a block, 46 instructions |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`, 12 instructions |
| `[u8; 4] -> Option<Shape>`                         | 14 instructions, no loop, no call             |
| `Ticket <-> u32` (no `repr`, a niche in `NonZero`) | `mov eax, edi; ret`, both ways                |
| `[u8; 4] -> Option<Small>` (no `repr`, direct tag) | the tag, one `cmp` for the `bool`, 13 instructions |
| `u8 -> Option<Choice>` (a union of `bool`, `u8in`) | one `cmp cl, 4`, 7 instructions               |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 207 instructions, no call                     |
| `head_ref::<Ipv6, [u8; 40]>`                       | `mov rax, rdi; ret`                           |
| `tail_ref::<[u32; 4], u32>`                        | `lea rax, [rdi + 12]; ret`                    |
| `from_head::<Position>(&[u8])`                     | the size, one `cmp` for the `bool`, 7 instructions |
| `from_head::<Udp>(&[u8])` (a size in the value)    | 15 instructions                               |
| `from_tail::<Trailer>(&[u8])` (an enum, a magic)   | the size, `bt`, one `cmp`, 18 instructions, no loop |
| `view_ref::<Old, New>` (recursive, with pointers)  | `mov rax, rdi; ret`                           |
| `view_ref::<[Old], [New]>`                         | `mov rdx, rsi; mov rax, rdi; ret`             |
| `try_view_ref::<Maybe, Owned>` (checks a pointer)  | one `cmp dword ptr [rdi + 4], 0`, 4 instructions |

A count is not a time. The check of a slice of `bool`s takes 64 bytes at a time with no branch
inside a block. It has more instructions than a loop of one `cmp` per byte that stops at the
first invalid byte, and takes a fraction of its time on a long slice. `benches/checks.rs` times
the checks next to the same checks written by hand.

`from_head` turns the value of a `size` or `len` rule into a number of elements in a `usize`.
There, the optimizer knows that the value of a field fits a `usize`.

### 4. Rules: what is not a property of bytes

A struct can have rules. `#[derive(Pun)]` compiles each rule from an expression over the integer
fields of the struct to a constant postfix program (`layout::Op`). There are four rules.

- `#[pun(size = EXPR)]` gives the size in bytes of the value.
- `#[pun(len = EXPR)]` gives the number of elements of its slice.
- `#[pun(assert(EXPR))]` states anything else: bit fields, relations between fields, a minimum
  size.
- `#[pun(regions(a = EXPR, b))]` cuts the tail into consecutive regions, with an accessor for
  each.

**What an expression means.** An expression means what the same Rust expression means where
every integer (a field, a constant, a literal) is a `u128`. Otherwise it is refused at compile
time. This is its meaning, not how it is computed ("Narrowing", below). The derive enforces this
meaning in the following ways.

- It types the expression as Rust does. A value is an integer or a `bool`. `!` is bitwise on an
  integer and logical on a `bool`. An integer is not a condition.
- It reads literals itself. It accepts no float, no negative number and no signed suffix.
- It converts constants through a trait that only unsigned integer types implement.
- It makes `as uN` keep the low bits. `<<` drops the bits that it shifts out, as in Rust.
- Where Rust would panic, the rule fails. This covers an overflow, a division by zero, a shift by
  128 or more, and a read out of the value.
- What Rust does not compute is not computed. This covers the right side of a `&&` or a `||`
  that the left side decides, and the branch of an `if` that is not taken. What Rust computes
  is computed, such as a `let`, whether it is used or not.

The derive puts an `Op::Shortcut` after the left side of a `&&` or a `||`, and an `Op::Then` and
an `Op::Else` before the branches of an `if`. They skip what follows when it is not computed.
`check_ops` checks that what they skip is exactly a right side or a branch, so that the stack has
the same depth after it either way. So `protocol != 17 || be(..) == 0` holds for a packet that
is not UDP, however short, and costs nothing more for it. Nothing that is computed can be left
out of the value of an expression. Its first failure is its failure, and the evaluator stops
there. What follows is compiled knowing that nothing failed. The derive also counts what the
evaluator has room for: the depth of its stack, the variables, the operations of an expression,
and the walks nested in one another. It reports an excess at the expression, with the numbers,
and `check_ops` checks the same limits again.

**What an expression reads.** An expression can read the value beyond its fields.

- `size_of_val(self)` is the size of the value.
- `le(offset, N)` and `be(offset, N)` read an integer anywhere in the value.
- `crc32(start, end)` and `sum16(start, end)` are the frame check sequence of Ethernet and the
  checksum of IP.
- `leb128(offset)`, `quic(offset)` and `vlq(offset)` read integers of variable length, and
  `leb128_len`, `quic_len` and `vlq_len` their lengths in bytes (`Op::Varint`). As with `be`, the
  operation replaces an offset with what is there. A read never goes past the value.

So a trailer is a region, and "the 4 last bytes are the CRC of what is before" is an assertion.
An MQTT packet is `size = 1 + leb128_len(1) + leb128(1)`.

Other checksums are described by their parameters (`Op::Crc`, `Op::CrcZeros`, `Op::Checksum`).
`crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)` is any CRC of up to 64 bits. Its tables
are made in const evaluation, once for each set of parameters. There are 16 tables of 256 entries
(32 KiB), or one table (2 KiB) where `usize` is 16 bits. With two more offsets, the bytes between
them are taken as zeros, for a CRC over the header that holds it (GPT). `fletcher16`, `adler32`
and `sum` are the other checksums of the formats of the examples. Every read of the value is
bounded by it, so a range out of it fails.

A `size` or a `len` rule reads the value too, but not its size, which it defines. `from_head`
evaluates the rule over the bytes that there are, which are initialized and in bounds whatever it
reads. It then validates the value that it takes as a whole, the rule included, over the bytes of
that value only. A rule that reads beyond what it sizes fails there. A checksum that covers
several layers is an assertion of the struct that holds them all. For example, the checksum of
UDP and TCP covers the addresses of the IP header, so it is an assertion of the IPv4 packet.

An expression can name what it computes and choose between values, with
`{ let header = ..; .. }` and `if .. { .. } else { .. }`. It can also **walk**.
`walk(START, |at| COND, |at| NEXT)` is what `START` becomes when it is replaced by `NEXT` for as
long as `COND` holds of it. A walk reaches what lies at an offset that depends on what is before
it. For example, the IPv6 packet checks the checksum of UDP, TCP and ICMPv6 behind any chain of
extension headers, each of which tells what follows it and how long it is. A `size` rule can
walk to the end of a string. A walk can be in any rule, inside another walk, and several walks
can be in one expression. A walk takes at most one step for each bit of the value, plus 128
steps, the bits of a `u128` (`walk_limit`). So a walk ends, and a walk over the bits of a field
fits whatever the size of the value.

**Walks.** The state of a walk is one `u128`. A walk that carries two numbers packs them in it.
On a 64-bit target a `u128` is two registers, so the two numbers are packed at bit 64, as
`(kind << 64) + start`. Then `at >> 64` and `at & 0xffff_ffff_ffff_ffff` are each a register.
Packed as `kind + (start << 8)`, reading `start` would need a `shrd` and writing it back a merge,
on the path from one step to the next. The halves of the state are computed apart where the low
half is known not to carry ("Narrowing", below). So `start + 8 + length * 8` is one `lea`, with no
`adc` for a carry into the high half. Behind 12 extension headers, the check of the IPv6 packet
is then 210 instructions against 322 for the same check by hand (the table under "Narrowing").

**Other options of the derive.** The following options are not rules. They save code that would
otherwise be written by hand.

- `#[pun(tag(Key = Type))]` implements `Tag<Key>`. It says that, when this type is carried by
  something that tells what it carries with a `Key`, it comes with a `Type`. A frame is then
  `struct Frame<P: ?Sized + Tag<EtherType> = [u8]> { .., ether_type: P::Tag, payload: P }`.
  `Frame<Ipv4>` requires the EtherType of IPv4, and `Frame` takes any EtherType, since bytes go
  with any tag.
- `#[pun(get(name: Type = EXPR))]` generates a `const fn` getter, for bit fields. Here `EXPR` is
  plain Rust, in which an integer field is its value as a `u128` wherever it is named. Any
  `const fn` can be called.
- A region can have a type, as in `regions(options = EXPR, payload: P)`. Its accessor then
  returns a `&P`, if the bytes of the region are one. A packet whose header has a variable size
  carries a typed payload this way.
- The derive generates `from_bytes` and `from_head` for an enum whose variants each hold a
  reference, such as
  `enum AnyFrame<'a> { Ipv4(&'a Frame<Ipv4>), Arp(&'a Frame<Arp>), Other(&'a Frame) }`. They
  return the first variant that the bytes are. There is no `from_tail`, because the variants may
  be unsized. How each variant is decided once is described under "Views", below.

**How rules are proven.** Rules are checked when a value is made from something else, and tell
`from_head` how long a value is. A rule is proven only by the same rules, at the same place of
the source, in a struct of the same extent (`Layout::same_rules`, `analysis::proves_rules`). A
rule is a function of four things: the bytes of its struct, its size (`size_of_val(self)` or a
`size` rule), the number of elements of its tail (a `len` rule), and the bytes of that tail
(regions). Consider a struct of the source with the same rules in the same order
(`Rule::equals`), the same sized part and the same tail. The same tail means the same start, the
same size of an element and, for a sized struct, the same bytes and elements of the array that it
ends with. Such a struct computes the same rules over the same bytes, whatever its fields. Its
fields are related to the target as any others. So `Frame<Ipv4<Udp>>` proves the rules of
`Frame<Ipv4>`, because the payload of `Ipv4` is a typed region of its bytes and not a field of
another type. A part of a struct of a dynamic size has another size, so it does not prove the
rules of a struct of its own size.

The elements of an array or a slice share their leaves, so their rules are proven together.
Where the source has elements of the same size at the same place, each element is to the source
what the first is, and the rules of all are proven at once. Otherwise each element is checked. A
rule that reads the bytes of its struct reads its padding as well (section 5). The same tests
decide whether the rule is checked, and so whether the padding is read
(`Cursor::padding_is_read`).

**No interpreter.** Every operation of every rule is applied by an instance of a function of its
own (`validate::steps`), in which the operation is a constant. What is compiled is the arithmetic
of the expression and the loop of each walk, never a `match` on operations at run time. The
derive writes the rules of a struct that have an expression as a list at the type level
(`Pun::Rules`, `RuleAt<RULE, STEPS, NEXT>`). It writes the operations of each rule as a balanced
tree (`steps::One<I>`, `steps::Two<A, B>`), and a walk as `steps::Walk<I, COND, NEXT>`, whose two
expressions are trees of their own at any depth. The recursions down a tree are as deep as its
logarithm. Nothing that a list says is trusted. Where a list is used, it is checked against the
layout (`validate::listed`, `covers`). Every rule with an expression must be listed once and in
order, with every operation of each, and a walk where there is one and only there. The functions
name every part of the tree whatever its kind. Where a part is not a walk, they name the
expression that it is in again, which adds nothing new. `Machine::apply` never sees a walk.
Because the list is a type, a struct without rules costs nothing more to compile. `from_head`
also finds a `size` rule of the last field through the types (`Fields::Last`), and checks that it
is the one of the layout.

The machine that applies the operations (`layout::Machine`) has a stack of 16 values and 8
variables. Each is a variable of the machine, not an element of an array, reached by a `match`
whose cases are all values. The last case is the default case, because a `match` with one more
case costs instructions. Once inlined, each slot of the stack is a local variable. An expression
has at most 256 operations, and nests at most 4 walks. The derive says where an expression goes
beyond a limit, and by how much, and `check_ops` checks the limits again. In a debug build,
where nothing is inlined, each operation is a call.

What is not computed is told by an index (`Machine::skip`). The operations before it are not
applied. A `Shortcut` that decides sets the index past the right side, and a failure sets it to
the end. Where the operations are constants, so is the index on each path through the code, and
the optimizer makes jumps of it. No test is left where nothing is skipped. The alternative, a
flag on each value of the stack that tells whether it failed, makes every operation carry
failures along, and computes the side of a `||` that is not needed.

**Narrowing.** An expression means what it means on `u128`, but most of its values are known to
be much smaller. Examples are a field of two bytes, a sum of two such fields, a checksum of 16
bits, or an offset in the value. On a 64-bit target a `u128` is two registers. Its addition
carries from one into the other (`adc`), its product is three multiplications, and its division
is a call. In a loop, the optimizer does not know that the high half of a state is zero. So the
range of every value of an expression is found at compile time (`narrow.rs`). Each operation is
computed as narrowly as its range allows, with the same result and the same failure as on
`u128`.

- A value is known as two intervals, one for each half of a `u128` (`narrow::Range`). For
  example, `(kind << 64) + start` is a kind of `0..=255` in the high half and an offset in the
  low half. A field, a read of `n` bytes and a checksum are as wide as they are. A literal is
  itself, and a comparison is `0..=1`. `size_of_val(self)` is at most `isize::MAX`. A QUIC
  integer is 62 bits, and the other integers of variable length may be anything. Each operation
  makes the range of its result from those of its operands, and tells whether it may fail. A sum
  whose bounds do not overflow cannot fail. Nor can a difference whose left side is never less
  than its right side.
- A read that succeeds tells that its offsets are in the value. Examples are `be(at, 1)` and a
  checksum over `start..end`. After the read, a variable that was read at is at most
  `isize::MAX` for the rest of the expression. This does not hold after a part that may not have
  been computed, such as the right side of a `&&` or a branch of an `if`. There the variables are
  what they were before the part or after it. It does not hold in the `else` branch either, which
  starts from what was before the first branch. This is what bounds the offset of a walk,
  whatever the number of its steps. The step of IPv6 reads at `start` before it adds to it, so
  `start + 8 + length * 8` is less than `2^64` and does not carry.
- The state of a walk is a fixed point. It is what the walk starts as, joined twice with what a
  step makes of it. Then each bound that still moves goes as far as it can, in at most four more
  steps. The number of steps of a walk, at most `8 * size + 128`, bounds nothing. On 64 bits it
  is more than `2^64` for a value of `isize::MAX` bytes. The state of a walk nested two deep or
  more may be anything, so that the analysis stays a few times the length of the expression.
- How each operation is computed is a `narrow::How`. It is a constant of the instance of the
  function that applies the operation, as the operation is ("No interpreter", above). It gives
  the width of the computation, a register or 128 bits. A register is 64 bits, or 32 bits where
  `usize` is narrower. It says whether the operation is tested. An addition, a product or a
  shift that cannot overflow is not tested. It says whether an addition or a difference of 128 bits is
  computed on each half alone, which is done where the low halves cannot carry or borrow. It
  gives the width of the result, to which the result is truncated. The truncation changes
  nothing, and tells the optimizer what the value is where it could not know otherwise. This is
  the case of the state of a walk at each step, and of the variables read in its expressions.
- The plan of an expression is a constant of its type (`narrow::Planned<X>::PLAN`). Where the
  expressions of a walk start (the variables, the state) is a constant of theirs (`Ops::ENTRY`).
  Both are made once at compile time, for each expression.

The computation is never narrower than a register. On 64 bits, computing everything in 32 bits
where it fits gave as many instructions in all, with one cost. `40 + payload_length` in 32 bits
is not known to be the size that the `size` rule compared in 64 bits, so the bounds of what it
reads were tested again. For the UDP packet behind 3 headers, that was 217 instructions against
209. A remainder in 32 bits after a sum in 64 bits was worse than either, 243 instructions
against 241 for IPv4. These counts are of `callgrind` on `benches/checks.rs`, as in the table
below. The bounds of a read are one comparison of the offset with `size - n`, which does not
change in a walk.

`narrow_tests.rs` tests that the narrowed evaluation is the one on `u128`, apart from the rest of
the analysis. It tests the following.

- Every binary operation, for every pair of ranges at the limits of 32, 64 and 128 bits and of
  the halves, over values at their bounds and others, as on a target of 64 bits and of 32 bits.
  Each is compared with the operation on `u128` with every test. The result must be in the range
  that the analysis gives, and what the analysis says cannot fail must not fail.
- Random expressions, with variables, parts, reads and walks. The walks are random, move an
  offset by what they read, or carry two numbers in the halves of the state. Each expression is
  evaluated over random and boundary bytes, narrowed and not. Both must have the same value or
  fail alike.

`tests/rules_are_rust.rs` compares rules at the limits of the widths, and walks, with Rust. The
tests catch an analysis that does not test a carry, that takes no fixed point, or that does not
join what follows a part. They also catch an analysis that is wrong about the range of a
product, a remainder or a shift. Each of these was tried. The tests cannot catch a bound of
`isize::MAX` taken too small, because no value of the tests is large enough to tell.

The table gives the instructions of each check, counted with `valgrind --tool=callgrind` on
`benches/checks.rs` (`PUNS_BENCH_RUNS` and `PUNS_BENCH_SIDE`, see `CLAUDE.md`), next to the same
check written by hand.

| Check                                  | `puns` | by hand |
|----------------------------------------|-------:|--------:|
| IPv4, UDP, 64 bytes                    |    241 |     217 |
| IPv4, protocol 253                     |    120 |     106 |
| IPv6, TCP behind 3 extension headers   |     75 |     106 |
| IPv6, TCP behind 12 extension headers  |    210 |     322 |
| IPv6, UDP behind 3, 64 bytes           |    209 |     229 |
| IPv6, UDP, 1400 bytes                  |    803 |     798 |
| a frame of 6 things, UDP over IPv4     |    267 |     244 |

**Views.** `from_bytes` and `from_head` of a view enum answer what trying each variant in turn
would answer. The order of the variants is the order of preference. But each variant is checked
at most once, and only for what is not already known of it (`view.rs`).

- A variant is passed over on its first constrained scalar alone, before anything else of it is
  computed, if the bytes do not have a value that this scalar accepts (`rejects`). Examples of
  such scalars are an EtherType and a version. These comparisons of the same bytes, one for each
  variant, become one decision of the optimizer. A VLAN frame meets the EtherTypes of IPv4 and of
  ARP as in a `match`.
- A variant has a *base*. It is the first later variant whose type takes every value of the type
  of the variant, over the same bytes. The analysis decides it at compile time, as it decides
  that `pun_ref` is proven. `Frame<Ipv4>` takes every `Frame<Ipv4<Udp>>`, and `Frame` takes
  both. The base is decided first. Bytes that are not a value of the base are not a value of the
  variant either. For bytes that are, the variant is checked only for what its base does not
  prove of it, which is the marks of `try_pun_ref` from the base. For a packet, that is its
  protocol, and not its checksums again.
- What is decided is recorded in `view::Known`, a state for each variant. A base that several
  variants share is decided once, as `Frame<Ipv4>` is for UDP, TCP and ICMP. So is a base that is
  tried in its own turn after it was a base. Each variant is a function that the derive writes.
  It decides the base of the variant before the variant, by calling the function of the base,
  chosen by a `match` on a constant. Once inlined, the states are constants on each path, and no
  state remains at run time.
- For `from_head`, a base must also take the same bytes as the variant (`same_extent`). Both
  must tell their extent (a size, or a `size` or `len` rule, which a valid value meets). Or
  neither does, and both take as many elements as fit in the same way: the same alignment and
  element, and a head of the same remainder. The variant then takes the extent of its base,
  without trailing padding, which each adds on its own.

The answer is the one of trying each variant in turn. This follows from what the analysis proves
and checks. Bytes that are not a value of the base are not a value of the variant, since every
value of the variant is a value of the base. Bytes that are a value of the base meet everything
that the base requires. From such a value, `try_pun_ref` checks exactly what the variant requires
beyond it. What it leaves unchecked is implied by the requirements of the source, which the model
check checks of every pair. A base is a later variant. It is decided before its turn only as a
step of an earlier variant, and it is returned only in its turn. The error is `Error::Validity`.
A view does not tell why no variant fits.

Soundness does not rest on the code that the derive writes. The types of the variants form a
tree at the type level (`view::List`). The tree is balanced, so that what proves a type of it is
as deep as the logarithm of the number of variants. Both the layouts and the types are taken from
it, by a place in it that is a type (`view::First<view::Here>`) and is sealed. A state is written
only by `view::decide`, for the type of its place. A variant is decided from its base only where
the state of the base says that the bytes are a value of the base, of the extent recorded with it.
The derive chooses the order in which variants are decided. A variant whose base was not decided
first is not found. That is never unsound, and the model check would see it.

The bases are found by the analysis of pairs of variants, at compile time. For each variant, the
analysis tries the later variants until one takes it. It skips those that a scalar at the same
place tells apart, such as another EtherType or another protocol (`view::apart`, a walk of the
layouts). It then analyzes what is left of the variant from the base found. The base of
`from_head` is the one of `from_bytes` if it takes the same bytes, which it does where both tell
their extent. One instance of the validation serves a variant for both functions. It checks what
the two bases do not prove, which is the same marks where the base is the same, or everything
that bytes do not prove where there is no base. The function of each variant names the functions
of all the variants after it, and the `match` on a constant keeps one. So what is compiled grows
as the square of the number of variants.

The table gives the instructions of each check of the `AnyFrame` of the bench (6 variants),
counted as above. The column "a call" is `from_bytes` behind a function that is not inlined, as
the check by hand is. Where the bench calls the view directly, the optimizer inlines it, and its
checksums stay calls.

| `AnyFrame` of the bench (6 variants)   | `puns` | `puns`, a call | by hand (a call) |
|----------------------------------------|-------:|---------------:|-----------------:|
| UDP over IPv4 (the first variant)      |    283 |            273 |              244 |
| TCP over IPv4                          |    302 |            290 |              286 |
| UDP over IPv4, a wrong checksum        |    266 |            255 |              246 |
| ARP                                    |     25 |             41 |               46 |
| VLAN                                   |     25 |             41 |               45 |
| another EtherType (the last variant)   |     21 |             36 |               40 |

A wrong checksum is computed once, for `Frame<Ipv4>`, the base of the first three variants. The
UDP variant is then its protocol alone. The EtherType is one decision, a `cmp` of its 16 bits
for each value, then the variant.

### 5. What the type system enforces, and what it does not

Soundness rests on one distinction, which `Spec::valid` and `Spec::known` hold.

- A type **guarantees** what holds whatever safe code does. `bool`, enums with or without fields,
  `NonZero` and `str` guarantee their values. So do library types with a private field and a
  checked constructor, such as `u16bec<V>` and `u8in<MIN, MAX>`.
- A struct **requires** what it is made from. `#[pun(valid(..))]` on a plain field is a
  requirement, and so is a rule. Safe code can write any value to a `pub version: u8`.

A requirement is checked or proven when a value is made. It never proves what another type
guarantees. `&[Unchecked] -> &str` needs a check even if `Unchecked` is `valid(..=0x7f)`, while
`&[u8in<0, 0x7f>] -> &str` does not. For the same reason, the accessors of regions cut a region
short rather than trust the fields that give its size. Two layouts are the same
(`Layout::equals`, by which an enum proves an enum without looking into it) only if they require
the same values and guarantee the same values. `V(#[pun(valid(0..=1))] u8)` is not `V(bool)`.

**Uninitialized bytes.** A scalar may be required to be some values while nothing is known of it
as a source, not even that it is initialized (`Spec::maybe_uninit`, `known: Uninit`). Wherever a
source is read, such a scalar is padding. The places that read a source are `atom_relation`, a run
of the target without constraints, a pattern, and `segment`. Two such scalars are never equal
(`Layout::equals`). What a type described this way holds lies between what it requires and what
it is known to be. Two types can have the same of both while one may be padding where the other
may not. `MaybeUninit<T>` is padding of the size and the alignment of `T`. As a target it takes
anything. As a source it proves nothing but a target that takes anything. So a `&mut` to a
`MaybeUninit<T>` is made only from a source that may hold anything there, as the reverse
relation of `pun_mut` requires. What is written through it may be uninitialized. A `&mut T` is never made
from a `MaybeUninit<T>`, because a byte that `T` requires would be read from padding. The same
holds of a union as a source, below.

**Unions.** The language has not decided the validity of unions
([`r-undefined.validity.union`][union validity]). Some rules under discussion let a union hold
any bytes. Others require its bytes to be one of its fields. Safe code may write one field over
another. So `puns` takes what is sound under each of these rules. As a target, bytes are a union
if they are one of its fields, which requires more than some rules do and never less. As a
source, nothing is known of a union, not even that its bytes are initialized. A union is proven
by one field that the source proves. Otherwise it is checked field by field at run time
(`validate_union`). Each field is read only if the analysis found that no byte that it reads may
be padding in the source. If some field might read padding and no field is proven, the pun is
refused. Checking only the other fields would be sound, but the marks of the analysis cannot
tell the validation which fields to skip.

**No value.** `Infallible` and an enum without variants are types without values. No value of a
type that has values is one of them (`relate_at` reports the pun as never valid). A variant or a
struct that holds one has no value either (`Layout::is_inhabited`). Without this rule, a variant
of a `repr(u8)` enum whose field has no value would be proven from a variant whose field has
values, because no byte of a type of size 0 is ever compared.

**Rules read padding.** A rule that reads the bytes of its value (`le`, `be`, `crc32`, `sum16`,
and `walk` through its expressions) reads its padding as well. Such a rule may be checked when
the source does not prove it, or when it is in a variant of an enum, which is validated as a
whole. Then the padding of its struct is a byte that the target requires to be initialized.

A type is known by what the type system enforces of its bytes, and by nothing else. This has two
consequences, which are by design.

- `PhantomData<T>` is a scalar of no byte, and implements `Scalar` for any `T`. Two types that
  differ only by phantom parameters or by lifetimes have the same layout. Each converts to the
  other freely, by value and by `&mut`. A type whose parameters are a brand must not derive
  `Pun`, because `pun` would turn one brand into another. A brand is an invariant lifetime that
  ties a value to a scope, or a marker that says what a value was checked against.
- Deriving `Pun` makes every field as accessible as if it were public. A value of the struct can
  be made from anything whose bytes are valid for its fields and its rules. `pun` and `try_pun`
  check what the derive is told, and nothing that a constructor checks. The value can be read as
  anything that its bytes are, and written through a `&mut` to another type. A type whose private
  fields keep an invariant that the derive is not told of must not derive `Pun`.

### 6. What `#[derive(Pun)]` trusts

The derive writes `unsafe impl`s in the crate that uses it. There, safe code can make every path
lead elsewhere. `#[pun(crate = ..)]` names any module, `#[pun(crate)]` the root of the crate, and
`extern crate other as core;` makes `::core` another crate. So what the layout is made of does
not depend on any path, in the following ways.

- The functions that the generated code calls are associated functions of the types that the
  implemented traits name (`Layout`, `Spec`, `Field`). Or they are associated functions of types
  that `Layout::PIN` fixes with `fn(T)` types (`Rule`, `Op`, `Field`, `Enum`, `Variant`), for
  which no path can stand.
- The offsets of the fields do not come from `offset_of!`, which is a macro named by a path.
  With `repr(C)`, they are what `repr(C)` specifies, computed from the sizes and the alignments
  of the fields. With `repr(transparent)`, the field of a size is at 0. Otherwise the struct is
  `Sized`, and the offsets are found by projecting a pointer to an uninitialized value
  (`&raw const (*p).field`). This costs nothing in const evaluation, whatever the size.
- The variants of an enum with fields and a primitive or `C` representation are laid out as
  [RFC 2195] specifies, from the sizes and the alignments of their fields. Without `repr`,
  `Layout::discover` finds them from samples made by `Layout::sample`. Both are associated
  functions, as every other one. The offsets in those samples are found by code that matches on
  them, with `byte_offset_from`, a method of a pointer that no path names. A fake `MaybeUninit`
  or `Desc` named by a path does not have the type that these functions take or return, and does
  not compile.
- `Path` is sealed. The leaf at which a list of fields says that a field is, is checked against
  the layout.
- Names that start with `__Pun` are reserved. In the type of a field, `Self` is replaced by the
  type that it means. The generated code writes these types where both names mean something
  else.

The rest of `::core` that the derive names (`Option`, `PhantomData`, `usize`, and so on) is
checked by the types of the traits that it implements. The derive still trusts the compiler for
what it parses, such as `repr`. It also trusts that the type of a field is the same wherever it
writes it. A procedural macro in the type of a field that expands to a different type at each
expansion would break that.

### 7. Pointers to unsized types (`ptr.rs`)

Generic `const fn` code splits and builds wide pointers by reinterpreting a `*const T` as a
`*const [u8]`. Rust does not document that representation. So it is **verified at compile time
for each unsized type**, against a pointer made with a genuine `as` cast (`Pun::PROBE`,
`ptr::verify`). If the representation ever changes, the crate stops compiling instead of
misbehaving.

CI runs the tests under [Miri] with `-Zmiri-strict-provenance`, with tree borrows on x86-64, and
with stacked borrows on `i686-unknown-linux-gnu` and `s390x-unknown-linux-gnu`. The i686 target
has a 32-bit `usize` and a `u64` aligned to 4. The s390x target is big-endian. The Miri selection
of the model check (below) runs there too, with tree borrows.

`-Zmiri-symbolic-alignment-check` reports a misaligned reference where `try_pun_ref` checked the
address of bytes that happened to be aligned. The check reads the address as an integer
(`ptr::is_aligned`), which the symbolic alignment of Miri cannot follow. The stable
`align_offset` is not a `const fn`, so it cannot replace that check.

### 8. Self-relative pointers (`layout::Pointer`, `relative.rs`, `deep.rs`)

A value may hold a *self-relative pointer*: an offset from its own address to a target in the
same buffer, as the `Near<T, O>` of the crate [`nearest`] is. The core of `puns` knows this kind
of pointer, and nothing of any crate that has such pointers. A crate declares its pointer types
(below). The description of a pointer (`layout::Pointer`) holds what the proofs need, and nothing
else.

- Its **scheme** is a name that the declaring crate chooses. It stands for the way that a pointer
  and the bytes of its buffer tell where the target is and how long it is, such as a length next
  to the target. `puns` does not know the way, and does not need to. A pointer is read only as a
  pointer of the same scheme, which finds the same target from the same bytes.
- Its **offset** is an integer of 1 to 16 bytes, the size of the pointer type, in a byte order.
- What it **aims** at (`Aim`) is one of three things. It is one value of the target type at an
  address aligned for it, or a **sequence** of values of the target type, `size_of` apart, or
  **text**, UTF-8. The first element of a sequence is at an address that is a multiple of an
  alignment that the declaring crate guarantees. That alignment is a number of the description,
  at least the alignment of the type. `nearest` declares the larger of that alignment and the
  width of the offset. The description keeps the size and the alignment of the target type, or
  of an element.
- Whether it is **nullable** (`Pointer::or_null`). The offset whose bytes are all zero is then no
  pointer. `Option` of a pointer is such a pointer (section 10).

What a pointer aims at is *not* in the layout. The layout of `struct Node { next: Opt<Node> }`
would have no end. Const evaluation has no identity of a type by which to stop where it has
already been. `TypeId` cannot be compared in `const`, nor can pointers, and a static reached
through an associated constant is a cycle. So what a pointer aims at is a type, `Pun::Pointee`.
It is the element for a sequence, and `()` for text. The layout keeps the size and the alignment
of the target, which the compiler gives without looking into the target.

**How a crate declares a pointer.** The crate writes an `unsafe impl SelfRelative for P`
(`relative.rs`). It says what the pointer is, with `const POINTER: Pointer` made by
`Pointer::one::<T>`, `Pointer::sequence::<T>` or `Pointer::text`, and `or_null`. It says what the
pointer aims at, with `type Target: Pun`. Then `self_relative!(impl[..] P<..>)` writes the
implementation of `Pun` from it. `Pointee` is the `Target`, and the layout is
`Layout::pointer::<P, Target>(POINTER)`. `Layout::pointer` asserts that `P` has the size of the
offset, and that `POINTER` was made for `Target`. The trait carries the promise, in its
`# Safety` section. The macro is safe to call, and writes nothing that the trait does not say.

`self_relative!` is not a blanket implementation of `Pun` for every `SelfRelative` type. That
implementation would overlap the one for every `Scalar`, because coherence cannot tell that no
type is both. Also, the items of `Pun` belong to `puns`, which may change them. The macro is
expanded with the version of `puns` that defines them. The doc test of `SelfRelative` declares a
pointer as such a crate would, since doc tests are outside the crate. So do the tests
(`tests/support/relative.rs`), the model check and `examples/codegen.rs`.

**What the declaring crate promises.** The contract is the `# Safety` section of `SelfRelative`,
items 1 to 7. In short:

1. **The type is its offset.** It is an integer of the width and the byte order of `POINTER`, and
   nothing else. Every byte is initialized, and none is in an `UnsafeCell`, since it is read as
   plain data. An offset that has no zero (a `NonZero`) is detected by `Layout::pointer`
   (`Pointer::nonzero`), because `Option` of it has its size and `None` is all zero bytes. No
   other restriction of the values of the offset is supported. A niche other than zero is
   refused where `puns` finds one.
2. **The scheme is the way to the target.** Where a pointer aims, and how many elements or bytes
   it aims at, depend only on the scheme, the width and the byte order of the offset, the kind of
   aim, the address of the pointer, the bytes of its offset, and bytes of its buffer. They never
   depend on the type that holds the pointer, on its `Target`, on its alignment, or on whether it
   is nullable. The declaring crate owns the name of its scheme, and changes the way only
   together with the name.
3. **What it aims at is there, and valid**, in the same allocation, for as long as a shared
   reference to the pointer lives. It is a valid `Target` aligned for it, or that many valid
   `Target`s with the first at a multiple of the declared alignment, or that many bytes of UTF-8.
4. **What it aims at is its own.** While a shared reference to the pointer lives, nothing writes
   to what it aims at, nor makes a `&mut` to it.
5. **Null.** With `or_null`, an offset whose bytes are all zero is no pointer, and is never
   followed. Any other value is a pointer, as a pointer of the same scheme that is not nullable
   would be.
6. **The target is the type.** `Target` is the type aimed at (an element, or `()` for text), and
   `POINTER` is made for it. `self_relative!` asserts this.
7. **The declaring crate relies on items 1 to 6, and on nothing more**, when it follows a
   pointer. In return, `puns` makes a value that holds a pointer of the type only through
   `view_ref` and `try_view_ref`. It makes it at the address of a pointer of the same scheme,
   offset and kind of aim, which is nullable only where the type is, or is checked not to be
   null. The target of that pointer is proven to be a valid target of the type. `puns` reads the
   bytes of a pointer, and never writes them.

**What a pointer guarantees is not a property of its bytes.** Its target is where it says,
valid, and its own. Three rules follow, which every function keeps.

1. **No bytes prove a pointer.** A pointer of the target is proven only by a pointer of the
   source at the same place whose target it can read (`Pointer::reads_as`). That pointer must
   have the same scheme, the same offset (width and byte order), and the same kind of aim. Its
   target must be no larger and need no more alignment. For one value, that is its size and
   alignment. For a sequence, the elements must have the same size, and the declared alignment
   of the target must not be greater than the one of the source. For text, nothing more is
   needed. The pointer of the source must not be nullable where the one of the target is not.
   Its type must not have an offset of zero where the type of the target has none (a
   `NonZero`), unless that is checked. `try_view_ref` checks that it is not zero, and `view_ref`
   refuses it. Anything else sets `Relation::pointer`, which no function accepts and no check
   makes up for. An enum that holds pointers is proven by an enum, variant by variant, or not at
   all.
2. **Only what proves what pointers aim at makes a value that holds a pointer.** `view_ref` and
   `try_view_ref` prove it (below). Every other function refuses a target that holds a pointer
   (`Plan::gate`). A crate that has pointers checks bytes from outside with a walk of its own, as
   `nearest` does. `puns` does not load a buffer.
3. **No `&mut` to a pointer, nor next to one.** `mutable` and `mutable_part` refuse a source or a
   target that holds a pointer, wherever it is. Through a `&mut`, safe code could write over a
   pointer, swap two pointers, or move one to where it aims at something else.

As a *source*, a pointer is initialized bytes of which nothing more is known. Any function that
reads (`as_bytes`, `head_ref`, and so on) reads a value that holds pointers as plain data.

The list gives every public function, and why it is safe with a pointer in its target or in its
source.

- `pun`, `head`, `tail`, `try_pun` and `zeroed` take and make `Copy` values. The target is gated
  (rule 2), and `zeroed` asserts it. A source that holds a pointer is read as bytes.
- `pun_ref`, `head_ref`, `tail_ref` and `try_pun_ref` refuse a target that holds a pointer (rule
  2). They read the bytes of a source that does, and make nothing of them.
- `pun_mut`, `head_mut`, `tail_mut` and `try_pun_mut` refuse a pointer on either side (rule 3),
  after the target is gated (rule 2).
- `as_bytes` is `pun_ref::<S, [u8]>`. It reads the bytes of a pointer.
- `from_bytes`, `from_head` and `from_tail` are `try_pun_ref` from bytes (`Plan::TRY_REF`,
  `FROM_TAIL`). They refuse the target (rule 2).
- `from_bytes_mut`, `from_head_mut` and `from_tail_mut` use the same plans, then `MUTABLE`. They
  refuse it (rules 2 and 3).
- `from_bytes` and `from_head` of a view enum, which the derive writes, call `view::decide` for
  the type of each variant. It requires of that type what `from_bytes` does (`Plan::TRY_REF`), so
  it refuses a pointer (rule 2).
- The getters of `#[pun(get(..))]` and `int` read an integer field of a `&self`. A pointer is not
  one (`Layout::as_integer`), so it fails to compile.
- The accessors of regions and `region` need `pun_ref::<S, [u8]>`, and return bytes of the tail.
  They read.
- The accessors of typed regions (`regions(name: P)`) call `from_bytes::<P>` on the bytes of the
  region. They refuse a pointer (rule 2).
- The rules (`size`, `len`, `assert`, `regions`) read unsigned integer fields, which a pointer is
  not (an error of the generated code), or bytes (`le`, `be`, `crc32`, and so on), which they
  read.
- `Tag`, `num::*`, `crc32` and `sum16` involve no pointer.
- `view_ref` and `try_view_ref` require each pointer of the target to be a pointer of the source
  (rule 1), prove what it aims at (`deep`, below), and return a `&`.
- `SelfRelative` and `self_relative!` are a declaration, whose promise is the `unsafe impl`. The
  macro writes `Pun` from it, and asserts what can be asserted (invariant P1, below).
- `layout::*` holds values that describe layouts, for the code that the derive and
  `self_relative!` write. They make no value of a type. A layout stands for a type only through
  `Pun`, an `unsafe trait`. `Pointer` is public, and made only by its constructors, since its
  fields are private.
- `hook::*`, behind the test-only feature `verdict`, runs the same plans and answers instead of
  enforcing. Its `view_ref` follows pointers a bounded number of times. It is for the model check,
  whose types that bound covers.

**`view_ref` proves what pointers aim at with instances of functions (`deep.rs`).**
`view_ref::<S, D>` runs the plan of `pun_ref` with pointers allowed. It then proves, for every
pointer of `D`, that the `X` that its source pointer aims at can be read as the `Y` that it aims
at itself. `Y` must be the first bytes of `X`, every `X` must be valid for them, and nothing may
be left to check, since nothing can be checked behind a pointer. The pointers of `Y` are then
pointers of `X`, and so on. For a recursive type, this never ends as an analysis. As instances of
functions, it ends.

- `target::<X, Y>` holds, in a `const` block, the proof of one pair up to the next pointers
  (`behind`, the analysis of `X` as `Y`, and `local`, below).
- `pair::<A, B>` *names* `target::<A::Pointee, B::Pointee>` for the pointers that `A` and `B` hold
  at the same place, through their fields, variants and elements. A named function is an
  instance, which exists once however often it is named. A recursive type closes a cycle of
  instances, and the proof is an induction over the pairs of types that can be met.

`view_ref` calls `proof`, and through it `pair` over everything up to the first pointers. These
are `const` blocks of code that runs, which the language evaluates
([`r-expr.block.const.evaluation`][const evaluated]). Behind a pointer, `target` is named and
never called, since calling it would not end. Its `const` block is evaluated because **the
compiler makes every function that the code it compiles names**. It does so whether that code
runs or not, and whatever the optimizations. It has to build the function pointer, and `rustc`
collects what a function mentions before it optimizes, so that errors of const evaluation do not
depend on the optimization level. This is a behavior of the compiler. The Reference says that a
`const` block that is not executed may or may not be evaluated
([`r-expr.block.const.not-executed`][const not executed]). The compile-fail test
`view_behind_pointers` (a `u8` read as a `bool` behind the pointers of a recursive type) holds
the compiler to this behavior in a debug build. `model.yml` builds the same program optimized,
with LTO, every night, and it must be refused there too. That check is to be run when
`rust-toolchain` changes. The program declares its own pointer and needs no other crate. For the
same reason, `view_ref` is not a `const fn`. A call in a constant would make no instance. An
interpreter that evaluates only the constants of the functions that it runs, as Miri does, would
not refuse such a view. The refusal is one of a build.

The compiler also makes the instances that a branch names when a constant rules the branch out.
So `pair` cannot choose what it names. It pairs by position: fields with fields and variants with
variants, in the order of their declarations, and elements with elements. This names more pairs
than matter. Where the source has nothing, its side is `()`. `target` has nothing to prove of a
type without size, since no pointer aims at one (`Pointer::one`, `Pointer::sequence`), and the
target type of text is `()`. `local`, which is `const` code, decides exactly which pairs matter.
Every pointer of the target must be, in the source, a pointer at the same offset that `pair`
pairs it with, or the build fails. So what `local` accepts, `pair` names, by the same walk of the
same lists. A pair that is named without mattering has one side without pointers.

Those lists (`Fields`, `Variants`, `Elem`, `Pointee`) are types, written next to the layout that
the analysis reads. On its way, `local` checks that they are what the layouts hold (`listed`):
the same fields in the order of their declarations, the same variants, the same elements, and a
target of the size and the alignment that the pointer says. So an implementation of `Pun` whose
lists and layout differ cannot make a pointer be missed, or taken for another (compile-fail test
`lists_not_the_layout`). Hence the limits, which are refusals.

- A field that holds a pointer keeps its position among the fields of its struct.
- A variant that holds a pointer keeps its position among the variants of its enum. New variants
  go last. A variant found by its tag at another position is refused (compile-fail test
  `view_variant_moved`).
- A struct that only holds a pointer is not that pointer.

`try_view_ref` checks what is not proven of the value itself (`validate`, with the marks of the
analysis). That may be a variant that the target does not have, a restricted scalar, or a
nullable pointer read as one that is not nullable, whose bytes must not be all zero.
`try_view_ref` requires of what the value points to what `view_ref` requires.

**Invariants.** Each invariant is kept where it is stated.

- **P1.** A `Kind::Pointer` layout is made only by `Layout::pointer`, which `self_relative!` calls
  for a `SelfRelative` type (an `unsafe trait`). The type is the offset and nothing else, which
  its size asserts. It aims at its `Target`, for which the description was made, which the size
  and the alignment assert. That target is a type with a size (`Pointer::one`,
  `Pointer::sequence`), or `()` for text.
- **P2.** `Relation::pointer` is set for every pointer of a target that a pointer of the source
  does not prove. The two places that give up on proving set it (`relate_range`,
  `relate_enums`). Every plan refuses it, fallible or not (`check`).
- **P3.** Every plan but `VIEW` and `TRY_VIEW` refuses a target that holds a pointer
  (`Plan::gate`). `Plan::check` and `Plan::check_tail` call it, and every plan goes through one
  of them.
- **P4.** `mutable` and `mutable_part` are false with a pointer on either side.
- **P5.** `view_ref` and `try_view_ref` are the only functions with the plans that P3 exempts.
  Both call `deep::proof` for the same pair of types. Neither is a `const fn`.
- **P6.** What `deep::local` accepts, `deep::pair` names. Both walk the fields, the variants and
  the elements of the two types by position. `local` refuses whatever is not at the position
  where `pair` looks for it, and a type whose lists are not what its layout holds (`listed`).
- **P7.** `Pointer::reads_as` relates only pointers of the same scheme, width, byte order and kind
  of aim, with the numbers of their descriptions (sizes, alignments, the declared alignment of a
  sequence). It says when nullability, or an offset of zero read as a `NonZero`, is to be checked.
  `Pointer::same`, by which two layouts are equal, compares every field of the description.
- **P8.** `puns` relies on items 1 to 7 of the contract of `SelfRelative`, and on nothing more.
  It relies on item 2 when it reads a pointer as another of the same scheme, on items 3 and 4
  for what `deep` proves of the targets, and on item 5 for the check of a nullable pointer by
  `try_view_ref`.

### 9. Who builds on this

[`nearest`] declares its pointers with `SelfRelative`, and depends on `puns`, not the reverse. A
region of `nearest` is a value and everything it points to, in one buffer, with `Near<T, O>`
pointers. The layout and the validity of bytes are the general thing. A region is one structure
made of them, with a byte format of its own, which `puns` does not state again. A scheme is a
name for that format. One validator of values, the one of `puns`, serves both crates. It checks
what a field may be, enums, rules, and what `try_view_ref` checks. What belongs to a region
(where its targets are, its blocks, its walk of an image) belongs to `nearest`.

**The validator of a value in a buffer (`visit.rs`).** `validate_in::<T, V>(bytes, at, visit)`
validates the `T` at the position `at` of `bytes`, except what its pointers aim at. It hands each
pointer of the value to the visitor, `V: Visit`, with its type and its position, as
`visit.pointer::<P>(at)`. The visitor belongs to the crate that has the pointers. It reads the
description of `P` (`Layout::as_pointer`, the getters of `Pointer`). It finds the target as the
scheme says, places it as its buffer requires, and validates it in turn or queues it. It
validates a target with `validate_in::<Pointee<P>, _>`, or with `validate_elems_in` for a
sequence. `validate_elems_in` validates `count` values one after the other. Both functions are
safe for any bytes and any position. They read nothing outside `bytes` (`get`, checked
arithmetic, then reads at positions in bounds), do not panic, and recurse over types only, never
through pointers.

The documentation of `validate_in` states its contract, which `unsafe` code of another crate
relies on. After `Ok(())`, the following holds.

1. `at + size_of::<T>()` is at most `bytes.len()`. For `validate_elems_in`, it is
   `at + count * size_of::<T>()`, which does not overflow.
2. Those bytes are a valid `T` as its layout says, provided that every pointer handed over is a
   valid pointer of its type (items 1 to 7 of `SelfRelative`). Every scalar is one of its values,
   `valid(..)` included. Every enum has the tag, or the niche, of a variant whose fields are
   valid. Every union is one of its fields. Every rule of every struct holds, evaluated on the
   bytes of that struct.
3. Every pointer of the value was handed over once, where it is, and the visitor returned `Ok`.
   The bytes of a pointer may be all zero. They are then no pointer if it is nullable, and are not
   handed over. They are not a value if its type has no value of zeros (its offset is a
   `NonZero`, `Pointer::nonzero`). Otherwise they are a pointer of offset zero, handed over, and
   its scheme says what it aims at.

The contract says nothing of what the pointers aim at: whether it is in the bytes, or shared. The
scheme is the business of the visitor. Nor does it say anything of addresses, only of positions.
A `&T` at `bytes.as_ptr() + at` must be aligned for `T`, and that is the duty of the caller: a
buffer at an aligned address, and `at` a multiple of the alignment. The bytes are initialized,
since they are a `&[u8]`, so `MaybeUninit` and padding hold what they hold. A `MaybeUninit` of a
pointer is nothing, and nothing of it is handed over. A union that holds a pointer does not
compile there, because which field it holds cannot be told, and so neither can the pointer. `T`
is `Sized`, since a buffer does not hold by value a value whose size its bytes tell. The error
`Invalid { at }` is where the bytes stop being a value. That is the scalar, the tag, the union,
the struct whose rule fails, or the value that is not all there. `Invalid` is `#[non_exhaustive]`,
so that it may later say why. A visitor may also end the walk with an error of its own
(`Visit::Error`).

What holds no pointer is validated by `validate`, the code of `try_pun_ref`, at the same speed.
Its answer is the verdict. Only where it fails is the value walked again, by the walk by
position (`value`, `fields`, `enumeration`), to tell where. If that walk finds nothing, the error
is at the value itself, which is less precise and never wrong. The same holds of the elements of
`validate_elems_in`. The test `the_verdict_is_the_one_of_try_pun_ref` holds the two to the same
answers. The walk goes by the lists of types that the derive writes (`Fields`, `Variants`,
`Elem`, `Pointee`). It checks them against the layout for every type that it goes through
(`listed`), as `deep::listed` does for `view_ref`. It checks every field by its index of
declaration, offset, layout and type, every variant by its tag, its tags of a niche and its
fields, the element, and the target of a pointer. A list that left a field out would leave it
unchecked (compile-fail test `validate_lists_not_the_layout`). A struct that holds pointers is
walked field by field, so that its pointers are handed over in order with the checks of the rest.
A visitor whose queue is full stops the walk where it is.

**The walk of the pointers of a valid value.** `pointers_in::<T, V>(base, at, visit)`, and
`pointers_elems_in` for `count` values, hand the visitor the pointers of a `T` known to be valid.
They make the same calls that `validate_in` makes on the same bytes, in the same order, and check
nothing. They are for a crate that copies a value with what it aims at, or that moves a value in
its buffer and re-aims its pointers. Validating again there would check every scalar for nothing.
`pointers_in` is `unsafe`. The bytes at `base + at` must hold a `T` as its types say, at any
alignment. Every scalar must be initialized and valid for its type. The tag of every enum, or its
niche, must be initialized and tell a variant, whose bytes are so in turn. Padding, and what the
variant does not hold, may be uninitialized, since neither is read. The rules of `T` need not
hold, nor the `valid(..)` of its fields, which `validate_in` checks. Safe code can break them in
a value that it makes, such as `Span { lo: 2, hi: 1 }` of `assert(lo <= hi)`, and `nearest` walks
and copies such a value all the same. They cannot matter here. The walk goes by types. It reads
the tags of enums, niches included, as `validate_in` does. `variants` and `follow_variants`
choose the variant by the same test. It also reads the offsets of nullable pointers, and nothing else,
and evaluates no rule. A restricted field is a scalar, which holds no pointer. The walk goes by
the same lists of types (`listed`), and refuses the same unions. A type without pointers is not
walked at all.

**The copy of a valid value.** `copy_in::<T>(src, dst)` copies the bytes that a valid `T` holds
initialized, except those of its pointers. It copies each scalar (a scalar that may be
uninitialized, as in `MaybeUninit`, is not one), the tag of each enum, and the fields of the
variant that it tells. A nullable pointer whose bytes are all zero is no pointer, and its zeros
are copied. The other pointers are the ones that `pointers_in` hands over, in the same order.
Their bytes are left to the caller, who writes their offsets, re-aimed for where the copy is or
aimed at copies of their targets. Copying them first would write them twice. The bytes of a valid
`T` that may be uninitialized are never read from `src` nor written to `dst`, whose bytes there
keep what they held. Those are padding, the bytes of the variants that the value is not, and
fields that may be uninitialized. `copy_in` is for a crate that writes values into a buffer of
initialized bytes, such as a region that hands out its bytes as a `&[u8]`. There, a typed copy
would leave padding uninitialized, and a copy of the bytes would copy the padding of the source,
uninitialized on the stack. A part without such bytes and without pointers (`is_plain`) is copied
whole, at once. The rest is walked by the same lists of types as `validate_in` (`listed`),
choosing the variant by the same test, so that it compiles to a few moves for each type. A union
does not compile there (compile-fail test `copy_in_union`), because which of its fields it holds,
and so which bytes are initialized, cannot be told. The source of `copy_in` is what `pointers_in`
reads, as its types say, whatever its rules say. No rule is read, and a restricted field is
copied as the scalar of its type.

The model check holds these functions to the oracle of pointers (below). Its visitor follows the
pointers of the tests through their buffer.

- `validate_in` accepts exactly what the oracle accepts, on the corpus and on every copy with one
  byte changed, padding and gaps included (`check_validate`).
- On every image that `validate_in` accepts, `pointers_in`, following the pointers in turn, hands
  over the same pointers, at the same positions and with the same types, in the same order
  (`check_pointers_in`).
- `copy_in` writes the bytes that the oracle says the value holds, and only those, into bytes of
  two fills (`check_copy_in`). Those bytes are its scalars, the tag and the fields of its
  variant, and a null pointer, but not the bytes of the other pointers.

Each check runs on a sample on a push, and on every type and every mask of one bit with the
`full` feature. The model checks `pointers_in` and `copy_in` only on images that `validate_in`
accepts, so on none whose rules do not hold. That they need not hold rests on the argument
above, that no rule is read, and on a test. `tests/validate_in.rs` gives each kind of type its
answer and the positions of errors. It runs random bytes, positions and counts, some of which
overflow, under Miri on every target of CI. On what `validate_in` accepts there, it checks that
`pointers_in` and `pointers_elems_in` hand over the same pointers, at any alignment, with the
bytes that they need not read left uninitialized. It checks that `copy_in` of a value on the
stack, with its padding and its other variant uninitialized, into memory that is then read whole
as a `&[u8]`, copies its value and nothing else. It checks that `copy_in` of random accepted bytes
gives a value that `validate_in` accepts again. Last, a value with pointers breaks its `assert`
and the `valid(..)` of a field, in a buffer, and `validate_in` refuses it. The test checks that
`pointers_in` hands over its pointers, and that `copy_in` copies its scalars and its null
pointer, as for a value that meets them (`rules_need_not_hold_for_pointers_in_and_copy_in`).

### 10. Enums without `repr` (`discover.rs`)

Rust does not specify the layout of an enum with fields and no `repr`. Stable Rust names neither
where its tag is nor its values. `offset_of!` on variants, [`mem::variant_count`] and
[`discriminant_value`] are unstable, and nothing tells a niche. But const evaluation runs code on
values of the enum. `#[derive(Pun)]` makes a *sample* of each variant (`Layout::sample`), with
every scalar of each field at the least of its values and the padding uninitialized. It finds the
offsets of the fields by matching on the sample. `Layout::discover` finds the rest by experiments
on copies of the samples, which read the variant with [`mem::discriminant`].

- **No variant has values.** The enum has no tag and no value.
- **One variant has values**, and either no other variant is laid out, or the fields of that
  variant start at 0. `rustc` lays out a variant without values only if one of its fields has a
  size. Fields at 0 mean a niche in its own fields, or none. The enum is then a struct of the
  fields of that variant. Its gaps are filled with zeros, then with ones, and it must stay that
  variant.
- **A direct tag** is used when the fields of no variant start at 0. `rustc` puts the tag before
  the fields, widened to all the bytes before the first field of any variant. For example,
  `enum { A(u16), B(u32) }` has a tag of 2 bytes, in which the high byte of each value is 0.
  Those bytes are *read* in every sample. A byte that is not part of the tag would be padding,
  uninitialized, and reading it fails the build (R2, R4). So the tag is exact on either byte
  order. No claim is made of a byte that may be padding, and the tag is an integer of native byte
  order wherever its low byte is.
- **A niche** is used when some field starts at 0. The candidates are the scalars of the fields
  of each variant that the type restricts, and that the fields of no other variant cover. The
  scalars that the type restricts are `bool`, `char`, `NonZero`, enums without fields, and tags
  of enums, but not a `valid(..)`, which safe code can break. They are tried for the largest
  variant first. The bytes of the scalar are moved from the sample of each variant to the sample
  of a variant that does not hold them, which must become that variant. A wrong scalar changes
  nothing there, and is dropped without failing. Then the tag of each other variant is read
  there. It is initialized, so it is the tag (R4). The variant that holds the niche has the values
  of that scalar, which the tags of the other variants must not be.

In both cases, the gaps of each variant that has one tag, outside the tag, are filled with zeros
and then with ones. This must leave it that variant, since a byte of a tag outside would give two
values to one variant. All the gaps of the variant that holds a niche are filled the same way.
Its scalar must tell it alone. A layout in which another place takes part, such as an encoding
in two levels, is refused. The tags must differ. The header of `discover.rs` says which fact each
experiment holds. What is found is checked against the size and the alignment of the enum
(`Layout::enumeration`). An experiment that goes wrong either refuses the enum with a message
("no tag found", "unsupported layout", and so on), or fails the build with an error of const
evaluation. It never finds a wrong layout. Every experiment copies a sample and reads a variant.
There are about four experiments for each variant, and none for each byte of a field.

What this rests on, of the compiler, and what holds each fact:

- **R0.** The layout of a type is the same in const evaluation as at run time, for a given
  target. Const evaluation computes it with the same code, for that target. This is a property of
  how `rustc` is built.
- **R1.** The variant of a value is told by one scalar at a fixed place. Either it is a direct tag
  before the fields of every variant, with one value for each variant. Or it is a niche in a
  scalar of the fields of one variant, which no field of another variant covers. The values of
  the niche that are valid for its field are that variant, and each other value is one other
  variant, or no value. This is how `rustc` lays out enums, since 2017 for one variant with
  fields and since 1.64 for several. It is not a promise of the language. The experiments verify
  what they rely on of it. The tag decides, and nothing outside the tag does. A layout of another
  shape is refused.
- **R2.** Const evaluation fails on reading an uninitialized byte, and on reading the variant of a
  value whose tag is not the tag of a variant that has values. This is a behavior of the
  compiler, which the Reference does not promise for undefined behavior in `const`. The
  compile-fail tests `discover_reads_padding` and `discover_invalid_tag` hold the compiler to it.
- **R3.** `mem::Discriminant<E>` of an enum without `repr` holds its discriminant, which is the
  index of the variant, as an `isize`. No explicit discriminant is allowed on such an enum with
  fields. The size of `Discriminant<E>` is asserted, and every sample is checked to read as its
  own index.
- **R4.** A value made in const evaluation (`E::A(x)`) has its padding uninitialized. Making a
  variant writes its fields and its tag, and nothing else. So a byte of a fresh sample outside its
  fields that can be read is a byte of the tag. The test `discover_reads_padding` holds this.

The derive also relies on one fact of the library. The valid values of a type, as `Pun::LAYOUT`
gives them, are valid for `rustc`, as `Pun` promises. So the values that `rustc` gives to the
other variants of a niche, which are invalid for the scalar that holds the niche, are not values
of that scalar in its layout.

Such an enum is then what an enum of a specified layout is. It is proven variant by variant, and
checked by its tag and its variant, with the same precision. A variant whose fields have no
value is in the layout, with no tag. The lists of fields and variants are the declared ones. A
field of a variant without values is not in the layout, and its entry in the list says so
(`FIELD` is `None`). No field may hold a self-relative pointer, because a sample of a pointer
cannot be made from nothing.

**`Option<T>` and `Result<T, E>`** (`std_enums.rs`) are found the same way, for any `T` and `E`
whose layout is known. `Option<NonZero<u32>>` is a niche at 0, so every `u32` is one, proven both
ways. `Option<bool>` is the niche 2. `Option<Option<bool>>` has two niches, and `rustc` puts
`None` at 255 today, below the valid values rather than after them. This is found, not assumed.
`Option<u16>` has a tag of two bytes. `Result<u32, Infallible>` is a struct of its `u32`.

**`Option` of a self-relative pointer** is the same pointer, made nullable. The discovery does
not look into a pointer. So `Pun` has a hidden associated constant, `OPTION`, the layout of
`Option<Self>` if `Self` gives it. It is `None` by default, and the layout is then found by
discovery. `self_relative!` gives `Layout::nullable::<Self, Target>(POINTER)`. That is
`POINTER.or_null()`, after a check made where `Option` of the pointer is used. The check is that
`Option<Self>` has the size of `Self`, and that `None` is all zero bytes, read in const
evaluation. Then `rustc` has put `None` in a niche of the bytes of the pointer, which are all of
its offset and initialized (item 1), and that niche is zeros. `Some(p)` is the bytes of `p`
(R1), which are not all zero. So item 5 holds of `Option<Self>`. An offset that has no niche at
zero, such as a byte array, makes `Option` of the pointer larger, with a tag of its own. That
fails to build where it is used, with a message (compile-fail test
`option_of_pointer_without_niche`). The generic `impl Pun for Option<T>` takes `T::OPTION` when it
is `Some`. Nothing is then discovered (the `Desc` is empty), and its lists of variants end at
once. `deep.rs` does not look into a pointer, except for what it aims at, its `Pointee`, which is
`T::Pointee`. The rules of section 8 apply to it as to any pointer. A crate that declares a
pointer gets this from the macro alone. The tests show it with an offset whose zero is a niche
(`Nz32le`). They read `Option<One<T, Nz32le>>` as `Opt<T, i32le>` and back, through values of a
buffer.

### 11. What `usize` is assumed to be

`puns` builds for every width of `usize` that Rust supports, 16 bits included. CI builds it for
`msp430-none-elf` with `-Zbuild-std=core`. Miri cannot run a 16-bit target, so what holds there
is asserted. The table lists what the code assumes of `usize`, and what holds each assumption.

| Assumption (where) | Held by |
|---|---|
| A value is at most `isize::MAX` bytes, so a size plus a little does not overflow (`walk_limit`) | Rust, and a `const` assertion next to `walk_limit` |
| An offset, a length or a range read by a rule is compared as a `u128` before it becomes a `usize` (`Machine::apply`, for `At`, `Varint` and the checksums) | the code: on every target, the comparison is with the size of the value |
| A position plus a size in the bytes given to `validate_in` and `validate_elems_in` does not overflow | `checked_mul` and `get`: the arithmetic of the bounds is checked, and so is any position in them |
| The tables of a `Crc` fit in a value (`Crc::new`, `Crc::update`) | `CRC_TABLES`, 16 tables (32 KiB), or 1 table (2 KiB) where `usize` is 16 bits, with a `const` assertion. The CRCs are the same, and the tests of `Crc` compare them with references |
| `sum16` folds before a carry could leave its `u64` (`sum16_inline`) | `SUM16_FOLD`, 2^20 blocks, or every block where `usize` cannot count that far, with a `const` assertion |
| The table of `crc32`, 16 KiB, fits in a value | it does on every width, since `isize::MAX` is 32 KiB minus 1 on 16 bits |
| The size given to the evaluator of rules is at most `isize::MAX`, and so are an offset that a rule read at and `size_of_val(self)` (`narrow::SIZE_MAX`, the ranges of "Narrowing") | Rust: it is the size of a value, or the length of a slice of bytes |
| A register is 64 bits where `usize` is, else 32 bits (`narrow::REGISTER`) | a choice of speed, not of results: every width computes the same, and `narrow_tests.rs` checks both |

### 12. What Rust keeps of a layout, and the `unstable` feature

Rust leaves some parts of the layout of some types to the compiler
([`r-layout.guarantees`][layout guarantees],
[`r-layout.repr.rust.unspecified`][rust unspecified]). The compiler may choose otherwise in its
next version, or at random with [`-Zrandomize-layout`]. These parts are the following.

- The order of the fields of a struct or a tuple of the `Rust` representation, and so its size.
  For example, `(u8, u16, u8)` is 4 or 6 bytes.
- The place and the values of the tag of an enum without `repr`.
- The niches of `Option`, except where `core` documents them.

`puns` reads what the compiler chose (section 1), so a pun is sound for the build that makes it,
whatever the compiler chose. The `unstable` feature is about whether the pun is the same under
another choice.

**The rule.** If `-Zrandomize-layout` can change what a pun does, the pun needs the `unstable`
feature. Otherwise it compiles without the feature. What a pun does is whether it compiles, what
it checks, and which values a check accepts. The message of a refusal is not part of it, and may
name what the compiler chose. For example, `(u8, u16, u8)` read as `[u8; 4]` is refused for its
layout, or for its size when it is 6 bytes. With the feature, `puns` takes the layout that
`rustc` chose, as it is, and does everything that it can with it. Within one build, the feature
only adds. A pun that compiles without the feature compiles with it, and does the same. The model
check runs both ways (section "Model check").

The rule is a property of a pun, not of a type. `(bool, u8)` read as bytes is proven whatever
byte its `bool` is in. Bytes read as `(bool, u8)` would check one byte or the other, so that pun
is refused.

**What a check may read.** A check must not read a byte of the source that, in another layout
that Rust allows, holds another field. Otherwise the check would read another part of the same
value, and could give another answer. So a check that reads a byte of a struct whose fields Rust
may put in another order is refused (`analysis::reads_moved`). The analysis counts exactly the
bytes that a check reads, in two cases.

- The check of an enum reads its tag, then the fields of the variant whose tag it is. So a byte
  under a variant counts only for the values of the source whose tag is the tag of that variant.
  This holds where the source has an enum with its tag at the same place, of the same size and
  byte order.
- The elements of an array share their checks. Where the source has elements of the same size at
  the same places, what the check of the first element reads tells what the check of each
  element reads. Otherwise every byte under the array counts.

The rules of a struct read any of its bytes, so a checked rule counts every byte of its struct.

Which field a byte holds is not part of what a pun does. `pun::<[u8; 2], (u8, u8)>([1, 2])` is
`(1, 2)` or `(2, 1)`. Either is a valid value, which is all that a pun promises, as `transmute`
does. What is the same in every layout is what `puns` does with given bytes. Which bytes a value
of a struct without `repr` has is the choice of the compiler, with or without `puns`. What a check
says of a value is something that `puns` does. Whether `try_pun::<(u8, u8), F>((0, 2))` is `Ok`,
for `F` a `repr(C)` struct of a `bool` and a `u8`, must not depend on which byte holds the `0`.

| Pun, without `unstable` | | Why |
|---|---|---|
| `[u8; 2]` as `(u8, u8)`, and back | proven | bytes of any value, in either order |
| `[u8; 2]` as `(bool, bool)` | checked | both bytes are a `bool`, in either order |
| `(bool, u8)` as `[u8; 2]`, `as_bytes` | proven | both bytes are initialized, in either order |
| `[u8; 4]` as `(u8, u16)` | proven | it only requires initialized bytes, wherever its padding is |
| a `repr(C)` struct that holds a `(bool, u8)` as itself, or its first bytes as the `(bool, u8)` | proven | the same type at the same place |
| an enum without `repr` as itself | proven | a type has one layout in a program |
| `[bool; 2]` as `(bool, u8)` | proven | the `bool` may be either byte, and both bytes are a `bool` |
| `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
| `(u8, u8)` as `[bool; 2]` | refused | the check of each element reads a byte that, in another layout, holds the other field |
| the first two bytes of `(u8, u16)` | refused | the second byte is padding in one layout and not in another |
| bytes as a `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | which byte is `a` |
| `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | the check reads one field or the other, and `(0, 2)` passes in one order only |
| `(u8, u8)` as `NonZero<u16>` | refused | the check reads both bytes, and each may hold either field |
| anything of `(u8, u16, u8)` but itself | refused | it is 4 or 6 bytes |
| bytes as an enum without `repr`, or as `Option<bool>` | refused | nothing documents where its tag is, nor its values |
| `validate_in::<(bool, u8)>` | refused | which byte it checks |

**How.** `Pun::LAYOUT` is the layout that `rustc` chose, with or without the feature. Everything
that reads values (validation, copies, the generated accessors) reads them where they are. Each
node says what Rust specifies of it (`layout::Stable`), computed from the layouts of its parts.

- `fixed` means that its offsets and its size are the same in every layout that Rust allows,
  given the sizes of its parts. A `repr(C)` or `repr(transparent)` struct, an enum of `repr(C)` or
  of a primitive representation, a `repr(C)` union and an array are fixed if the sizes of their
  parts are, since their offsets are computed from those sizes. A `repr(C)` struct whose rules
  read its bytes by offset (`le`, `be`, a checksum) is fixed only if everything in it is fixed, at
  every depth. A struct or a tuple of the `Rust` representation (`Layout::rust`) is fixed if every
  order of its fields makes the same bytes. That is so in two cases, and in both the struct must
  have no rule. In the first case, one field has a size, so it is at 0. In the second case, the
  fields that have a size are all multiples of the alignment of every field, so no order has
  padding between them. These fields must then be all bytes of any value with no rule in them,
  such as `(u8, [u8; 3])`. Or they must all be the same layout of a type that is fixed at every
  depth, such as `(bool, bool)`. A rule of the struct names fields, so `a < b` holds of other bytes
  in another order. The rule of a field holds of the bytes where the field is. An enum without
  `repr` is never fixed, and neither are `Option` and `Result` except where `core` documents them
  (`Layout::unspecified`).
- `size` means that its size is the same in every layout. For a struct of the `Rust`
  representation that is not fixed, `Layout::orders` tries every order of its fields. It places
  each field at the first offset that its alignment allows, as `rustc` does, for up to 6 fields
  that matter. The fields of `(u8, u16)` are 4 bytes in any order, and those of `(u8, u16, u8)`
  are not.
- `init`, of a node that is not fixed, means that its bytes are initialized in every layout. No
  order has padding, and the fields are initialized.
- `deep` means that nothing in the node, at any depth, is not fixed.
- `ordered` means that each byte of a value belongs to the same part in every layout. It is as
  `fixed`, except for a struct whose orders all make the same bytes. `(u8, u8)` is fixed, since
  its two bytes are of any value in either order. It is not ordered, since which byte is the first
  field is not specified. What a check reads must be ordered.

Without the feature, the analysis (`analysis::STRICT`) does not look into a node that is not fixed
(`Layout::is_opaque`), and the cursor makes it one run (`RunKind::Opaque`). The exception is a
node whose size is the same in every layout, that has no rule, and whose bytes are all of any
value or all padding. What such bytes are does not depend on where its fields are.

- **As a source**, an opaque node is what every layout of it is. It is initialized bytes if it is
  `init`, and otherwise bytes that may be padding. It is all one or all the other. That the first
  byte of a `(u8, u16)` is never padding is not used.
- **As a target**, an opaque node is proven by the same type at the same place (below). A struct
  or a tuple whose size is the same in every layout may also be proven field by field
  (`analysis::relate_fields`). **A field may be anywhere, so it must be proven everywhere it may
  be.** A struct is valid if each of its fields is valid at its place. So the source proves the
  struct in every order of its fields if, and only if, it proves each field at each offset that
  the field has in some order. This is exact, and no order is tried. The offsets taken are the
  multiples of the alignment of the field where the field fits. Where no order needs padding,
  they are the multiples of the greatest common divisor of the sizes, of which every sum of sizes
  is one. A few of these offsets are in no order, such as the second byte of a
  `(bool, [u8; 3])`. More is then required, never less. So two `bool`s prove a `(bool, u8)`, four
  initialized bytes prove a `(u8, u16)`, and two `NonZero<u16>` prove a `(NonZero<u16>, u16)`. A
  struct that has a rule, which names fields, or a pointer, which is paired by its place, proves
  only itself, as an enum does.
- **Nothing in an opaque node is checked at run time.** A check would read where the compiler may
  not have put the field. What is not proven does not compile (`Relation::unspecified`), and
  whatever reads a relation reads this. That includes what a pointer aims at (`deep::behind`),
  what a variant of a view takes from another (`view::base`), and an enum or a union that would
  be checked as a whole, fields included, which the relations of its variants report. So the
  marks of the analysis, which tell validation what to check, are at places that every layout
  shares.

Nothing is found in a source that is not fixed, where `rustc` put it. A rule is proven by the same
rule of the same struct at the same place (section 4). The analysis looks for it through the
fields of the source (`proves_rules`, `covered`, `same_elements`), but not through a node that is
not fixed. Two layouts are the same there only if what Rust does not specify of them is of the
same type (`analysis::equal`). A `&mut` to a part covers such a node whole, by the same type, or
not at all (`cut`). A pun of a type whose size Rust does not specify compiles only if both sides
are the same type (`analysis::sizes_kept`). A function of `Visit` compiles only if nothing that
Rust does not specify has something to check or holds a pointer (`Layout::checks_kept`).
`zeroed` does not accept the zeros of an enum that is not fixed.

**The same type** is told by an identity (`layout::Id`) and by equal layouts. A type has one
layout in a program, but two types, however alike, may each be arranged otherwise
([`offset_of!`, "Layout is subject to change"][offset_of layout]). For example,
`-Zrandomize-layout` does arrange `Wrapper<u8, u8>` and `Wrapper<u8, i8>` otherwise. Stable Rust
compares no types in `const`. `TypeId` cannot be compared there, and `type_name` is not a
`const fn`. So an identity is a name, with the identities of what makes the type. The primitive
types, the types of `num`, arrays, slices, tuples, `Option`, `Result` and `MaybeUninit` have
identities, and `ManuallyDrop<T>` is `T`. A type that derives `Pun` is named by where it is
declared (`__site!`): the module, the file, the line and the column, and its name. A generic type
that derives `Pun`, a `PhantomData`, a `Scalar` implemented by hand, and a self-relative pointer
have no identity (`Id::NONE`), which equals nothing. A type that holds one of them proves itself
only with the feature. An identity only ever refuses. The layouts are compared as well, so two
types that would share a name are still read by their layouts.

**What this rests on that Rust does not document.** `rustc` places the fields of a struct of the
`Rust` representation one after the other, each at the first offset that its alignment allows, in
an order of its choosing. So the sizes of the orders are the sizes that it may give.
`-Zrandomize-layout` draws the order at random, and does nothing else. Rust only bounds the
offsets ([`r-layout.repr.rust.layout`][rust layout]). A compiler could put padding where no
alignment needs it. A size that is read would then differ from the one that `Layout::rust` takes
as the same in every layout, which makes it not so, because the size is compared with the one of
the compiler. A struct that was fixed would stop being fixed. The pun then fails at compile time
rather than change what it does. The alignment of a type is read as well. Rust gives the alignment
of a primitive for each platform, and makes a struct at least as aligned as its fields.
`-Zrandomize-layout` does not change the alignment.

**`Option` of a self-relative pointer** is the same pointer, made nullable, with or without the
feature (section 10). Rust documents this for a `repr(transparent)` struct around a `NonZero`,
which is how a pointer is to be declared. `self_relative!` says so, since `puns` cannot see the
`repr` of the type.

**What this rests on that Rust documents.** The table lists each guarantee and where Rust states
it. "R" is the [Reference], and "C" the documentation of [`core`].

| Guarantee | Where |
|---|---|
| Layout may change with each compilation, and only what is listed is guaranteed | R [`r-layout.guarantees`][layout guarantees] |
| A size is a multiple of the alignment, and the stride of an array | R [`r-layout.properties.size`][size] |
| The sizes of `bool`, integers, floats and `char`, and of `usize` and `isize` | R [`r-layout.primitive.size`][primitive size], [`r-layout.primitive.size-align`][primitive size-align] |
| The alignment of a primitive is the one of the platform, read and not assumed | R [`r-layout.primitive.platform-specific-alignment`][primitive align] |
| Any initialized bytes are an integer or a float | R [`r-type.numeric.validity`][numeric validity] |
| `bool` is one byte, 0 or 1 | R [`r-type.bool.layout`][bool layout], [`r-type.bool.repr`][bool repr] |
| `char` is a `u32` that is a Unicode scalar value | R [`r-type.char.value`][char value], [`r-type.char.layout`][char layout] |
| `[T; N]` is `N` elements one after the other, `[T]` is laid out as the array, and `str` as `[u8]` | R [`r-layout.array`][array], [`r-layout.slice`][slice], [`r-layout.str`][str] |
| `()` has size 0 and alignment 1. `!` has the same, and no value | R [`r-layout.tuple.unit`][unit], [`r-type.never.layout`][never layout] (nightly Reference) |
| A value is valid if its fields, or its elements, are. Padding may be uninitialized | R [`r-undefined.validity.struct`][validity struct], [`r-undefined.validity.undef`][validity undef] |
| An enum has a valid discriminant, and the fields of its variant are valid | R [`r-undefined.validity.enum`][validity enum] |
| A `repr(C)` struct has its fields in order, each at the next offset that its alignment allows | R [`r-layout.repr.c.struct`][c struct] |
| A `repr(C)` union has every field at 0 | R [`r-layout.repr.c.union.size-align`][c union] |
| The validity of a union is not decided, so a target is one of its fields, and a source proves nothing | R [`r-undefined.validity.union`][union validity] |
| An enum without fields, `repr(C)` or of a primitive representation, is that integer, which holds the discriminant | R [`r-layout.repr.c.enum`][c enum], [`r-layout.repr.primitive.enum`][primitive enum], [`r-items.enum.discriminant.access-memory`][discriminant memory] |
| An enum with fields, `repr(C)`, of a primitive representation, or both ([RFC 2195]) | R [`r-layout.repr.c.adt`][c adt], [`r-layout.repr.primitive.adt`][primitive adt], [`r-layout.repr.primitive-c`][primitive c] |
| `repr(transparent)` has the layout of its field | R [`r-layout.repr.transparent.layout-abi`][transparent] |
| `packed(N)` and `align(N)` | R [`r-layout.repr.alignment`][alignment] |
| The `Rust` representation puts each field at a multiple of its alignment, and the type is at least as aligned as its fields | R [`r-layout.repr.rust.layout`][rust layout] |
| The fields of a struct of the `Rust` representation do not overlap, in an order that is not specified | R [`r-layout.repr.rust.struct`][rust struct] |
| A struct of the `Rust` representation whose fields have no size has none. So does an enum without variants, or of one variant whose fields have no size | R [`r-layout.repr.rust.struct-zst`][struct zst], [`r-layout.repr.rust.enum-empty-zst`][enum empty], [`r-layout.repr.rust.enum-struct-like-zst`][enum struct-like] |
| The `Rust` representation guarantees nothing else | R [`r-layout.repr.rust.unspecified`][rust unspecified] |
| A tuple has the `Rust` representation | R [`r-layout.tuple.def`][tuple] |
| The representation of a type does not change the layout of its fields | R [`r-layout.repr.inter-field`][inter-field] |
| A type has one layout in a program, and a similar type may have another | C [`offset_of!`, "Layout is subject to change"][offset_of layout], R [`r-type.struct.layout`][struct layout] |
| Where and how the discriminant of an enum without `repr` is stored is not defined | R [`r-items.enum.discriminant.type`][discriminant type], C [`mem::discriminant`] |
| The fields of a union without `repr` need not be at 0, so such a union is refused | R [`r-type.union.layout`][union layout] |
| `NonZero<T>` is `T`, except zero | C [`NonZero`, "Layout"][nonzero layout] |
| `Option<T>` is `T`, and `None` is zeros, for a `NonZero` and a `repr(transparent)` struct around one | C [`option`, "Representation"][option representation] |
| `Result<T, E>` is the same when the other of `T` and `E` has size 0 and alignment 1 | C [`result`, "Representation"][result representation] |
| `ManuallyDrop<T>` has the layout and the valid values of `T` | C [`ManuallyDrop`] |
| `MaybeUninit<T>` has the size and the alignment of `T`, and any bytes are a value | C [`MaybeUninit`, "Layout"][maybe-uninit layout], [`MaybeUninit`, "Validity"][maybe-uninit validity] |
| `PhantomData<T>` has size 0 and alignment 1 | C [`PhantomData`, "Layout"][phantom layout] |

Rust documents more than `puns` uses yet. `UnsafeCell`, `Cell`, `Wrapping`, `Saturating` and the
atomics have the layout of what they hold, and `puns` implements `Pun` for none of them. An enum
of two variants, one of a type that `Option` is documented for and one without a sized field, is
["ABI-compatible"][abi compatibility] with that type. That gives its size, but not which bytes
its other variant is.

Niches are not guaranteed beyond the rows of `Option` and `Result` above. `Option<bool>` is one
byte, with `None` at 2, but that is how `rustc` does it today. The documentation of
`MaybeUninit` and of `UnsafeCell` says that such optimizations vary. So `Option<bool>` needs the
`unstable` feature. Niches that Rust documents only as "ABI-compatible" are not taken either. An enum
without `repr` of a variant that holds a `NonZero` and a variant without fields, such as the
`Ticket` of the README, needs the feature, though `rustc` lays it out as the `Option` of it.

**How it is checked.** The tests of `puns` run without the feature and with it.

- Without it (`cargo test -p puns --features verdict`), `tests/seeds.rs` records the verdict of
  every function on every pair of 34 types. The types are those of the table above and, next to
  them, what holds them: an enum and a union of a specified representation, a struct with a rule,
  arrays and pointers. The record covers each pair whole, in part (by `&mut` and from the end),
  and behind pointers. It also records which byte patterns each type accepts from bytes, but not
  the message of a refusal, which may name what `rustc` chose. The test compares the record with
  `tests/seeds.txt`. CI runs it without `-Zrandomize-layout`, and with it under three seeds, and
  the record must be the same each time. A rule, an enum checked as a whole, and what a pointer
  aims at each have a type in the record.
- The cases of `tests/compile-fail-kept` hold each refusal and its message, and the cases of
  `tests/compile-pass-kept` what compiles. `tests/compile_fail.rs` runs them without the feature,
  and runs `tests/compile-fail-unstable` with it. The other cases are the same both ways.
- With the feature (`--features unstable`), the targets that need it run as well
  (`required-features` in `Cargo.toml`). These are enums without `repr`, `validate_in` of
  `Option<bool>`, the example `codegen` and the benchmark. The tests about the layout that `rustc`
  chose (`#[cfg(feature = "unstable")]`) run too.

## Model check

`puns-model/` compares every verdict of the analysis with oracles that do not use the analysis.
It goes through the test-only `verdict` feature (`puns::hook`), which says what each function
would do without failing to compile. It runs without the `unstable` feature of `puns` and with
it, through a feature `unstable` of its own. Without it, a refusal for what Rust does not specify
counts as imprecise, by design (`hook::Verdict::unspecified`).

The model has about 140 types of 1 to 8 bytes. It checks every ordered pair of types of the same
size, and every value of the source over an alphabet, which is every value up to 2 bytes. It
checks slices and structs that end with a slice on every pattern of up to 6 bytes. The oracles
tell what a type requires (`valid(..)`, rules) from what it guarantees. The model check reports
three kinds of verdicts.

- A verdict is **unsound** if it lets through a value that the type system does not allow, or
  reads a byte of padding.
- A verdict is **incorrect** if it rejects a valid value at run time, or leaves unchecked a
  requirement of the target that the source does not imply.
- A verdict is **imprecise** otherwise, where it refuses or checks what it could prove. These are
  counted.

The oracle finds what the requirements of a source imply, one requirement at a time. The oracle
of each type says what the type requires, with where and what each requirement is
(`puns_model::require`). A requirement of the target is implied if every value of the source
that meets the requirements of the source meets it (`implied`). A requirement may be left
unchecked only if it is implied. Nothing of the test of the library (the same rules, the same
extent) is in the oracle. So a rule of the target wrongly taken as proven by another rule is
reported. The types include the same rules over other fields, another rule, and another extent.
They put these rules in a struct, an enum with and without `repr`, an array, a union, and a struct
with a byte that may be uninitialized. The model check reports a rule proven by another (a
`same_rules` that compares nothing) and a `&mut` part over a rule that the target does not have
(a `cut` without its test). Both faults were tried.

The types include enums without `repr`: a niche in a `bool`, in a `NonZero` and in a `char`, a
niche shared by two variants next to a field of a third, a direct tag of one byte and one that
`rustc` widened, one variant, and a variant without values that `rustc` gives a tag. Their oracles
are written from how `rustc` lays out enums. They are checked against the bytes of values
natively, and by Miri on the values that the oracle makes. The offsets of the fields are taken
from values, never from the library. The types also include unions, whose oracle is "one of its
fields", a variant without values, `MaybeUninit`, and a scalar that may be uninitialized
(`Zero`). Against `Zero`, reading such a scalar as initialized is found unsound.

Parts are checked over the same types, for about four thousand pairs whose target is not larger
than the source. `head*` and `tail*` are checked against the oracle of the target at offset 0 and
at `size(S) - size(D)`, with the bytes and the address that they give at run time. `head_mut` and
`tail_mut` are checked by writing every value of the target into every value of the source. The
result must be a valid source. If the source met its requirements and what was written meets
those of the target, the result must still meet the requirements of the source. A rule of the
source over the part, or a part of an enum, would break that. The checks catch a wrong shift, in
the analysis or in the pointer, and a `mutable_part` without its second condition.

View enums are checked against what they mean (`puns_model::views`). The meaning is the first
variant that the bytes are, found by trying each variant in turn with `from_bytes` and
`from_head` of its type, which the pairs above check against the oracles. Every ordered pair of
the types of 1 and of 2 bytes is a view of two variants (`Two<A, B>`, a generic view enum, in
which `A` is decided from `B` where `B` takes every `A`). It is compared on every pattern of bytes
up to one more than its size, at an aligned and a misaligned address. The comparison covers the
variant, its address, its size and what remains. A variant that its type does not take from the
bytes is unsound. Another variant, or none, is incorrect. A sample runs on a push
(`tests/views.rs`). It has views of three variants, a chain of bases, a base shared by two
variants, and views of unsized types, whose extents `from_head` takes from a rule or as many
elements as fit. Under Miri, a view runs for each unsafe path of `view.rs`. Every pair runs with
the `full` feature. The report counts the variants that have a base, since a view without one
checks nothing new. The checks report each of the following faults, which were tried: a variant
checked for nothing from its base, a base chosen without the test that it takes the variant, a
base for `from_head` that takes other bytes, extents compared with their trailing padding, and a
variant rebuilt from its state with another length.

Every check that CI runs on a push takes at most a minute. So `cargo test` runs a sample of the
model check, and Miri runs a selection of pairs chosen for the unsafe paths that they reach, not
for the verdicts. Miri is there to find undefined behavior (`puns-model/tests/miri_sample.rs`
lists what each pair is for). The examples other than `ethernet` do not run under Miri. They are
declarations, and the unsafe code that they reach is that of the library, which its own tests and
`ethernet` run there. The exhaustive run, every pair and every part of every type, is behind the
`full` feature, which `cargo test` does not build. It runs with
`cargo test --profile model -p puns-model --features full`, optimized and without LTO. It takes
minutes to build and a few minutes to run. The `model.yml` workflow runs it without and with the
`unstable` feature, every night and on demand. `puns-model/src/lib.rs` says what is checked.

Self-relative pointers have a model of their own (`puns_model::pointers`), on the pointers of the
tests of `puns` (`tests/support/relative.rs`). These pointers aim at one value, a sequence, or
text, and may be nullable. They are declared through the public API only, built in a buffer, and
followed through it. Sixteen types that hold pointers are each described by hand: recursive
structs, through a nullable pointer and through a sequence, enums, a pointer to a `bool`, to a
`u8`, to two bytes, to a sequence and to text, another byte order, and bytes. The oracle is a
reference validator that walks concrete bytes with such a description. Every byte that it reads
must be in bounds and written, every value valid, and every pointer must lead to a place aligned
for its target that holds one. Every value of a corpus of a source is read as each target. What
`view_ref` accepts must be valid, or the verdict is unsound, and is then read through, under Miri
too. `try_view_ref` must accept the values that are valid and only them. What the oracle alone
says of pairs whose answer is known is asserted, so that the oracle cannot accept everything. The
builder leaves a byte that nothing writes after every value, so that a target read past its end
is seen. `hook::views` answers with the `const` code that decides the pairing (`deep::local`,
pointers followed a bounded number of times). The functions themselves are called for the pairs
that it accepts, and the compile-fail tests are the pairs that they refuse. A sample runs on a
push (`cargo test -p puns-model --test pointers`), and every pair of every type runs with `full`,
at night. The model counts as imprecise what is refused by design: a target of another size, a
sequence or text read as something else, and a check that would be behind a pointer.

## Not done yet

- **One walk to copy a value and hand over its pointers.** A deep copy is `copy_in` then
  `pointers_in` on the same value. The tag of an enum is then read, and its variant chosen,
  twice. One walk could hand the pointers to the visitor during the copy. But the visitor of a
  deep copy allocates the targets, which can move the buffer under the copy. The walk would have
  to keep the pointers aside until it ends. A prototype that kept them on a stack of its own was
  twice as slow, with an indirect call, a push and a pop for each pointer.
- **Checks behind pointers.** `try_view_ref` checks the value that it is given, and requires of
  what that value points to what `view_ref` requires. Checking behind pointers is a walk of the
  buffer with the marks of each pair of types. Only the crate that has the pointers can make that
  walk, since it knows the way to the targets. `validate_in` is the walk for one type, with
  everything checked. The version for a pair would be a function of its own,
  `validate_pair_in::<S, D, V>`. It would check what the marks of the analysis of `S` as `D`
  leave, and hand over pairs of pointers through a method added to `Visit` with a default body,
  `pointer_pair::<PS, PD>(at)`. By default, that method would validate the target of `PD` alone.
  Adding either breaks nothing of `validate_in`.
- **Precise mutable parts.** `head_mut` and `tail_mut` refuse any source with a rule over the part
  that the target does not have over the same extent. Yet the operations of a rule tell which
  fields and bytes it reads, so a rule over other fields only could be allowed. They refuse any
  enum that the part covers only in part. Yet an enum all of whose variants accept what is written
  could be allowed. A `pun_mut` of a whole value does not require this of rules. Through a
  `&mut Plain`, a struct read from a `&mut S` whose rule `Plain` does not have can be written past
  that rule, as through the fields of `S` itself. A rule is a requirement (section 5). The two
  could be made the same, either way.
- **Ranges from conditions.** A value is narrowed by what it is made of and by the reads that
  succeeded, not by a comparison that held. The condition of a walk (`|i| i < count`) does not
  narrow its step, nor does the condition of an `if` narrow its branches. So a counter that a walk
  increments up to a field is computed on 128 bits, unless it is read at. The state of a walk
  nested two deep or more may be anything. The examples have neither case, since their walks move
  offsets.
- **Rules implied by other rules.** A rule is proven by the same rules, not by other rules that
  imply it. `v % 6 == 0` implies `v % 3 == 0`, but does not prove it. Nor is a rule proven by the
  same rule of a struct of another extent that does not read the extent, such as `a != b` in a
  struct of 2 bytes and in one of 3. The model check counts as nothing what the source implies
  and the library checks all the same. It is not wrong.
- **A rule that two variants of a view share, where neither takes the other.** A variant is
  decided from a later variant that takes all its values ("Views"), not from one that only shares
  a rule with it. Consider two such variants, which the bytes could both be but for the rule. Each
  requires a field of its own that the bytes have, or a rule of its own after the shared one. For
  bytes that fail the rule, each variant computes it. The later variants that have a rule that
  failed, over the same extent at the same place (`Layout::same_rules`), could be ended at once.
  The validation would then report which rule failed. Every check would carry that report, where
  it is now a `bool` that the optimizer folds. A view that declares, after them, a variant that
  takes both gets this already, and shares what succeeds as well. `Frame<Ipv4>` after two kinds of
  IPv4 packet is such a variant.
- **Checked parts of a typed value** (`try_head` and so on). A part is proven or refused. Bytes
  have `from_head` and `from_tail`.
- **Precise checks of an enum made from something else.** When only some of its variants are
  proven, or some values of the source are no tag, the whole enum is checked at run time: its tag,
  then the fields of its variant. Only what was not proven could be checked.
- **An enum checked into what is not an enum.** Consider `try_pun` from the `Shape` of the crate
  documentation to a struct that requires an initialized byte where one variant of `Shape` has
  padding. It is refused for that byte of padding, even if the check of the tag would exclude
  that variant. As a source, an enum is known by the bytes that every variant initializes, not
  variant by variant. Relating the target to each variant of the source whose tag it may accept
  would make the pun checked.
- **Checking some fields of a union.** When the check of one field would read a byte that may be
  padding, and no field is proven, the pun is refused. Checking only the other fields would be
  sound, but the marks of the analysis cannot tell the validation which fields to skip.
- **The identity of an enum whose bytes may be uninitialized.** A scalar made by
  `Spec::maybe_uninit` equals no scalar. So a struct or an enum that holds one does not prove
  itself by `Layout::equals`. The library makes no such enum today, since the tags that it
  discovers are exact.
- **Patterns other than `str` as types.** The engine takes any DFA of at most 64 states, as a type
  that guarantees it. But no such type can come from the derive, since safe code can write the
  fields of a type that derives `Pun`. A struct requires a pattern of its bytes with a rule,
  `matches(DFA, start, end)` (`Op::Matches`), which is proven only by the same rule, as any rule
  is. Automata are compared by their states and edges, not by their `id`, which a user can
  choose.
- **A byte order that a value tells**, such as `EI_DATA` of ELF or the byte order mark of TIFF. A
  type has a byte order of its own (`u32be`, `i16le`, `f64be`, and so on). So this is a choice
  between two types, which an enum of references makes.
- **`default()`.** Building a value from the defaults of its fields (literals, sizes).
- **Fields of fields in rules.** A rule names the integer fields of its struct, not a field of a
  field (`header.length`) or an element of an array (`magic[0]`). It reads those with `be` or
  `le` at their offsets. Loading them would be sound if the offset were checked against the
  layout of the field, as a scalar of that size there, as it is for the fields of the struct.
- **Signed and floating-point fields in rules.** Rules compute on unsigned integers. A signed or
  floating-point field is refused, and is read as bits with `le` or `be`. A getter reads fields,
  not bytes. A value that only the bytes hold, such as an integer of variable length, is decoded
  by the code of the user.
- **A size that is not in the value**, such as "to the end of what holds it" (an MP4 box of size
  0).
- **Sums of types in sequences, and recursion**, such as `OPTIONAL` and `CHOICE` of ASN.1, and the
  containers of CBOR and MessagePack. A chain of types (`Then<A, B>`) is one sequence of types.
- **An arbitrary function in a rule.** Getters may call any function. The validator is `const
  fn` code, generic over the type that it validates. On stable Rust 1.100, such code cannot call a
  function that the type chooses. A call through a function pointer is refused ("function pointer
  calls are not allowed in constant functions"). So is a call of a trait method (E0015, "cannot
  call non-const associated function"), and const traits are unstable (E0658). A user function
  could be called only if validation stopped being `const`, which every function of the crate is,
  or behind a check that runs only outside of `const`, which stable Rust cannot tell. So what a
  rule computes is built in, and described by data where it varies: walks, integers of variable
  length, CRCs by their parameters, and sums. The data is all that the analysis sees, and the
  analysis does not reason about it, since a rule is proven only by the same rule. A checksum only
  makes a check stricter.

## Limits

- Pointers are paired by position. Reading a value that holds pointers as another type
  (`view_ref`) requires the fields that hold pointers to be at the same position in both structs,
  and the variants that hold pointers to be at the same position in both enums. `view_ref` does
  not look through a struct that only holds a pointer (section 8).
- The derive bounds its implementation by "the type of each field that depends on a parameter is
  `Pun`", except for the fields that name the type itself, since a recursive type would be `Pun`
  if it is. So the parameter that is the offset of a recursive type is declared with what its
  pointers need, such as `O: Offset` in the tests, a trait of the crate that declares the
  pointers. Two generic types that hold each other would have their bounds found in a circle
  (`overflow evaluating the requirement`). They state their bounds with `#[pun(bound())]` on both,
  when the bounds that the types declare are enough, as `O: Offset` is.
- The types of the fields of a struct must be at least as visible as the struct. The list of
  fields is an associated type, which the privacy rules of Rust do not allow to name a less
  visible type.
- A struct of more than about a hundred fields may need `#![recursion_limit]` to be raised in the
  crate that validates it.
- A packed struct whose unsized field has trailing padding of its own is refused.
- An enum with fields and more than about 120 variants needs `#![recursion_limit]` to be raised in
  the crate that validates it, because validation recurses over the list of the variants.
- The build of a view enum grows as the square of the number of its variants ("Views").
- `PhantomData<T>` is `Scalar` for any `T`, and deriving `Pun` exposes private fields. Section 5
  says which types must not derive `Pun`.
- The evaluator of rules has a stack of 16 values and 8 variables. An expression has at most 256
  operations and at most 4 walks nested in one another. The derive says when an expression needs
  more, and how much more. A walk takes at most `8 * size + 128` steps.
- Unions must be `repr(C)`, since the fields of another union are not known to be at 0. A union
  must not hold a pointer, and is known as nothing as a source (section 5).
- An enum without `repr` holds no self-relative pointer. Its direct tag, all the bytes before the
  first field of any variant, is 1, 2, 4, 8 or 16 bytes, or it is refused, since `rustc` makes no
  other size. A variant without values whose fields `rustc` places before those of the others
  makes the build fail rather than be guessed, since its fields cannot be observed (R4).

[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[Reference]: https://doc.rust-lang.org/reference/
[`core`]: https://doc.rust-lang.org/core/
[Miri]: https://github.com/rust-lang/miri
[`nearest`]: https://docs.rs/nearest
[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[`TransmuteFrom`]: https://doc.rust-lang.org/core/mem/trait.TransmuteFrom.html
[`ptr::metadata`]: https://doc.rust-lang.org/core/ptr/fn.metadata.html
[const traits]: https://doc.rust-lang.org/nightly/unstable-book/language-features/const-trait-impl.html
[`generic_const_exprs`]: https://doc.rust-lang.org/nightly/unstable-book/language-features/generic-const-exprs.html
[offset_of]: https://doc.rust-lang.org/core/mem/macro.offset_of.html
[offset_of layout]: https://doc.rust-lang.org/core/mem/macro.offset_of.html#layout-is-subject-to-change
[1.77]: https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/
[1.79]: https://blog.rust-lang.org/2024/06/13/Rust-1.79.0/
[1.83]: https://blog.rust-lang.org/2024/11/28/Rust-1.83.0/
[`size_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.size_of_val_raw.html
[`align_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.align_of_val_raw.html
[`long_running_const_eval`]: https://doc.rust-lang.org/rustc/lints/listing/deny-by-default.html#long-running-const-eval
[`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/main/compiler/rustc_const_eval/src/const_eval/machine.rs
[const evaluated]: https://doc.rust-lang.org/reference/expressions/block-expr.html#r-expr.block.const.evaluation
[const not executed]: https://doc.rust-lang.org/reference/expressions/block-expr.html#r-expr.block.const.not-executed
[`mem::variant_count`]: https://doc.rust-lang.org/core/mem/fn.variant_count.html
[`discriminant_value`]: https://doc.rust-lang.org/core/intrinsics/fn.discriminant_value.html
[`mem::discriminant`]: https://doc.rust-lang.org/core/mem/fn.discriminant.html
[layout guarantees]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.guarantees
[size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.properties.size
[primitive size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size
[primitive size-align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size-align
[primitive align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.platform-specific-alignment
[numeric validity]: https://doc.rust-lang.org/reference/types/numeric.html#r-type.numeric.validity
[bool layout]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.layout
[bool repr]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.repr
[char value]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.value
[char layout]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.layout
[array]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.array
[slice]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.slice
[str]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.str
[unit]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.unit
[never layout]: https://doc.rust-lang.org/nightly/reference/types/never.html#r-type.never.layout
[validity struct]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.struct
[validity undef]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.undef
[validity enum]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.enum
[union validity]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.union
[c struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.struct
[c union]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.union.size-align
[c enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.enum
[primitive enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.enum
[discriminant memory]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.access-memory
[c adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.adt
[primitive adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt
[primitive c]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive-c
[transparent]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.transparent.layout-abi
[alignment]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.alignment
[rust layout]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.layout
[rust struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct
[struct zst]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct-zst
[enum empty]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-empty-zst
[enum struct-like]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-struct-like-zst
[rust unspecified]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.unspecified
[tuple]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.def
[inter-field]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.inter-field
[struct layout]: https://doc.rust-lang.org/reference/types/struct.html#r-type.struct.layout
[discriminant type]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.type
[union layout]: https://doc.rust-lang.org/reference/types/union.html#r-type.union.layout
[nonzero layout]: https://doc.rust-lang.org/core/num/struct.NonZero.html#layout-1
[option representation]: https://doc.rust-lang.org/core/option/index.html#representation
[result representation]: https://doc.rust-lang.org/core/result/index.html#representation
[`ManuallyDrop`]: https://doc.rust-lang.org/core/mem/struct.ManuallyDrop.html
[maybe-uninit layout]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#layout-1
[maybe-uninit validity]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#validity
[phantom layout]: https://doc.rust-lang.org/core/marker/struct.PhantomData.html#layout-1
[abi compatibility]: https://doc.rust-lang.org/core/primitive.fn.html#abi-compatibility
