# Testing

`puns` has `unsafe` code, and its analysis decides what that code may do. The tests check both.

## The model check

The analysis is compared with oracles written by hand, which do not use it. This model check
covers every ordered pair of its types of the same size. It tries every value of the source for
types of up to 2 bytes, and a chosen set of values for larger ones. It runs in full every night,
and in part on every push, with and without the `unstable` feature. The [crate documentation of
`puns-model`][model] describes its types, its oracles and what runs where.

## Miri

The tests of `puns` run under [Miri] on every push: with tree borrows on x86-64, and with stacked
borrows on a 32-bit target (`i686`) and on a big-endian one (`s390x`). A test file that is not
about the `unsafe` code does not run there, such as the examples other than `ethernet`. Under
Miri, the model check runs on a selection of its pairs that reaches every `unsafe` path
([`miri_sample.rs`][sample] says which pair is for which), not on every pair.

## Compile errors

More than a hundred programs must fail to compile, each with its expected message. They are in
[`puns/tests/compile-fail/`][compile-fail] and the folders next to it.

## The derive

The derive is also built in a crate whose `core` is another crate, with an `offset_of!` that
swaps fields. This checks that the generated code trusts no name that you can redefine.

[Soundness] states what a pun guarantees, and what `puns` assumes to guarantee it.

[Miri]: https://github.com/rust-lang/miri
[model]: https://github.com/uael/puns/blob/main/puns-model/src/lib.rs
[sample]: https://github.com/uael/puns/blob/main/puns-model/tests/miri_sample.rs
[compile-fail]: https://github.com/uael/puns/tree/main/puns/tests/compile-fail
[Soundness]: https://docs.rs/puns/latest/puns/#soundness
