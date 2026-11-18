# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/uael/puns/releases/tag/v0.1.0) - 2026-11-18

### Added

- rules compiled to their arithmetic, and proven by the same rules in another struct ([#11](https://github.com/uael/puns/pull/11))
- `validate_in`, signed `NonZero`, a build for 16 bits: what `nearest` needs of `puns` ([#10](https://github.com/uael/puns/pull/10))
- enums without `repr`, `Option` and `Result`, unions, uninitialized bytes ([#9](https://github.com/uael/puns/pull/9))
- [**breaking**] self-relative pointers are declared by the crate that has them ([#5](https://github.com/uael/puns/pull/5))
- puns 0.1: the library, its checks and its release system ([#1](https://github.com/uael/puns/pull/1))

### Fixed

- the doc tests of the README build from the published sources ([#7](https://github.com/uael/puns/pull/7))

### Other

- a whole compile error, complex layouts and measured times in the README ([#8](https://github.com/uael/puns/pull/8))
- a README that starts from what a reader wants to do ([#6](https://github.com/uael/puns/pull/6))
- a failure returns at once, checksums and slices by blocks ([#4](https://github.com/uael/puns/pull/4))
- what a release ships: a status that is one, and the licenses ([#3](https://github.com/uael/puns/pull/3))
- README and CLAUDE.md
