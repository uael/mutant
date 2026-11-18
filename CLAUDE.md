# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```sh
cargo build                          # Build all crates
cargo build --all-targets            # Also tests and examples: refusals are only reported by a build
cargo test --all                     # All tests: unit, integration, compile-fail, doc tests (README included)
cargo test --test api                # Run only one integration test file (also: bytes, parts, enums, ethernet, str)
cargo test --test api TESTNAME       # Run a single test
cargo test --test compile_fail       # Compile-fail tests (trybuild); TRYBUILD=overwrite to regenerate .stderr
cargo run --example ethernet         # The Ethernet example; it asserts its results and prints `ok`
cargo rustc --release --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code, see DESIGN.md
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests  # Miri, tree borrows
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests  # Miri, stacked borrows
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target i686-unknown-linux-gnu  # 32 bits
cargo test --profile model -p puns-model --features full  # The full model check (nightly in model.yml); `cargo test` runs a sample
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model  # A selection of the model check: the unsafe paths it reaches
cargo build -p puns --target thumbv7em-none-eabihf  # A target without std
cargo test -p puns -p puns-model --features nearest --test nearest --test ir --test nearest_compile_fail --test pointers  # The feature `nearest`: its tests, the IR example, its compile-fail tests, the sample of its model check
cargo test -p puns --features nearest --doc -- nearest view_ref  # Its doc tests
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance -Zmiri-disable-isolation" cargo +nightly-2026-11-17 miri test -p puns --features nearest --test nearest --test ir  # Miri on the feature (tree borrows only: `nearest` needs them); add `--target s390x-unknown-linux-gnu` for big-endian
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model --features nearest --test pointers  # Miri on the model check of pointers
cargo test --profile model -p puns-model --features nearest,full --test pointers_full  # The full model check of pointers
PUNS_WRITE_IMAGE=1 cargo run -p puns --features nearest --example ir  # Rewrite `examples/ir.image` (then run again: it is compared)
cargo rustc -p puns --release --features nearest --example codegen_nearest -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code of the feature
cargo xtask fmt                      # Format code (cargo fmt on the pinned nightly)
cargo xtask fmt --check              # Check formatting
cargo xtask lint                     # Clippy (workspace, tests, examples, benches; and with the feature `nearest`; -D warnings) + format check
cargo xtask lint --fix               # Auto-fix clippy + formatting
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all  # Build documentation
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p puns --features nearest  # And with the feature `nearest`
cargo deny check                     # License/advisory/ban auditing
```

The crates build on **stable** Rust (`1.100.0`, pinned in `rust-toolchain`; `rust-version` is
1.100). CI also builds for `thumbv7em-none-eabihf`, a target without `std`. A pinned nightly
(`nightly-2026-11-17`) is used by `cargo xtask fmt` (`rustfmt.toml` uses unstable options) and by
Miri, under which the trybuild test is ignored. Generate trybuild `.stderr` files with the pinned
stable.

Most refusals are panics of `const { .. }` blocks in generic functions, evaluated at
monomorphization: `cargo check` does not report them, `cargo build` does. The trybuild test has a
passing case (`tests/compile-pass/`) so that trybuild builds instead of checking.

Every check that CI runs on a push must take at most a minute, build included (CI builds the crates
of the workspace each time): check what matters, and time a check before adding one. What is
exhaustive (the full model check, behind the `full` feature of `puns-model`) runs in
`.github/workflows/model.yml`, every night and on demand, with a check of the compiler (an optimized
build refuses a wrong view behind pointers): run it when `rust-toolchain` changes. Under Miri, the
model check runs a selection of pairs that reaches every unsafe path
(`puns-model/tests/miri_sample.rs` says which pair is for which), not every pair, and of the
examples only `ethernet` runs: the others reach the same unsafe code of the library. A test file
that is not for Miri starts with `#![cfg(not(miri))]` and says why; CI gives Miri the other ones by
name, since Miri takes a second to start each test binary, even one that runs nothing. Each pair of
types in a test of the model is an instance of the analysis, about a tenth of a second of build:
keep the tests that every push builds small.

Do not let the machine code of `examples/codegen.rs` regress, nor the one of
`examples/codegen_nearest.rs` (the feature `nearest`): the instruction counts of their functions
are in `puns/DESIGN.md`; compare before and after a change of the library.

## Architecture

**puns** is type punning proven at compile time: the layout and the valid values of a type are a
`const` value (`Pun::LAYOUT`), compared by ordinary `const fn` code. A pun is **proven** (it
compiles to nothing), **checked** (only what could not be proven is checked at run time) or
**refused** (it does not compile). **`puns/DESIGN.md` is the design** (goal, what stable Rust
gives, architecture, soundness, what is not done yet): read it before changing
the library, and keep it, the crate docs and `README.md` in step with the code.

- **`puns/`** — the library (`#![no_std]`). Its optional feature `nearest` (the only
  dependency, off by default) is `src/nearest.rs`: regions of the crate `nearest`, whose
  pointers the core knows as a kind of its own (`layout::Pointer`, `deep.rs`). What is only
  built with the feature has its own tests (`tests/nearest*.rs`, `tests/ir.rs`,
  `tests/compile-fail-nearest/`), which say `#![cfg(feature = "nearest")]`, and its own CI jobs:
  the jobs without the feature must stay what they are without it
- **`puns-derive/`** — `#[derive(Pun)]`, re-exported as `puns::Pun`
- **`puns-model/`** — model check of the analysis against oracles that do not use it (not
  published; uses the test-only `verdict` feature of `puns`), with `fakecore/`, a `core`
  that lies, for what the derive must not trust
- **`xtask/`** — dev automation (fmt, lint)

## Releasing

Releases are automated with [release-plz](https://release-plz.dev/). On every push to `main`, the GitHub Action opens (or updates) a release PR that bumps versions and updates `CHANGELOG.md`. Merging that PR publishes to crates.io and creates a GitHub release with a `v<version>` tag.

- Config: `release-plz.toml`
- Workflow: `.github/workflows/release-plz.yml`
- Only `puns` and `puns-derive` are published; `xtask`, `puns-model` and `fakecore` are excluded
- `puns` owns the changelog and git tags (`v{{ version }}`); `puns-derive` has no changelog or tags of its own
- Both crates are released under the same version (`version_group` in `release-plz.toml`): the higher of their next versions
- `release_always = false`: nothing is published by an ordinary push, only by merging a release PR (a branch whose name starts with `release-plz-`). The first release, 0.1.0, is the merge of the first release PR

### Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/) — release-plz uses them to determine version bumps and generate changelog entries.

```
<type>[optional scope]: <description>

[optional body]
```

| Type | Bump | Example |
|------|------|---------|
| `fix:` | patch | `fix: reject packed structs whose tail is aligned` |
| `feat:` | minor | `feat: patterns other than str from the derive` |
| `feat!:` / `BREAKING CHANGE:` | major | `feat!: rename unpack to from_head` |
| `docs:`, `ci:`, `chore:`, `refactor:`, `test:`, `style:` | none | `ci: pin the Miri nightly` |

## Code Style

- 2-space indentation, max 100 columns
- Imports: reordered, grouped by `StdExternalCrate`, granularity `Crate`
- Clippy: `pedantic` + `nursery` at warn; `dbg_macro`, `allow_attributes`, `missing_safety_doc`, `undocumented_unsafe_blocks` denied; `#[expect(lint, reason = "..")]` where a lint is wrong for the code
- Every `unsafe` block has a `// SAFETY:` comment, every `unsafe fn` and trait a `# Safety` section
- Dual-licensed: Apache-2.0 OR MIT
