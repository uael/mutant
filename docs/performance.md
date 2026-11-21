# Performance

A proven pun compiles to no check. It may still compute an address or a length. A checked pun
compiles to the checks of what the analysis could not prove.

## Machine code

The table below gives the machine code of some functions of
[`puns/examples/codegen.rs`][codegen]. That file defines their types, records the machine code
of more functions, and gives the command that writes the assembly. The code was measured on
`x86_64-unknown-linux-gnu` with `rustc` 1.100.0, in release with LTO and one codegen unit.

| Function                                           | Machine code                                  |
|----------------------------------------------------|-----------------------------------------------|
| `&Position -> &[u8; 2]`                            | `mov rax, rdi; ret`                           |
| `[bool; 4] -> u32`                                 | `mov eax, edi; ret`                           |
| `&Packet<Datagram> -> &Packet` (the same rules)    | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`               |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`, 12 instructions |
| `from_head::<Frame<Ipv4>>(&[u8])`, no checksum     | 29 instructions, no loop, no call             |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 207 instructions, no call                     |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, no branch in a block      |

## Time

[`puns/benches/checks.rs`][checks] times each check made by `puns` next to the same check written
by hand. The times below are in nanoseconds per check. They are the median of 5 runs of the
benchmark, and each run takes the best of 7 samples of each case. They were measured on
2026-11-20 with:

- the library at commit `50e17d8`,
- `rustc` 1.100.0, with the `bench` profile of the workspace (`opt-level = 3`, LTO, one codegen
  unit),
- an Apple M4 Max (14 cores, 36 GiB) on macOS 26.7.2,
- `cargo bench -p puns --features unstable --bench checks`.

The machine was not idle. Other programs kept its load average between 14 and 20 during the runs,
and most cases varied by less than 10% from one run to the next. The output of each run is in
[`docs/checks-2026-11-20.txt`][record].

| Case | `puns` | by hand |
|---|---:|---:|
| `ipv4, udp, 64 bytes` | 5.83 | 5.28 |
| `ipv4, udp, 1472 bytes` | 25.1 | 25.3 |
| `ipv4, tcp, 1460 bytes` | 25.3 | 25.2 |
| `ipv4, protocol 253, 1480 bytes` | 3.21 | 2.86 |
| `ipv4, udp, 64 bytes, bad checksum` | 5.89 | 5.07 |
| `ipv6, tcp, 0 headers` | 1.71 | 1.67 |
| `ipv6, tcp, 3 headers` | 2.67 | 4.46 |
| `ipv6, tcp, 12 headers` | 6.82 | 12.4 |
| `ipv6, udp, 3 headers, 64 bytes` | 5.22 | 7.08 |
| `ipv6, udp, 0 headers, 1400 bytes` | 23.2 | 23.9 |
| `udp, from_head` | 0.87 | 0.87 |
| `view enum, udp over ipv4 (first)` | 6.46 | 5.98 |
| `view enum, tcp over ipv4` | 8.41 | 7.64 |
| `view enum, udp, bad checksum` | 6.12 | 6.27 |
| `view enum, arp` | 1.11 | 1.25 |
| `view enum, vlan` | 0.83 | 1.51 |
| `view enum, other ethertype (last)` | 0.89 | 1.20 |
| `[u8] -> [bool], 64 KiB` | 731 | 18,820 |
| `[u8] -> [Kind] (1, 2, 3, 7), 4 KiB` | 110 | 1,233 |
| `[u8] -> [Small] (no repr: a tag, a bool), 4 KiB` | 569 | 917 |
| `[u8] -> [MaybeFlag] (no repr: a niche), 64 KiB` | 28,881 | 19,014 |
| `[u32] -> [char], 4 KiB` | 96.8 | 322 |
| `crc32, 64 bytes` | 12.5 | 13.2 |
| `crc32, 64 bytes (by hand: a byte at a time)` | 12.5 | 80.1 |
| `crc-32c (a Crc), 64 bytes` | 14.2 | 13.3 |
| `crc-16/modbus (a Crc), 64 bytes` | 14.9 | 76.9 |
| `crc-16/xmodem (a Crc), 64 bytes` | 14.2 | 86.0 |
| `crc32, 1518 bytes` | 556 | 1,345 |
| `crc32, 1518 bytes (by hand: a byte at a time)` | 533 | 3,201 |
| `crc-32c (a Crc), 1518 bytes` | 445 | 1,323 |
| `crc-16/modbus (a Crc), 1518 bytes` | 436 | 3,218 |
| `crc-16/xmodem (a Crc), 1518 bytes` | 517 | 3,514 |
| `sum16, 1500 bytes` | 24.7 | 24.4 |

A case is named as in the benchmark, and `cargo bench -p puns --features unstable --bench checks
-- 'ipv6'` runs the cases whose name contains `ipv6`. The size of an IPv4 case is the size of its
UDP datagram or TCP segment, after the 20 bytes of the IPv4 header. The IPv6 cases of TCP time the
walk over the extension headers only. The IPv6 case of 1400 bytes computes a checksum that fails.
By hand, a CRC is computed 16 bytes at a time, unless the name says a byte at a time, and a slice
is checked with an iterator, such as `bytes.iter().all(|&b| b <= 1)`.

`puns` checks a slice of small elements, each a few comparisons, in blocks of up to 64 bytes,
without a branch inside a block. A slice of `MaybeFlag`, an enum without `repr` whose tag is a niche
in a `bool`, is not checked this way, and is slower than by hand. In a view enum, a variant whose
values are all values of a later variant is checked from that variant. So a wrong checksum of UDP
over IPv4 is computed once, for the IPv4 variant.

## Costs

A check never allocates and never panics. Its time grows with the size of the value. A `walk` takes
at most `8 * size + 128` steps, so a checksum or another `walk` inside a `walk` multiplies the time
by the size, at each level. Walks nest up to 4 deep. Keep this in mind before you give such a type
untrusted bytes. Each checked pun is code specialized for its two types. Measure the checks in
release. They rely on inlining, which a debug build mostly does not do.

[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[checks]: https://github.com/uael/puns/blob/main/puns/benches/checks.rs
[record]: https://github.com/uael/puns/blob/main/docs/checks-2026-11-20.txt
