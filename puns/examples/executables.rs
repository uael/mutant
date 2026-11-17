//! Executables and bytecode, in safe Rust, with nothing but `#[derive(Pun)]` and its options:
//! ELF (32 and 64 bits, of either byte order), PE/COFF, Mach-O, Java class files and WebAssembly
//! modules.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads files in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these formats, and what tells it:
//!
//! | Specificity                                     | Told by                                    |
//! |-------------------------------------------------|--------------------------------------------|
//! | ELF: the class and byte order change all fields | type parameters, an enum of references     |
//! | ELF: headers where and as long as it says       | `regions(program_headers: [P] = .., ..)`   |
//! | ELF: entry sizes are the sizes of the types     | `assert(e_phentsize == size_of::<P>())`    |
//! | an alignment that is a power of two             | `assert(p_align & (p_align - 1) == 0)`     |
//! | PE: headers where the DOS header says           | `regions(stub = e_lfanew - 64, ..)`        |
//! | PE: a magic at a distance tells the kind        | `assert(le(e_lfanew + 24, 2) == MAGIC)`    |
//! | PE: data directories and sections               | `regions(sections: [..] = .., ..)`         |
//! | Mach-O: commands that fill their room, counted  | a `walk` of an offset and a count          |
//! | Mach-O: sections counted by their segment       | `len = nsects`                             |
//! | Java: entries of a size their tag tells         | a `walk`, a table of sizes in a constant   |
//! | Java: what follows the pool, from its end       | `be(at + 2, 2) < constant_pool_count`      |
//! | WebAssembly: sections of a size in LEB128       | a struct per length, an enum of references |
//! | WebAssembly: value types                        | an enum without fields                     |
//!
//! What is not covered, and why:
//!
//! - ELF: the fields of the header are those of the file (rather than of a header struct in it),
//!   since an expression reads the integer fields of its own struct only: `header.e_phoff`
//!   cannot be named. Program and section headers are expected in that order after the header,
//!   as consecutive regions; other layouts are valid ELF, and are refused. Segments and sections
//!   themselves are found by offsets that use code follows.
//! - PE: the checksum of the image, the import and export tables (addresses that are relative
//!   to where sections are loaded, not to the file) are not views of the bytes as they are.
//! - Java: the constant pool is walked in one rule of 65 operations, near the 72 an expression that
//!   walks may have. Fields and methods are counts of things that have attributes of their own
//!   sizes, counted as well: a walk inside a walk, which is not allowed, so they are not checked.
//!   The text of a `CONSTANT_Utf8` is "modified UTF-8", which is not `str`, and no other text
//!   pattern can be declared yet.
//! - WebAssembly: the size of a section is LEB128 of 1 to 5 bytes. A `size` rule reads fields,
//!   not `be(..)`: each length is a struct, and an enum of references tells them apart. This is
//!   the current way; with `be` in `size` (planned), it is one struct. [`SmallModule`] checks
//!   that sections fill a module by a walk that reads sizes of 1 or 2 bytes only (below 16 KiB):
//!   a third byte makes the step of the walk longer than the 56 operations it may have, and so
//!   would checking that the sections are in order (79). With longer walks (planned), it is any
//!   module.
#![deny(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, from_head,
  num::{u8c, u16be, u16le, u16lec, u32be, u32bec, u32le, u32lec, u64be, u64le},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// ELF

/// An ELF file, of the integers and the headers that its type parameters tell.
///
/// `C` and `D` are the class and the data (byte order) of the identification (`u8c<1>` or
/// `u8c<2>`), `H`, `W` and `A` the integers of a half, a word and an address, `P` and `S` the
/// program and the section headers.
///
/// The class and the byte order change the size and the order of every field after them. A field
/// cannot choose its type from another field: each of the four combinations is an instance of
/// this struct ([`Elf64Le`]...), and an enum of references ([`AnyElf`]) chooses.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(e_version == 1))]
#[pun(assert(e_ehsize == if class == 2 { 64 } else { 52 }))]
#[pun(assert(e_phnum == 0 || e_phentsize == size_of::<P>()))]
#[pun(assert(e_shnum == 0 || e_shentsize == size_of::<S>()))]
#[pun(assert(e_shnum == 0 || e_shstrndx < e_shnum))]
// Program headers after the header, section headers after them.
#[pun(assert(e_phnum == 0 || e_phoff >= e_ehsize))]
#[pun(assert(e_shnum == 0 || e_shoff >= e_phoff + e_phnum * e_phentsize))]
#[pun(get(is_executable: bool = e_type == 2, is_shared: bool = e_type == 3))]
pub struct Elf<C, D, H: Copy, W, A, P, S> {
  pub magic: u32bec<0x7f45_4c46>,
  pub class: C,
  pub data: D,
  pub ident_version: u8c<1>,
  pub os_abi: u8,
  pub abi_version: u8,
  pub padding: [u8; 7],
  /// None, relocatable, executable, shared, core; or specific to an OS or a processor.
  #[pun(valid(0..=4, 0xfe00..))]
  pub e_type: H,
  pub e_machine: H,
  pub e_version: W,
  pub e_entry: A,
  pub e_phoff: A,
  pub e_shoff: A,
  pub e_flags: W,
  pub e_ehsize: H,
  pub e_phentsize: H,
  pub e_phnum: H,
  pub e_shentsize: H,
  pub e_shnum: H,
  pub e_shstrndx: H,
  /// The types that only regions name.
  pub header_types: PhantomData<(P, S)>,
  #[pun(regions(
    before_program_headers = if e_phnum == 0 { 0 } else { e_phoff - e_ehsize },
    program_headers: [P] = e_phnum * e_phentsize,
    before_section_headers = if e_shnum == 0 {
      0
    } else {
      e_shoff
        - e_ehsize
        - (if e_phnum == 0 { 0 } else { e_phoff - e_ehsize })
        - e_phnum * e_phentsize
    },
    section_headers: [S] = e_shnum * e_shentsize,
    rest,
  ))]
  pub tail: [u8],
}

/// A program header of 64 bits.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(p_filesz <= p_memsz || p_type != 1))]
#[pun(assert(p_align == 0 || p_align & (p_align - 1) == 0))]
pub struct Phdr64<W, X> {
  pub p_type: W,
  pub p_flags: W,
  pub p_offset: X,
  pub p_vaddr: X,
  pub p_paddr: X,
  pub p_filesz: X,
  pub p_memsz: X,
  pub p_align: X,
}

/// A program header of 32 bits: its fields are in another order.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(p_filesz <= p_memsz || p_type != 1))]
#[pun(assert(p_align == 0 || p_align & (p_align - 1) == 0))]
pub struct Phdr32<W> {
  pub p_type: W,
  pub p_offset: W,
  pub p_vaddr: W,
  pub p_paddr: W,
  pub p_filesz: W,
  pub p_memsz: W,
  pub p_flags: W,
  pub p_align: W,
}

/// A section header.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Shdr<W, X> {
  pub sh_name: W,
  pub sh_type: W,
  pub sh_flags: X,
  pub sh_addr: X,
  pub sh_offset: X,
  pub sh_size: X,
  pub sh_link: W,
  pub sh_info: W,
  pub sh_addralign: X,
  pub sh_entsize: X,
}

/// ELF of 64 bits, little-endian.
pub type Elf64Le =
  Elf<u8c<2>, u8c<1>, u16le, u32le, u64le, Phdr64<u32le, u64le>, Shdr<u32le, u64le>>;
/// ELF of 64 bits, big-endian.
pub type Elf64Be =
  Elf<u8c<2>, u8c<2>, u16be, u32be, u64be, Phdr64<u32be, u64be>, Shdr<u32be, u64be>>;
/// ELF of 32 bits, little-endian.
pub type Elf32Le = Elf<u8c<1>, u8c<1>, u16le, u32le, u32le, Phdr32<u32le>, Shdr<u32le, u32le>>;
/// ELF of 32 bits, big-endian.
pub type Elf32Be = Elf<u8c<1>, u8c<2>, u16be, u32be, u32be, Phdr32<u32be>, Shdr<u32be, u32be>>;

/// An ELF file, of any class and byte order.
#[derive(Pun)]
pub enum AnyElf<'a> {
  Le64(&'a Elf64Le),
  Be64(&'a Elf64Be),
  Le32(&'a Elf32Le),
  Be32(&'a Elf32Be),
}

// -------------------------------------------------------------------------------------------------
// PE/COFF

/// A PE file: a DOS header whose `e_lfanew` tells where the PE headers are, the DOS stub, then
/// the headers `O`, whose optional header is the one that `MAGIC` tells.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(e_lfanew >= 64 && e_lfanew % 8 == 0))]
// The magic of the optional header, at a distance, tells which `O` this is.
#[pun(assert(le(e_lfanew + 24, 2) == MAGIC))]
pub struct Pe<O: ?Sized, const MAGIC: u16> {
  pub e_magic: u16lec<0x5a4d>,
  pub dos: [u16le; 29],
  pub e_lfanew: u32le,
  pub headers_type: PhantomData<O>,
  #[pun(regions(stub = e_lfanew - 64, headers: O))]
  pub rest: [u8],
}

/// The PE signature, the COFF header, an optional header `H`, its data directories, the section
/// table, and the rest of the file.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_optional_header >= size_of::<H>()))]
#[pun(get(dll: bool = characteristics & 0x2000 != 0, executable: bool = characteristics & 2 != 0))]
pub struct Headers<H> {
  pub signature: u32lec<0x4550>,
  /// x86, x86-64, ARM64, ARM, ARM Thumb-2.
  #[pun(valid(0x14c, 0x8664, 0xaa64, 0x1c0, 0x1c4))]
  pub machine: u16le,
  #[pun(valid(1..=96))]
  pub number_of_sections: u16le,
  pub time_date_stamp: u32le,
  pub pointer_to_symbol_table: u32le,
  pub number_of_symbols: u32le,
  pub size_of_optional_header: u16le,
  pub characteristics: u16le,
  pub optional: H,
  #[pun(regions(
    data_directories: [DataDirectory] = size_of_optional_header - size_of::<H>(),
    sections: [SectionHeader] = number_of_sections * 40,
    rest,
  ))]
  pub rest: [u8],
}

/// The standard and Windows fields of the optional header of PE32+ (without its data
/// directories).
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(section_alignment >= file_alignment
  && file_alignment >= 512
  && file_alignment & (file_alignment - 1) == 0))]
#[pun(assert(size_of_image % section_alignment == 0))]
pub struct Optional64 {
  pub magic: u16lec<0x20b>,
  pub linker: [u8; 2],
  pub size_of_code: u32le,
  pub size_of_initialized_data: u32le,
  pub size_of_uninitialized_data: u32le,
  pub address_of_entry_point: u32le,
  pub base_of_code: u32le,
  pub image_base: u64le,
  pub section_alignment: u32le,
  pub file_alignment: u32le,
  pub versions: [u16le; 6],
  pub win32_version: u32lec<0>,
  pub size_of_image: u32le,
  pub size_of_headers: u32le,
  pub checksum: u32le,
  #[pun(valid(1..=3, 9..=14, 16))]
  pub subsystem: u16le,
  pub dll_characteristics: u16le,
  pub stack_reserve: u64le,
  pub stack_commit: u64le,
  pub heap_reserve: u64le,
  pub heap_commit: u64le,
  pub loader_flags: u32lec<0>,
  #[pun(valid(..=16))]
  pub number_of_rva_and_sizes: u32le,
}

/// The same for PE32: another magic, a `base_of_data`, sizes of 32 bits.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Optional32 {
  pub magic: u16lec<0x10b>,
  pub linker: [u8; 2],
  pub sizes: [u32le; 3],
  pub address_of_entry_point: u32le,
  pub base_of_code: u32le,
  pub base_of_data: u32le,
  pub image_base: u32le,
  pub section_alignment: u32le,
  pub file_alignment: u32le,
  pub versions: [u16le; 6],
  pub win32_version: u32lec<0>,
  pub size_of_image: u32le,
  pub size_of_headers: u32le,
  pub checksum: u32le,
  pub subsystem: u16le,
  pub dll_characteristics: u16le,
  pub reserves: [u32le; 4],
  pub loader_flags: u32lec<0>,
  #[pun(valid(..=16))]
  pub number_of_rva_and_sizes: u32le,
}

/// A data directory: where a table is, once loaded, and its size.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct DataDirectory {
  pub virtual_address: u32le,
  pub size: u32le,
}

/// A section header: its raw data, if any, is at a multiple of 512 in the file.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(size_of_raw_data == 0 || pointer_to_raw_data % 512 == 0))]
pub struct SectionHeader {
  pub name: [u8; 8],
  pub virtual_size: u32le,
  pub virtual_address: u32le,
  pub size_of_raw_data: u32le,
  pub pointer_to_raw_data: u32le,
  pub pointer_to_relocations: u32le,
  pub pointer_to_line_numbers: u32le,
  pub number_of_relocations: u16le,
  pub number_of_line_numbers: u16le,
  pub characteristics: u32le,
}

/// PE32+ (64 bits).
pub type Pe64 = Pe<Headers<Optional64>, 0x20b>;
/// PE32.
pub type Pe32 = Pe<Headers<Optional32>, 0x10b>;

/// A PE file, of either kind.
#[derive(Pun)]
pub enum AnyPe<'a> {
  Pe64(&'a Pe64),
  Pe32(&'a Pe32),
}

// -------------------------------------------------------------------------------------------------
// Mach-O

/// A Mach-O file of 64 bits, little-endian: load commands that fill `sizeofcmds`, as many as
/// `ncmds` counts, each of a size that is a multiple of 8.
#[derive(Pun)]
#[repr(C)]
// A walk whose state is the offset of a command (the low 32 bits) and the number of commands
// before it (the high bits).
#[pun(assert({
  let end = walk(
    32,
    |s| s >> 32 < ncmds,
    |s| {
      let at = s & 0xffff_ffff;
      let size = le(at + 4, 4);
      if size < 8 || size % 8 != 0 { 1 / 0 } else { (at + size) + (((s >> 32) + 1) << 32) }
    },
  );
  end & 0xffff_ffff == 32 + sizeofcmds
}))]
pub struct MachO64 {
  pub magic: u32lec<0xfeed_facf>,
  pub cputype: u32le,
  pub cpusubtype: u32le,
  #[pun(valid(1..=12))]
  pub filetype: u32le,
  pub ncmds: u32le,
  pub sizeofcmds: u32le,
  pub flags: u32le,
  pub reserved: u32le,
  #[pun(regions(commands = sizeofcmds, rest))]
  pub tail: [u8],
}

/// A load command, of any kind.
#[derive(Pun)]
#[repr(C)]
#[pun(size = cmdsize)]
pub struct LoadCommand {
  pub cmd: u32le,
  #[pun(valid(8..))]
  pub cmdsize: u32le,
  pub data: [u8],
}

/// `LC_SEGMENT_64`, and its sections.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(cmdsize == 72 + nsects * 80))]
#[pun(assert(filesize <= vmsize))]
pub struct Segment64 {
  pub cmd: u32lec<0x19>,
  pub cmdsize: u32le,
  pub segname: [u8; 16],
  pub vmaddr: u64le,
  pub vmsize: u64le,
  pub fileoff: u64le,
  pub filesize: u64le,
  pub maxprot: u32le,
  pub initprot: u32le,
  pub nsects: u32le,
  pub flags: u32le,
  #[pun(len = nsects)]
  pub sections: [Section64],
}

/// A section of a segment.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Section64 {
  pub sectname: [u8; 16],
  pub segname: [u8; 16],
  pub addr: u64le,
  pub size: u64le,
  pub offset: u32le,
  /// A power of two, as its log2.
  #[pun(valid(0..=15))]
  pub align: u32le,
  pub reloff: u32le,
  pub nreloc: u32le,
  pub flags: u32le,
  pub reserved: [u32le; 3],
}

// -------------------------------------------------------------------------------------------------
// Java class files

/// The size of a constant pool entry by its tag, 4 bits per tag.
///
/// 0 is not a tag, 1 is `CONSTANT_Utf8`, whose size is in it. A table in a constant, since an
/// `if` chain of the 17 tags would be deeper than the stack of an expression.
pub const ENTRY_SIZES: u128 = 0x3_3553_4005_5553_3995_5010;

/// A class file: the constant pool, then the class.
#[derive(Pun)]
#[repr(C)]
// The constant pool: a walk whose state is the offset of an entry (the low 32 bits) and its
// index (the high bits); `CONSTANT_Long` and `CONSTANT_Double` take two indices. What follows
// the pool is reached from its end: the access flags, this class, the super class, the
// interfaces.
#[pun(assert({
  let end = walk(
    10 + (1 << 32),
    |s| s >> 32 < constant_pool_count,
    |s| {
      let at = s & 0xffff_ffff;
      let tag = be(at, 1);
      let size = (ENTRY_SIZES >> (tag * 4)) & 15;
      // (`2 << 32` is 3 operations, a literal is 1: with shifts, this step is 50.)
      if size == 0 {
        1 / 0
      } else {
        s + (if size == 1 { 3 + be(at + 1, 2) } else { size })
          + (if tag == 5 || tag == 6 { 0x2_0000_0000 } else { 0x1_0000_0000 })
      }
    },
  );
  let at = end & 0xffff_ffff;
  end >> 32 == constant_pool_count
    && be(at + 2, 2) >= 1 && be(at + 2, 2) < constant_pool_count
    && be(at + 4, 2) < constant_pool_count
    && at + 8 + 2 * be(at + 6, 2) <= size_of_val(self)
    // An interface is abstract.
    && (be(at, 2) & 0x0200 == 0 || be(at, 2) & 0x0400 != 0)
}))]
pub struct Class {
  pub magic: u32bec<0xcafe_babe>,
  pub minor: u16be,
  /// Java 1.1 to 26.
  #[pun(valid(45..=70))]
  pub major: u16be,
  #[pun(valid(1..))]
  pub constant_pool_count: u16be,
  pub rest: [u8],
}

/// A `CONSTANT_Utf8` entry: its text is "modified UTF-8" (see the top of this file).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 3)]
pub struct Utf8Entry {
  pub tag: u8c<1>,
  pub length: u16be,
  pub bytes: [u8],
}

// -------------------------------------------------------------------------------------------------
// WebAssembly

/// A WebAssembly module: a magic, a version, then sections.
#[derive(Pun)]
#[repr(C)]
pub struct Module {
  pub magic: u32bec<0x0061_736d>,
  pub version: u32lec<1>,
  pub sections: [u8],
}

/// A module whose sections fill it, each of a known id, of a size below 16 KiB (1 or 2 bytes of
/// LEB128: see the top of this file).
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(
  8,
  |at| at < size_of_val(self),
  |at| {
    // (Written in the `if` below, this is 9 deep: more than the 8 of the stack.)
    let first = be(at + 1, 1);
    let size = if first < 0x80 {
      2 + first
    } else if be(at + 2, 1) < 0x80 {
      3 + (first & 0x7f) + (be(at + 2, 1) << 7)
    } else {
      1 / 0
    };
    if be(at, 1) > 13 { 1 / 0 } else { at + size }
  },
) == size_of_val(self)))]
pub struct SmallModule {
  pub magic: u32bec<0x0061_736d>,
  pub version: u32lec<1>,
  pub sections: [u8],
}

/// A section whose size is 1 byte of LEB128.
///
/// The size of a section is 1 to 5 bytes of LEB128 (7 bits each, least significant first): a
/// struct for each length ([`Section1`] to [`Section5`]), and an enum of references
/// ([`Section`]). This is the current way: with `be` in `size` (planned), it is one struct.
#[derive(Pun)]
#[repr(C)]
#[pun(size = s0 + 2)]
pub struct Section1 {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(valid(..0x80))]
  pub s0: u8,
  pub payload: [u8],
}

/// A section whose size is 2 bytes of LEB128 (the last one is not 0: the shortest form).
#[derive(Pun)]
#[repr(C)]
#[pun(size = (s0 & 0x7f) + (s1 << 7) + 3)]
pub struct Section2 {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(valid(0x80..))]
  pub s0: u8,
  #[pun(valid(1..0x80))]
  pub s1: u8,
  pub payload: [u8],
}

/// A section whose size is 3 bytes of LEB128.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (s0 & 0x7f) + ((s1 & 0x7f) << 7) + (s2 << 14) + 4)]
pub struct Section3 {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(valid(0x80..))]
  pub s0: u8,
  #[pun(valid(0x80..))]
  pub s1: u8,
  #[pun(valid(1..0x80))]
  pub s2: u8,
  pub payload: [u8],
}

/// A section whose size is 4 bytes of LEB128.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (s0 & 0x7f) + ((s1 & 0x7f) << 7) + ((s2 & 0x7f) << 14) + (s3 << 21) + 5)]
pub struct Section4 {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(valid(0x80..))]
  pub s0: u8,
  #[pun(valid(0x80..))]
  pub s1: u8,
  #[pun(valid(0x80..))]
  pub s2: u8,
  #[pun(valid(1..0x80))]
  pub s3: u8,
  pub payload: [u8],
}

/// A section whose size is 5 bytes of LEB128: the last one holds the 4 last bits of a `u32`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (s0 & 0x7f) + ((s1 & 0x7f) << 7) + ((s2 & 0x7f) << 14) + ((s3 & 0x7f) << 21)
  + (s4 << 28) + 6)]
pub struct Section5 {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(valid(0x80..))]
  pub s0: u8,
  #[pun(valid(0x80..))]
  pub s1: u8,
  #[pun(valid(0x80..))]
  pub s2: u8,
  #[pun(valid(0x80..))]
  pub s3: u8,
  #[pun(valid(1..0x10))]
  pub s4: u8,
  pub payload: [u8],
}

/// A section, of any length of its size.
#[derive(Pun)]
pub enum Section<'a> {
  One(&'a Section1),
  Two(&'a Section2),
  Three(&'a Section3),
  Four(&'a Section4),
  Five(&'a Section5),
}

/// A function type of the type section (with a count of parameters of 1 byte of LEB128).
#[derive(Pun)]
#[repr(C)]
pub struct FuncType {
  pub form: u8c<0x60>,
  #[pun(valid(..0x80))]
  pub params: u8,
  #[pun(len = params)]
  pub types: [ValueType],
}

/// A value type.
#[derive(Pun, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueType {
  I32 = 0x7f,
  I64 = 0x7e,
  F32 = 0x7d,
  F64 = 0x7c,
  V128 = 0x7b,
  FuncRef = 0x70,
  ExternRef = 0x6f,
}

// -------------------------------------------------------------------------------------------------
// Use

/// An ELF executable, x86-64, little-endian, as bytes: a header, a program header (`PT_LOAD`), 8
/// bytes of code, two section headers.
const ELF64_LE: [u8; 256] = [
  0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x3e, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x02, 0x00, 0x01, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// An ELF executable, 64 bits, big-endian, as bytes: a header, a program header (`PT_LOAD`), 8
/// bytes of code, two section headers.
const ELF64_BE: [u8; 256] = [
  0x7f, 0x45, 0x4c, 0x46, 0x02, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x02, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x02, 0x00, 0x01,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// An ELF executable, 32 bits, little-endian, as bytes: a header, a program header (`PT_LOAD`), 8
/// bytes of code, two section headers.
const ELF32_LE: [u8; 172] = [
  0x7f, 0x45, 0x4c, 0x46, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x34, 0x00, 0x00, 0x00,
  0x5c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x34, 0x00, 0x20, 0x00, 0x01, 0x00, 0x28, 0x00,
  0x02, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x54, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00,
  0x00, 0x10, 0x40, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// An ELF executable, 32 bits, big-endian, as bytes: a header, a program header (`PT_LOAD`), 8
/// bytes of code, two section headers.
const ELF32_BE: [u8; 172] = [
  0x7f, 0x45, 0x4c, 0x46, 0x01, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x34,
  0x00, 0x00, 0x00, 0x5c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x34, 0x00, 0x20, 0x00, 0x01, 0x00, 0x28,
  0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x54, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x05,
  0x00, 0x00, 0x10, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
];

/// A PE32+ executable, as bytes: a DOS header, the PE headers, two data directories, a section
/// header.
const PE64: [u8; 320] = [
  0x4d, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x50, 0x45, 0x00, 0x00, 0x64, 0x86, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x22, 0x00, 0x0b, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'.', b't', b'e', b'x', b't', 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// A Mach-O executable, as bytes: a segment `__TEXT` of one section, then an `LC_UUID`-like
/// command of 16 bytes.
const MACHO: [u8; 200] = [
  0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x00, 0x00, 0xa8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x19, 0x00, 0x00, 0x00, 0x98, 0x00, 0x00, 0x00, b'_', b'_', b'T', b'E', b'X', b'T', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
  0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'_', b'_', b't', b'e', b'x', b't', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'_', b'_', b'T', b'E', b'X', b'T', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
  0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
];

/// A Java class file, as bytes: a constant pool of 6 entries (one of two slots), then the class.
const CLASS: [u8; 66] = [
  0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x34, 0x00, 0x07, 0x01, 0x00, 0x05, b'H', b'e', b'l',
  b'l', b'o', 0x07, 0x00, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x01, 0x00,
  0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c',
  b't', 0x07, 0x00, 0x05, 0x00, 0x21, 0x00, 0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00,
];

/// A WebAssembly module, as bytes: a type section, a custom section "name", a function
/// section; a code section of 200 bytes follows.
const WASM: [u8; 31] = [
  0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00, 1, 7, 1, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0, 5, 4,
  b'n', b'a', b'm', b'e', 3, 2, 1, 0, 10, 0xc8, 0x01,
];

pub fn main() {
  // ---- ELF: four kinds of files, one declaration.
  for bytes in [&ELF64_LE[..], &ELF64_BE, &ELF32_LE, &ELF32_BE] {
    let (kind, entry, load, code) = match AnyElf::from_bytes(bytes).unwrap() {
      AnyElf::Le64(elf) => {
        assert!(elf.is_executable() && !elf.is_shared());
        assert_eq!(elf.section_headers().unwrap()[1].sh_type.get(), 3);
        let load = elf.program_headers().unwrap()[0];
        (
          "64 LE",
          elf.e_entry.get(),
          (load.p_flags.get(), load.p_filesz.get()),
          elf.before_section_headers(),
        )
      }
      AnyElf::Be64(elf) => {
        let load = elf.program_headers().unwrap()[0];
        (
          "64 BE",
          elf.e_entry.get(),
          (load.p_flags.get(), load.p_filesz.get()),
          elf.before_section_headers(),
        )
      }
      AnyElf::Le32(elf) => {
        let load = elf.program_headers().unwrap()[0];
        let flags = (load.p_flags.get(), u64::from(load.p_filesz.get()));
        ("32 LE", u64::from(elf.e_entry.get()), flags, elf.before_section_headers())
      }
      AnyElf::Be32(elf) => {
        let load = elf.program_headers().unwrap()[0];
        let flags = (load.p_flags.get(), u64::from(load.p_filesz.get()));
        ("32 BE", u64::from(elf.e_entry.get()), flags, elf.before_section_headers())
      }
    };
    assert_eq!((entry, load, code), (0x40_1000, (5, 8), &[0x90; 8][..]));
    assert_eq!(
      kind,
      match bytes[4..6] {
        [2, 1] => "64 LE",
        [2, 2] => "64 BE",
        [1, 1] => "32 LE",
        _ => "32 BE",
      }
    );
  }
  // The magic, the class, the byte order, the version of the identification, the type, the
  // version, the size of the header, the size of a program header, the number of sections.
  for (at, value) in
    [(0, 0x7e), (4, 3), (5, 0), (6, 2), (16, 5), (20, 2), (52, 63), (54, 55), (60, 3)]
  {
    let mut bytes = ELF64_LE;
    bytes[at] = value;
    assert_eq!(AnyElf::from_bytes(&bytes).err(), Some(Error::Validity));
  }
  // A program header of an alignment that is not a power of two: a typed region, checked when
  // it is read.
  let mut bytes = ELF64_LE;
  bytes[64 + 48] = 3;
  let elf: &Elf64Le = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!(elf.program_headers().err(), Some(Error::Validity));
  // Section headers out of the file.
  let mut bytes = ELF64_LE;
  bytes[40] = 0xf0;
  assert_eq!(AnyElf::from_bytes(&bytes).err(), Some(Error::Validity));

  // ---- PE: headers where the DOS header says, an optional header of the kind its magic tells.
  let Ok(AnyPe::Pe64(pe)) = AnyPe::from_bytes(&PE64) else { panic!() };
  assert_eq!((pe.e_lfanew.get(), pe.stub().len()), (0x80, 64));
  let headers = pe.headers().unwrap();
  assert_eq!((headers.machine.get(), headers.executable(), headers.dll()), (0x8664, true, false));
  let optional = &headers.optional;
  assert_eq!(
    (optional.address_of_entry_point.get(), optional.image_base.get()),
    (0x1000, 0x1_4000_0000)
  );
  assert_eq!((optional.subsystem.get(), headers.data_directories().unwrap().len()), (3, 2));
  let sections = headers.sections().unwrap();
  assert_eq!(
    (&sections[0].name[..5], sections[0].pointer_to_raw_data.get()),
    (&b".text"[..], 0x400)
  );
  assert!(try_pun_ref::<_, Pe32>(&PE64[..]).is_err());
  // The DOS magic, `e_lfanew`, the PE signature, the machine, the number of sections, the magic
  // of the optional header.
  for (at, value) in [(0, b'N'), (0x3c, 0x81), (0x80, b'Q'), (0x84, 0x4d), (0x86, 0), (0x98, 0x0c)]
  {
    let mut bytes = PE64;
    bytes[at] = value;
    let rejected = match AnyPe::from_bytes(&bytes) {
      Err(_) => true,
      Ok(AnyPe::Pe64(pe)) => pe.headers().is_err(),
      Ok(AnyPe::Pe32(pe)) => pe.headers().is_err(),
    };
    assert!(rejected);
  }
  // A file alignment that is not a power of two: the headers are a typed region.
  let mut bytes = PE64;
  bytes[0x98 + 36..0x98 + 38].copy_from_slice(&[0x01, 0x03]);
  let Ok(AnyPe::Pe64(pe)) = AnyPe::from_bytes(&bytes) else { panic!() };
  assert_eq!(pe.headers().err(), Some(Error::Validity));

  // ---- Mach-O: load commands that fill their room.
  let file: &MachO64 = try_pun_ref(&MACHO[..]).unwrap();
  assert_eq!((file.ncmds.get(), file.commands().len(), file.rest().len()), (2, 168, 0));
  let (command, rest) = from_head::<LoadCommand>(file.commands()).unwrap();
  assert_eq!(command.cmd.get(), 0x19);
  let segment: &Segment64 = try_pun_ref(&file.commands()[..152]).unwrap();
  assert_eq!(
    (&segment.segname[..6], segment.nsects.get(), segment.sections[0].align.get()),
    (&b"__TEXT"[..], 1, 4)
  );
  let (command, rest) = from_head::<LoadCommand>(rest).unwrap();
  assert_eq!((command.cmd.get(), command.data.len(), rest.len()), (0x1b, 8, 0));
  // A count of commands that is not theirs; a command size that is not a multiple of 8; a
  // `sizeofcmds` larger than the commands.
  for (at, value) in [(16, 3), (188, 20), (20, 0xf8)] {
    let mut bytes = MACHO;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, MachO64>(&bytes[..]).err(), Some(Error::Validity));
  }
  // A segment whose size does not count its sections.
  let mut bytes = MACHO;
  bytes[32 + 4] = 160;
  assert_eq!(try_pun_ref::<_, Segment64>(&bytes[32..32 + 152]).err(), Some(Error::Validity));

  // ---- Java: a constant pool of entries of their own sizes.
  let class: &Class = try_pun_ref(&CLASS[..]).unwrap();
  assert_eq!((class.major.get(), class.constant_pool_count.get()), (52, 7));
  let (utf8, _) = from_head::<Utf8Entry>(&class.rest).unwrap();
  assert_eq!(&utf8.bytes, b"Hello");
  // A count that the entries do not reach; a tag that is not one; this class out of the pool; a
  // `CONSTANT_Long` counted as one entry; a version from the future.
  for (at, value) in [(9, 8), (10, 2), (CLASS.len() - 11, 9), (9, 6), (7, 71)] {
    let mut bytes = CLASS;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Class>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- WebAssembly: sections of a size in LEB128.
  let mut module = WASM.to_vec();
  module.resize(31 + 200, 0);
  let small: &SmallModule = try_pun_ref(&module[..]).unwrap();
  let (mut rest, mut sections) = (&small.sections[..], vec![]);
  while let Ok((section, next)) = Section::from_head(rest) {
    sections.push(match section {
      Section::One(section) => (section.id, section.payload.len()),
      Section::Two(section) => (section.id, section.payload.len()),
      Section::Three(section) => (section.id, section.payload.len()),
      Section::Four(section) => (section.id, section.payload.len()),
      Section::Five(section) => (section.id, section.payload.len()),
    });
    rest = next;
  }
  assert_eq!((sections, rest.len()), (vec![(1, 7), (0, 5), (3, 2), (10, 200)], 0));
  let Ok((Section::One(types), _)) = Section::from_head(&small.sections) else { panic!() };
  let (function, _) = from_head::<FuncType>(&types.payload[1..]).unwrap();
  assert_eq!(function.types, [ValueType::I32, ValueType::I32]);
  assert_eq!(from_head::<FuncType>(&[0x60, 1, 0x7a]).err(), Some(Error::Validity));
  // An id that is not one; a size past the end.
  for (at, value) in [(8, 14), (29, 0xc9)] {
    let mut bytes = module.clone();
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, SmallModule>(&bytes[..]).err(), Some(Error::Validity));
  }
  // Sections out of order are not seen (see the top of this file).
  let mut bytes = module.clone();
  bytes[24] = 1;
  assert!(try_pun_ref::<_, SmallModule>(&bytes[..]).is_ok());
  // A section of 16 KiB: 3 bytes of LEB128. A module, and a section of the third form, but not a
  // small module.
  let mut large = [&WASM[..8], &[0, 0x80, 0x80, 0x01]].concat();
  large.resize(12 + 16384, 0);
  let module: &Module = try_pun_ref(&large[..]).unwrap();
  assert!(matches!(Section::from_bytes(&module.sections), Ok(Section::Three(_))));
  assert_eq!(try_pun_ref::<_, SmallModule>(&large[..]).err(), Some(Error::Validity));

  println!("ok");
}
