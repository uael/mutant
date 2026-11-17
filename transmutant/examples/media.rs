//! Sound and video containers, in safe Rust, with nothing but `#[derive(Data)]` and its options:
//! RIFF and WAVE, and the boxes of ISO-BMFF (MP4).
//!
//! As in `ethernet.rs`, the first half of this file is type declarations only: no `unsafe`, no
//! `impl`, no parsing code. The second half reads files in place, from bytes, and checks that
//! corrupted ones are rejected.
//!
//! What is specific to these formats, and what tells it:
//!
//! | Specificity                                               | Told by                                        |
//! |-----------------------------------------------------------|------------------------------------------------|
//! | four character codes tell what a chunk or a box holds     | `tag(FourCc = u32bec<0x666d_7420>)`            |
//! | RIFF chunks are padded to 2 bytes, not counted            | `size = 8 + length + (length & 1)`, `regions(data: P = length, pad)` |
//! | the fields of a WAVE format agree with each other         | `assert(byte_rate == sample_rate * block_align)` |
//! | WAVE: chunks fill the file, "fmt " first                  | `assert(walk(12, ..) == size_of_val(self))`    |
//! | WAVE: the data is whole frames of the format              | `assert({ let data = walk(..); .. % le(32, 2) == 0 })` |
//! | MP4: a size of 32 bits, of 64 bits (if 1), or to the end (if 0) | a struct per form, an enum of references |
//! | a full box: a version and flags                           | `u8c<0>`, `u24be`                              |
//! | an array of brands, to the end of a box                   | `[FourCc]`                                     |
//!
//! What is not covered, and why:
//!
//! - MP4: the size of a box is of 4 or 8 bytes, as its first 4 bytes tell. A `size` rule reads
//!   fields: each form is a struct, and an enum of references tells them apart. This is the
//!   current way; with `be` in `size` (planned), it is one struct. Boxes inside boxes (`moov`,
//!   `trak`...) are read by use code, one level at a time: a box does not say what kind of box
//!   its children are.
//! - The samples themselves are compressed, or interleaved by tables that point anywhere in the
//!   file (`stco`, `stsz`): decoding them is not a view of the bytes.
#![deny(unsafe_code)]

use core::marker::PhantomData;

use transmutant::{
  Data, Error, Tag,
  num::{u8c, u16le, u24be, u32be, u32bec, u32le, u64be},
  unpack,
};

/// A four character code, when what it tells is not known.
#[derive(Data, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct FourCc(pub u32be);

// -------------------------------------------------------------------------------------------------
// RIFF

/// A RIFF chunk: its data, then a pad byte if its size is odd. Its data is a `P` (bytes, unless
/// told).
#[derive(Data)]
#[repr(C)]
#[data(size = 8 + length + (length & 1))]
pub struct Chunk<P: ?Sized + Tag<FourCc> = [u8]> {
  pub id: P::Tag,
  pub length: u32le,
  pub data_type: PhantomData<P>,
  #[data(regions(data: P = length, pad))]
  pub rest: [u8],
}

/// The data of the "fmt " chunk of PCM (1), IEEE float (3) or extensible (`0xfffe`) audio.
#[derive(Data)]
#[repr(C)]
#[data(tag(FourCc = u32bec<0x666d_7420>))]
#[data(assert(block_align == channels * ((bits_per_sample + 7) / 8)))]
#[data(assert(byte_rate == sample_rate * block_align))]
pub struct Format {
  #[data(valid(1, 3, 0xfffe))]
  pub encoding: u16le,
  #[data(valid(1..))]
  pub channels: u16le,
  #[data(valid(1..))]
  pub sample_rate: u32le,
  pub byte_rate: u32le,
  pub block_align: u16le,
  #[data(valid(8, 16, 24, 32, 64))]
  pub bits_per_sample: u16le,
  pub extension: [u8],
}

/// A WAVE file: chunks that fill the size that RIFF tells, "fmt " first; the "data" chunk,
/// wherever it is, holds whole frames of the block alignment of the format.
#[derive(Data)]
#[repr(C)]
#[data(size = length + 8)]
#[data(assert(be(12, 4) == 0x666d_7420))]
#[data(assert(walk(
  12,
  |at| at < size_of_val(self),
  |at| at + 8 + le(at + 4, 4) + (le(at + 4, 4) & 1),
) == size_of_val(self)))]
// The block alignment is at a fixed place, since "fmt " is first.
#[data(assert({
  let data = walk(
    12,
    |at| be(at, 4) != 0x6461_7461,
    |at| at + 8 + le(at + 4, 4) + (le(at + 4, 4) & 1),
  );
  le(data + 4, 4) % le(32, 2) == 0
}))]
pub struct Wave {
  pub riff: u32bec<0x5249_4646>,
  pub length: u32le,
  /// The form of the RIFF file: "WAVE".
  pub form: u32bec<0x5741_5645>,
  pub chunks: [u8],
}

// -------------------------------------------------------------------------------------------------
// MP4

/// A box of a 32 bits size, at least 8, and of a payload `P`.
///
/// The size of a box is in 4 bytes, or, when they are 1, in 8 bytes after its type; when they are
/// 0, the box extends to the end of the file. A struct for each ([`Box32`], [`Box64`],
/// [`BoxToEnd`]) and an enum of references ([`AnyBox`]) tell them apart.
#[derive(Data)]
#[repr(C)]
#[data(size = size)]
pub struct Box32<P: ?Sized + Tag<FourCc> = [u8]> {
  #[data(valid(8..))]
  pub size: u32be,
  pub kind: P::Tag,
  pub payload: P,
}

/// A box of a 64 bits size.
#[derive(Data)]
#[repr(C)]
#[data(size = large_size)]
pub struct Box64<P: ?Sized + Tag<FourCc> = [u8]> {
  pub size: u32bec<1>,
  pub kind: P::Tag,
  #[data(valid(16..))]
  pub large_size: u64be,
  pub payload: P,
}

/// A box that extends to the end of the file.
#[derive(Data)]
#[repr(C)]
pub struct BoxToEnd<P: ?Sized + Tag<FourCc> = [u8]> {
  pub size: u32bec<0>,
  pub kind: P::Tag,
  pub payload: P,
}

/// A box, of any of the three forms.
#[derive(Data)]
pub enum AnyBox<'a> {
  Box32(&'a Box32),
  Box64(&'a Box64),
  ToEnd(&'a BoxToEnd),
}

/// The payload of "ftyp": a brand, a version, compatible brands.
#[derive(Data)]
#[repr(C)]
#[data(tag(FourCc = u32bec<0x6674_7970>))]
pub struct FileType {
  pub major_brand: FourCc,
  pub minor_version: u32be,
  pub compatible: [FourCc],
}

/// The payload of "mvhd", version 0: a full box (a version and flags), then times of 32 bits.
#[derive(Data)]
#[repr(C)]
#[data(tag(FourCc = u32bec<0x6d76_6864>))]
#[data(assert(timescale != 0))]
#[data(get(seconds: u32 = duration / timescale))]
pub struct MovieHeader {
  pub version: u8c<0>,
  pub flags: u24be,
  pub creation: u32be,
  pub modification: u32be,
  pub timescale: u32be,
  pub duration: u32be,
  /// The rate, the volume, the matrix and the next track ID.
  pub rest: [u8; 80],
}

// -------------------------------------------------------------------------------------------------
// Use

/// A WAVE file, as bytes: 16 bits stereo PCM at 8000 Hz, a LIST chunk, two frames of data.
const WAVE: [u8; 64] = [
  b'R', b'I', b'F', b'F', 0x38, 0x00, 0x00, 0x00, b'W', b'A', b'V', b'E', b'f', b'm', b't', b' ',
  0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x40, 0x1f, 0x00, 0x00, 0x00, 0x7d, 0x00, 0x00,
  0x04, 0x00, 0x10, 0x00, b'L', b'I', b'S', b'T', 0x04, 0x00, 0x00, 0x00, b'a', b'b', b'c', b'd',
  b'd', b'a', b't', b'a', 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00,
];

/// An MP4 file, as bytes: "ftyp", "mdat" of a 64 bits size, "free" up to the end.
const MP4: [u8; 57] = [
  0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0x00, 0x00, 0x02, 0x00,
  b'i', b's', b'o', b'm', b'i', b's', b'o', b'2', 0x00, 0x00, 0x00, 0x01, b'm', b'd', b'a', b't',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
  b'f', b'r', b'e', b'e', 0x00, 0x00, 0x00, 0x00, 0x00,
];

pub fn main() {
  // ---- WAVE: chunks, a typed format, data of whole frames.
  let (file, rest) = unpack::<Wave>(&WAVE).unwrap();
  assert_eq!((file.length.get(), rest.len()), (56, 0));
  let (format, rest) = unpack::<Chunk<Format>>(&file.chunks).unwrap();
  let format = format.data().unwrap();
  assert_eq!(
    (format.encoding.get(), format.channels.get(), format.sample_rate.get()),
    (1, 2, 8000)
  );
  assert_eq!((format.block_align.get(), format.bits_per_sample.get()), (4, 16));
  let (list, rest) = unpack::<Chunk>(rest).unwrap();
  assert_eq!(
    (list.id, list.data().unwrap(), list.pad().len()),
    (FourCc(0x4c49_5354.into()), &b"abcd"[..], 0)
  );
  let (data, rest) = unpack::<Chunk>(rest).unwrap();
  assert_eq!((data.data().unwrap().len(), rest.len()), (8, 0));
  // A chunk of an odd size is padded.
  let mut odd = WAVE;
  odd[40] = 3;
  let (file, _) = unpack::<Wave>(&odd).unwrap();
  let (_, rest) = unpack::<Chunk>(&file.chunks).unwrap();
  let (list, _) = unpack::<Chunk>(rest).unwrap();
  assert_eq!((list.data().unwrap(), list.pad()), (&b"abc"[..], &b"d"[..]));
  // A byte rate that disagrees: "fmt " is a typed region, checked when read.
  let mut bytes = WAVE;
  bytes[28] = 1;
  let (file, _) = unpack::<Wave>(&bytes).unwrap();
  assert_eq!(unpack::<Chunk<Format>>(&file.chunks).unwrap().0.data().err(), Some(Error::Validity));
  // Data of 7 bytes (and padded): not whole frames.
  let mut bytes = WAVE.to_vec();
  bytes[52] = 7;
  assert_eq!(unpack::<Wave>(&bytes).err(), Some(Error::Validity));
  // A chunk that runs past the file; "fmt " not first; not RIFF; a RIFF size beyond the bytes.
  for (at, value, error) in [
    (40, 9, Error::Validity),
    (12, b'g', Error::Validity),
    (0, b'r', Error::Validity),
    (4, 0x39, Error::Size),
  ] {
    let mut bytes = WAVE;
    bytes[at] = value;
    assert_eq!(unpack::<Wave>(&bytes).err(), Some(error));
  }

  // ---- MP4: boxes of three forms, of typed payloads.
  let (first, rest) = unpack::<Box32<FileType>>(&MP4).unwrap();
  let brands: Vec<[u8; 4]> =
    first.payload.compatible.iter().map(|brand| brand.0.get().to_be_bytes()).collect();
  assert_eq!(
    (first.payload.major_brand.0.get().to_be_bytes(), brands),
    (*b"isom", vec![*b"isom", *b"iso2"])
  );
  let mut boxes = vec![];
  let mut rest = rest;
  while let Ok((any, next)) = AnyBox::unpack(rest) {
    boxes.push(match any {
      AnyBox::Box32(any) => (any.kind, any.payload.len()),
      AnyBox::Box64(any) => (any.kind, any.payload.len()),
      AnyBox::ToEnd(any) => (any.kind, any.payload.len()),
    });
    rest = next;
  }
  assert_eq!(
    (boxes, rest.len()),
    (vec![(FourCc(0x6d64_6174.into()), 4), (FourCc(0x6672_6565.into()), 5)], 0)
  );
  // A size below 8; a 64 bits size below 16; not "ftyp".
  assert!(AnyBox::unpack(&[0, 0, 0, 7, b'f', b'r', b'e', b'e']).is_err());
  let short = [&[0, 0, 0, 1][..], b"mdat", &15u64.to_be_bytes(), &[0; 8]].concat();
  assert!(AnyBox::unpack(&short).is_err());
  assert_eq!(unpack::<Box32<FileType>>(&MP4[24..]).err(), Some(Error::Validity));
  // A movie header, version 0: one minute at 1000 units a second.
  let mut header = [
    &[0, 0, 0, 108][..],
    b"mvhd",
    &[0; 12],
    &1000u32.to_be_bytes(),
    &60_000u32.to_be_bytes(),
    &[0; 80],
  ]
  .concat();
  let (movie, _) = unpack::<Box32<MovieHeader>>(&header).unwrap();
  assert_eq!((movie.payload.timescale.get(), movie.payload.seconds()), (1000, 60));
  // Version 1 (its times are of 64 bits: another struct); a timescale of 0.
  header[8] = 1;
  assert_eq!(unpack::<Box32<MovieHeader>>(&header).err(), Some(Error::Validity));
  header[8] = 0;
  header[23] = 0;
  header[22] = 0;
  assert_eq!(unpack::<Box32<MovieHeader>>(&header).err(), Some(Error::Validity));

  println!("ok");
}
