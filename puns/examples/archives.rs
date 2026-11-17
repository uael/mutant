//! Archives, in safe Rust, with nothing but `#[derive(Pun)]` and its options: ZIP, tar (ustar)
//! and gzip.
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads archives in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these formats, and what tells it:
//!
//! | Specificity                                    | Told by                                     |
//! |------------------------------------------------|---------------------------------------------|
//! | little-endian signatures                       | `u32lec<0x0403_4b50>`                       |
//! | ZIP: a name, extra fields, data, sized before  | `regions(name = .., extra = .., data)`      |
//! | stored data agrees with its CRC-32             | `assert(method != 0 \|\| crc32(..) ..)`     |
//! | the end of a ZIP file is found from its end    | `walk(.., \|at\| at - 1)`                   |
//! | the central directory ends at the end record   | `assert(le(end + 12, 4) + .. == end)`       |
//! | tar: sizes are octal ASCII digits              | `size = 512 + ..`, `assert(.. == 0x3030..)` |
//! | tar: data padded to 512 bytes                  | `regions(data = .., padding)`               |
//! | tar: a checksum, its own field as spaces       | `assert({ let s = walk(0, ..); .. })`       |
//! | gzip: optional fields told by flags            | `if flags & 4 != 0 { .. } else { 0 }`       |
//! | gzip: names and comments that end with a NUL   | a `walk` that counts the NULs to find       |
//! | gzip: the CRC-16 of the header                 | `crc32(0, end) & 0xffff == le(end, 2)`      |
//! | the CRC-32 and the size at the end of a member | `regions(.., crc32 = 4, isize = 4)`         |
//!
//! What is not covered, and why:
//!
//! - Compressed data (deflate) is not a view of the bytes: the CRC-32 and the size at the end of
//!   a gzip member, or of compressed ZIP entries, are of the uncompressed data, and are not
//!   checked. Stored ZIP entries are.
//! - ZIP: an entry with a data descriptor (flag 3) has its sizes after its data, so its size is
//!   not in its header: such entries are refused by [`LocalHeader`], and found from the central
//!   directory by use code. The central directory and the entries are found from the end record,
//!   which is found by a walk: what a walk finds cannot size a region, so they are not regions of
//!   [`Zip`], but are reached again by use code. ZIP64 is more declarations of the same kind.
//! - tar: the size of an entry is decoded from 11 octal digits, the 12 bytes of two integer
//!   fields (an expression reads integer fields, not arrays). The numbers of base-256 (GNU) and
//!   the headers of pax are more declarations of the same kind.
//! - gzip: the walk over the name and the comment cannot size regions either: the deflate data is
//!   a region that starts after the 10 bytes of the fixed header, of everything but the 8 last
//!   bytes, and it holds the optional fields.
#![deny(unsafe_code)]

use puns::{
  Error, Pun, from_head,
  num::{u8c, u16bec, u16le, u32be, u32le, u32lec, u48bec, u64be},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// ZIP

/// A local file header and its data, without a data descriptor.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 30 + name_length + extra_length + compressed_size)]
#[pun(assert(flags & 8 == 0))]
#[pun(assert(method != 0 || compressed_size == uncompressed_size))]
// Stored data agrees with its CRC-32.
#[pun(assert(method != 0 || crc32(30 + name_length + extra_length, size_of_val(self)) == crc))]
pub struct LocalHeader {
  pub signature: u32lec<0x0403_4b50>,
  pub version: u16le,
  pub flags: u16le,
  /// Stored, deflate, deflate64, bzip2, LZMA, zstd, xz.
  #[pun(valid(0, 8, 9, 12, 14, 93, 95))]
  pub method: u16le,
  pub time: u16le,
  pub date: u16le,
  pub crc: u32le,
  pub compressed_size: u32le,
  pub uncompressed_size: u32le,
  pub name_length: u16le,
  pub extra_length: u16le,
  #[pun(regions(name = name_length, extra = extra_length, data))]
  pub rest: [u8],
}

/// A header of the central directory.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 46 + name_length + extra_length + comment_length)]
pub struct CentralHeader {
  pub signature: u32lec<0x0201_4b50>,
  pub made_by: u16le,
  pub version: u16le,
  pub flags: u16le,
  pub method: u16le,
  pub time: u16le,
  pub date: u16le,
  pub crc: u32le,
  pub compressed_size: u32le,
  pub uncompressed_size: u32le,
  pub name_length: u16le,
  pub extra_length: u16le,
  pub comment_length: u16le,
  pub disk: u16le,
  pub internal: u16le,
  pub external: u32le,
  pub local_offset: u32le,
  #[pun(regions(name = name_length, extra = extra_length, comment = comment_length))]
  pub rest: [u8],
}

/// The end of the central directory, and the comment of the file.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(entries_on_disk == entries))]
pub struct EndOfCentralDirectory {
  pub signature: u32lec<0x0605_4b50>,
  pub disk: u16le,
  pub directory_disk: u16le,
  pub entries_on_disk: u16le,
  pub entries: u16le,
  pub directory_size: u32le,
  pub directory_offset: u32le,
  pub comment_length: u16le,
  #[pun(len = comment_length)]
  pub comment: [u8],
}

/// A whole ZIP file: its end record is found by a search from the end (the comment before it is
/// of any size); the central directory ends where the end record starts; the comment ends the
/// file.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert({
  let end = walk(size_of_val(self) - 22, |at| le(at, 4) != 0x0605_4b50, |at| at - 1);
  le(end + 12, 4) + le(end + 16, 4) == end && end + 22 + le(end + 20, 2) == size_of_val(self)
}))]
pub struct Zip(pub [u8]);

// -------------------------------------------------------------------------------------------------
// tar

/// A ustar header, then the data of its entry, padded to 512 bytes.
///
/// Its numbers are octal ASCII. The size of the data, 11 digits then a NUL or a space, is in two
/// integer fields (`size_high`, `size_low`): it is decoded digit by digit (`& 7`) where it is
/// needed, in the `size` rule and in the regions, and its digits are checked to be digits.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 512 + ({
  let digits = ((((((size_high >> 56) & 7) * 8 + ((size_high >> 48) & 7)) * 8
    + ((size_high >> 40) & 7)) * 8 + ((size_high >> 32) & 7)) * 8 + ((size_high >> 24) & 7)) * 8
    + ((size_high >> 16) & 7);
  let size = ((((digits * 8 + ((size_high >> 8) & 7)) * 8 + (size_high & 7)) * 8
    + ((size_low >> 24) & 7)) * 8 + ((size_low >> 16) & 7)) * 8 + ((size_low >> 8) & 7);
  (size + 511) / 512 * 512
}))]
#[pun(assert(size_high & 0xf8f8_f8f8_f8f8_f8f8 == 0x3030_3030_3030_3030
  && size_low & 0xf8f8_f800 == 0x3030_3000))]
#[pun(assert(size_low & 0xff == 0 || size_low & 0xff == 0x20))]
// The checksum: the sum of the bytes of the header, its own 8 bytes counted as spaces, is the
// octal number it holds (6 digits, a NUL, a space). One walk adds the bytes (in bits 16 to 39 of
// its state) and reads the digits (from bit 40: NUL and space are read as 0).
#[pun(assert({
  let s = walk(
    0,
    |s| s & 0xffff < 512,
    |s| {
      let at = s & 0xffff;
      let byte = be(at, 1);
      if at >= 148 && at < 156 {
        s + 1 + 0x20_0000 + (((s >> 40) * 7 + (byte & 7)) << 40)
      } else {
        s + 1 + (byte << 16)
      }
    },
  );
  (s >> 16) & 0xff_ffff == (s >> 40) / 64
}))]
pub struct TarEntry {
  pub name: [u8; 100],
  pub mode: [u8; 8],
  pub uid: [u8; 8],
  pub gid: [u8; 8],
  /// The 12 bytes of the size, as two integers.
  pub size_high: u64be,
  pub size_low: u32be,
  pub mtime: [u8; 12],
  pub checksum: [u8; 8],
  /// A file, a link, a directory...; extended headers.
  #[pun(valid(0, b'0'..=b'7', b'g', b'x', b'L', b'K'))]
  pub typeflag: u8,
  pub linkname: [u8; 100],
  /// `ustar\0` then `00`.
  pub magic: u8c<b'u'>,
  pub magic_rest: u48bec<0x7374_6172_0030>,
  pub version_end: u8c<b'0'>,
  pub uname: [u8; 32],
  pub gname: [u8; 32],
  pub devmajor: [u8; 8],
  pub devminor: [u8; 8],
  pub prefix: [u8; 155],
  pub pad: [u8; 12],
  #[pun(regions(data = {
    let digits = ((((((size_high >> 56) & 7) * 8 + ((size_high >> 48) & 7)) * 8
      + ((size_high >> 40) & 7)) * 8 + ((size_high >> 32) & 7)) * 8 + ((size_high >> 24) & 7)) * 8
      + ((size_high >> 16) & 7);
    ((((digits * 8 + ((size_high >> 8) & 7)) * 8 + (size_high & 7)) * 8
      + ((size_low >> 24) & 7)) * 8 + ((size_low >> 16) & 7)) * 8 + ((size_low >> 8) & 7)
  }, padding))]
  pub content: [u8],
}

// -------------------------------------------------------------------------------------------------
// gzip

/// A gzip member: a fixed header, optional fields that flags tell, deflate data, then the CRC-32
/// and the size of the uncompressed data.
///
/// The optional fields are FEXTRA (a length and data), FNAME and FCOMMENT (strings that end with
/// a NUL) and FHCRC (the low 16 bits of the CRC-32 of the header).
#[derive(Pun)]
#[repr(C)]
#[pun(assert(flags & 0xe0 == 0))]
#[pun(assert(size_of_val(self) >= 18))]
// After FEXTRA, the strings: the state of the walk counts the NULs still to find (from bit 32).
#[pun(assert({
  let start = 10 + if flags & 4 != 0 { 2 + le(10, 2) } else { 0 };
  let end = walk(
    start + ((((flags >> 3) & 1) + ((flags >> 4) & 1)) << 32),
    |s| s >> 32 != 0,
    |s| if be(s & 0xffff_ffff, 1) == 0 { s + 1 - 0x1_0000_0000 } else { s + 1 },
  );
  // The strings end before the CRC-32 and the size of the data. (Without this, a walk that runs
  // out of the member would only fail the rule where its result is needed: with FHCRC.)
  end + 8 <= size_of_val(self) && (flags & 2 == 0 || crc32(0, end) & 0xffff == le(end, 2))
}))]
pub struct Gzip {
  pub magic: u16bec<0x1f8b>,
  /// Deflate.
  pub method: u8c<8>,
  pub flags: u8,
  pub mtime: u32le,
  #[pun(valid(0, 2, 4))]
  pub extra_flags: u8,
  pub os: u8,
  #[pun(regions(rest = size_of_val(self) - 18, crc32 = 4, isize = 4))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// Use

/// A ZIP file, as bytes: "a.txt", stored, then the central directory, and its end with a
/// comment.
const ZIP: [u8; 124] = [
  0x50, 0x4b, 0x03, 0x04, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x53, 0x74,
  0x24, 0xf4, 0x0d, 0x00, 0x00, 0x00, 0x0d, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, b'a', b'.',
  b't', b'x', b't', b'h', b'e', b'l', b'l', b'o', b',', b' ', b'w', b'o', b'r', b'l', b'd', b'\n',
  0x50, 0x4b, 0x01, 0x02, 0x1e, 0x03, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x53, 0x74, 0x24, 0xf4, 0x0d, 0x00, 0x00, 0x00, 0x0d, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', b'.',
  b't', b'x', b't', 0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x33,
  0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x03, 0x00, b'P', b'K', b'!',
];

/// The fields of a ustar header of "hello.txt", 13 bytes, at their offsets; the other bytes are
/// 0. Its checksum was computed out of this file.
const TAR_FIELDS: [(usize, &[u8]); 7] = [
  (0, b"hello.txt"),
  (100, b"0000644\0"),
  (124, b"00000000015\0"),
  (136, b"14751453627\0"),
  (148, b"006422\0 "),
  (156, b"0"),
  (257, b"ustar\x0000"),
];

/// A gzip member, as bytes: "hello", compressed.
const GZIP: [u8; 25] = [
  0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07,
  0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
];

/// The same, with a file name.
const GZIP_NAMED: [u8; 35] = [
  0x1f, 0x8b, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'h', b'e', b'l', b'l', b'o', b'.',
  b't', b'x', b't', 0x00, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05,
  0x00, 0x00, 0x00,
];

/// The same, with extra fields, a file name, a comment and the CRC of the header.
const GZIP_ALL: [u8; 53] = [
  0x1f, 0x8b, 0x08, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00, 0x41, 0x42, 0x02, 0x00,
  b'h', b'e', b'l', b'l', b'o', b'.', b't', b'x', b't', 0x00, b'a', b' ', b'c', b'o', b'm', b'm',
  b'e', b'n', b't', 0x00, 0x41, 0x63, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x86, 0xa6, 0x10,
  0x36, 0x05, 0x00, 0x00, 0x00,
];

pub fn main() {
  // ---- ZIP: the whole file, then its records one by one.
  assert!(try_pun_ref::<_, Zip>(&ZIP[..]).is_ok());
  let (local, rest) = from_head::<LocalHeader>(&ZIP).unwrap();
  assert_eq!(
    (local.name(), local.extra().len(), local.data()),
    (&b"a.txt"[..], 0, &b"hello, world\n"[..])
  );
  let (central, rest) = from_head::<CentralHeader>(rest).unwrap();
  assert_eq!(
    (central.name(), central.crc.get(), central.local_offset.get()),
    (&b"a.txt"[..], 0xf424_7453, 0)
  );
  let end: &EndOfCentralDirectory = try_pun_ref(rest).unwrap();
  assert_eq!((end.entries.get(), end.directory_offset.get(), &end.comment), (1, 48, &b"PK!"[..]));
  // The stored data and its CRC; the comment length; the offset of the directory; no end
  // record at all (the search runs to the start, and fails).
  let mut bytes = ZIP;
  bytes[40] ^= 1;
  assert_eq!(from_head::<LocalHeader>(&bytes).err(), Some(Error::Validity));
  for (at, value) in [(119, 2), (115, 1)] {
    let mut bytes = ZIP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Zip>(&bytes[..]).err(), Some(Error::Validity));
  }
  assert_eq!(try_pun_ref::<_, Zip>(&ZIP[..99]).err(), Some(Error::Validity));
  // A data descriptor: the size of the data is not in the header.
  let mut bytes = ZIP;
  bytes[6] = 8;
  assert_eq!(from_head::<LocalHeader>(&bytes).err(), Some(Error::Validity));

  // ---- tar: octal sizes, a checksum, data padded to 512 bytes.
  let mut tar = [0u8; 1024];
  for (at, field) in TAR_FIELDS {
    tar[at..at + field.len()].copy_from_slice(field);
  }
  tar[512..525].copy_from_slice(b"hello, world\n");
  let (entry, rest) = from_head::<TarEntry>(&tar).unwrap();
  assert_eq!((&entry.name[..9], entry.typeflag), (&b"hello.txt"[..], b'0'));
  assert_eq!((entry.data(), entry.padding().len(), rest.len()), (&b"hello, world\n"[..], 499, 0));
  // 1000 bytes: two blocks of data.
  let mut large = vec![0u8; 512 + 1024];
  large[..512].copy_from_slice(&tar[..512]);
  large[124..136].copy_from_slice(b"00000001750\0");
  large[148..156].copy_from_slice(b"006431\0 ");
  let (entry, _) = from_head::<TarEntry>(&large).unwrap();
  assert_eq!((entry.data().len(), entry.padding().len()), (1000, 24));
  // A digit `8` in the size; a byte of the name or of the checksum (the checksum then does not
  // agree); a type; the magic; the data cut short.
  for (at, value) in [(134, b'8'), (0, b'H'), (150, b'5'), (156, b'9'), (258, b'S')] {
    let mut bytes = tar;
    bytes[at] = value;
    assert_eq!(from_head::<TarEntry>(&bytes).err(), Some(Error::Validity));
  }
  assert_eq!(from_head::<TarEntry>(&tar[..1000]).err(), Some(Error::Size));

  // ---- gzip: optional fields told by flags, the CRC of the header.
  for bytes in [&GZIP[..], &GZIP_NAMED, &GZIP_ALL] {
    let member: &Gzip = try_pun_ref(bytes).unwrap();
    assert_eq!(
      (member.crc32(), member.isize()),
      (&[0x86, 0xa6, 0x10, 0x36][..], &[5, 0, 0, 0][..])
    );
  }
  let member: &Gzip = try_pun_ref(&GZIP_ALL[..]).unwrap();
  assert_eq!((member.flags, member.rest().len()), (0x1e, 35));
  // A reserved flag; a byte of the name, of the comment, of the CRC of the header.
  for (at, value) in [(3, 0x3e), (17, b'E'), (29, b'C'), (36, 0x40)] {
    let mut bytes = GZIP_ALL;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Gzip>(&bytes[..]).err(), Some(Error::Validity));
  }
  // Not deflate; a name without its NUL, which runs out of the member.
  let mut bytes = GZIP;
  bytes[2] = 9;
  assert_eq!(try_pun_ref::<_, Gzip>(&bytes[..]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Gzip>(&GZIP_NAMED[..18]).err(), Some(Error::Validity));

  println!("ok");
}
