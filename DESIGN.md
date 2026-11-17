# Design

Status: **work in progress**. The crate does not expose its transmute functions yet; what exists
and is tested is the compile-time analysis core (`src/layout.rs`, `src/analysis.rs`).

## Goal

Safe transmutation checked at compile time, on stable Rust, between any two "plain data" types:
structs and enums of any `repr`, arrays, slices and slice-tailed structs, `str`, with

- a **compile error** when a conversion can never be valid,
- **no code at all** when it is always valid,
- **only the checks that could not be proven** when it is valid for some values (`try_*`),
- dependent lengths: a field that, through some function, is the element count or the byte size
  of another region.

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

### 1. The layout of a type is a value, not a type

`Data::LAYOUT` is a `const` tree (`layout::Layout`) built by `const fn`s from what the compiler
says of the type (`size_of`, `offset_of!`, ...):

- **scalar**: 1 to 16 bytes, a byte order, a sorted list of valid ranges. `bool` is `0..=1`,
  `char` is two ranges, `NonZero` is `1..`, a fieldless enum is a list of points;
- **struct**: fields at their actual offsets (any `repr`: the offsets come from the compiler);
  bytes that no field covers are padding;
- **array** and **slice**: an element layout and a count (or none);
- planned: **enum** (a tag scalar that selects a list of fields), **pattern** (a byte-level DFA,
  for `str`), **relations** (dependent lengths).

This tree *is* an automaton, in a compact hierarchical form: a position in it is a state.

### 2. The analysis is ordinary `const fn` code over those values

For a source `S` and a target `D`, `analysis::analyze` decides language inclusion, `L(S) ⊆ L(D)`,
by walking both trees in lockstep (a product automaton), with three things that keep it cheap:

- **runs**: a subtree without constraint is one step, whatever its size (`[u8; 1 << 40]`);
- **scalars are compared as sets, not byte by byte**: aligned scalars compare their range lists;
  misaligned ones, or ones of different byte orders, go through an exact decomposition of integer
  ranges into byte boxes (`digit_box`, `box_meets`), so `NonZero<u16>` little-endian is known to
  be `NonZero<u16>` big-endian, and `u32 >= 0x100` is known not to be;
- **periods**: arrays and slices are not unrolled. When the target is at the start of an element
  and the source is inside a repetition, the least common multiple of both strides is analysed
  once and the rest is skipped. Two slice tails, which never end, are settled in one period.

It answers:

- *fit*: how sizes map (always / for some slice lengths / never / truncated), with the trailing
  padding of unsized structs accounted for, and checked against `size_of_val_raw`;
- *uninit*: a target byte that must be initialized would be read from padding (always an error);
- *infallible* / *fallible* / *unreachable*;
- *marks*: the scalars of the target that may receive an invalid value: exactly what a fallible
  transmutation must check at run time;
- *mutable*: the reverse inclusion, required to hand out a `&mut`.

Errors are post-monomorphization (`const { assert!(..) }` in the generic function), since there
are no const traits to turn them into trait bounds. `cargo check` does not see them, `cargo build`
does.

### 3. Run-time validation is type-directed and guarded by the analysis

`try_*` functions recurse over the *types* (associated types: element, list of fields) in
`const fn`s; each scalar check sits behind `if const { marks.contains(this scalar) }`. What the
analysis proved never reaches code generation, and what remains is a load and a comparison per
scalar. (Planned; this is the design of `main`'s `validate`, with the marks added.)

### 4. Pointers to unsized types

Splitting and building wide pointers in generic `const fn` code is done by reinterpreting
`*const T` as `*const [u8]`. That representation is not documented, so it is **verified at compile
time for each unsized type** against a pointer made with a genuine `as` cast (`Data::PROBE`,
`ptr::verify`): if it ever changes, the crate stops compiling instead of misbehaving.

## Planned, by order

1. Public API (`transmute`, `transmute_ref`, `transmute_mut`, `truncate*`, `try_*`), validation,
   the derive for structs of any `repr` and fieldless enums of any `repr` (their bytes are read at
   compile time), endian and literal integers.
2. **Enums with fields.** In the engine: a source enum is analysed variant by variant, a target
   enum by relating what the source has under the tag, then each variant it can select.
   - `repr(C)`, `repr(u8)`...: the layout is specified (RFC 2195).
   - `repr(Rust)`: the layout is unspecified and stable Rust cannot name it, but it can be
     *observed* in const evaluation from a sample of each variant: field offsets by matching on
     the sample; the tag by "transplanting" bytes of one variant over another and matching again.
     Probed on 1.100: finds the niche of `enum { A(u8, bool), B(u8), C }` (the `bool` byte, values
     2 and 3) and the tag of a directly tagged enum. An experiment that goes wrong is a compile
     error (const evaluation rejects invalid tags), never undefined behavior.
     One thing cannot be observed: the *width* of a direct tag (its high bytes are zero in every
     variant, and const evaluation refuses to read bytes that may be padding). Those bytes are
     modelled as "must be zero" in a target and "may be uninitialized" in a source, which is sound
     either way.
3. **Patterns**: a layout leaf that is a byte-level DFA, for languages that are not products of
   scalars. `str` is the UTF-8 DFA (9 states). The engine steps it with sets of states and stops
   on a fixed point. `&str -> &[u8]` is free, `&[Ascii] -> &str` is proven, `&[u8] -> &str` is
   fallible and checks with `core::str::from_utf8`, `&mut str -> &mut [u8]` is rejected.
4. **Dependent lengths**: `len(region) == F(field)`, in elements or bytes. Not a regular property:
   it is a relation attached to the struct. A target relation is proven only when the source
   carries the same one, otherwise it is marked and checked at run time; parsing a prefix
   (`unpack`) uses it to find the length. `F` as an arbitrary function can only be called through
   a trait, hence not in `const` contexts on stable; a small const description of the usual
   functions (shift, mask, multiply, add) would stay `const` and analysable.

## Limits

- Unions are not planned.
- `repr(Rust)` enums with fields and more than 256 variants would need an explicit `repr`.
- A packed struct whose unsized field has trailing padding of its own is rejected.
