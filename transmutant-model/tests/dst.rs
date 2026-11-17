//! Slices, `str` and structs that end with a slice, from bytes of every length up to 6 (over an
//! alphabet that reaches every decision of the types below), at an aligned and a misaligned
//! address: `try_transmute_ref`, `unpack` and the infallible conversions back to bytes, against
//! oracles written from what the types are.

use core::num::NonZero;

use transmutant::{Data, Error, hook, num::u8in};

#[derive(Data, Debug)]
#[repr(C)]
struct TailU16 {
  a: u8,
  t: [u16],
}

#[derive(Data, Debug)]
#[repr(C)]
struct TailU8 {
  a: u16,
  t: [u8],
}

#[derive(Data, Debug)]
#[repr(C)]
struct TailBool {
  a: bool,
  t: [bool],
}

#[derive(Data, Debug)]
#[repr(C)]
struct Counted {
  n: u8,
  #[data(len = n)]
  t: [NonZero<u8>],
}

#[derive(Data, Debug)]
#[repr(C)]
#[data(size = size)]
struct Sized_ {
  size: u8,
  t: [u8],
}

#[derive(Data, Debug)]
#[repr(C, align(4))]
struct ZeroTail {
  a: u8,
  t: [()],
}

/// What a target is: its head, the size of an element, its alignment, and whatever bytes (of a
/// length that fits) are a valid value, with the number of elements of its slice. `unpack` is
/// told how long the value is by `prefix`, if it has a rule for it.
struct Oracle {
  name: &'static str,
  head: usize,
  stride: usize,
  align: usize,
  valid: fn(&[u8]) -> bool,
  /// For `unpack`: the size of the value that starts the bytes, if it can be told.
  prefix: fn(&[u8]) -> Option<usize>,
}

const fn utf8(b: &[u8]) -> bool {
  core::str::from_utf8(b).is_ok()
}

/// The size of a value of `head + n * stride` bytes, with its trailing padding.
const fn padded(extent: usize, align: usize) -> usize {
  extent.div_ceil(align) * align
}

const ORACLES: &[Oracle] = &[
  Oracle { name: "[u8]", head: 0, stride: 1, align: 1, valid: |_| true, prefix: |b| Some(b.len()) },
  Oracle {
    name: "[bool]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 1),
    prefix: |b| Some(b.len()),
  },
  Oracle {
    name: "[u16]",
    head: 0,
    stride: 2,
    align: 2,
    valid: |_| true,
    prefix: |b| Some(b.len() / 2 * 2),
  },
  Oracle {
    name: "[NonZero<u8>]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x != 0),
    prefix: |b| Some(b.len()),
  },
  Oracle {
    name: "[u8in<0, 0x7f>]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 0x7f),
    prefix: |b| Some(b.len()),
  },
  Oracle { name: "str", head: 0, stride: 1, align: 1, valid: utf8, prefix: |b| Some(b.len()) },
  Oracle {
    name: "TailU16",
    head: 2,
    stride: 2,
    align: 2,
    valid: |_| true,
    prefix: |b| (b.len() >= 2).then(|| 2 + (b.len() - 2) / 2 * 2),
  },
  Oracle {
    name: "TailU8",
    head: 2,
    stride: 1,
    align: 2,
    valid: |_| true,
    prefix: |b| (b.len() >= 2).then_some(b.len() & !1),
  },
  Oracle {
    name: "TailBool",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 1),
    prefix: |b| (!b.is_empty()).then_some(b.len()),
  },
  Oracle {
    name: "Counted",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b[0] as usize == b.len() - 1 && b[1..].iter().all(|&x| x != 0),
    prefix: |b| (!b.is_empty() && (b[0] as usize) < b.len()).then(|| 1 + b[0] as usize),
  },
  Oracle {
    name: "Sized_",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b[0] as usize == b.len(),
    prefix: |b| (!b.is_empty() && b[0] >= 1 && b[0] as usize <= b.len()).then(|| b[0] as usize),
  },
  Oracle {
    name: "ZeroTail",
    head: 1,
    stride: 0,
    align: 4,
    valid: |_| true,
    prefix: |b| (b.len() >= 4).then_some(1),
  },
];

const ALPHABET: [u8; 8] = [0, 1, 2, 0x7f, 0x80, 0xc3, 0xa9, 0xff];

/// Compare `try_transmute_ref` and `unpack` with the oracle, for every pattern of up to 6 bytes,
/// at an address that is a multiple of 4 and at one that is not.
fn check<D: ?Sized + Data + core::fmt::Debug>(oracle: &Oracle, errors: &mut Vec<String>) -> usize {
  let mut count = 0;
  for len in 0..=if cfg!(miri) { 3 } else { 6 } {
    let mut pattern = vec![0usize; len];
    loop {
      let bytes: Vec<u8> = pattern.iter().map(|&i| ALPHABET[i]).collect();
      for misalign in [0usize, 1] {
        // A buffer that is a multiple of 4 bytes, holding the bytes at `misalign`.
        let mut words = [0u32; 4];
        let start = {
          let all: &mut [u8; 16] = transmutant::transmute_mut(&mut words);
          all[misalign..misalign + len].copy_from_slice(&bytes);
          misalign
        };
        let all: &[u8; 16] = transmutant::transmute_ref(&words);
        let source = &all[start..start + len];
        count += 1;
        // `try_transmute_ref`: the whole bytes.
        let fits = len >= oracle.head
          && if oracle.stride == 0 {
            len == oracle.head
          } else {
            (len - oracle.head).is_multiple_of(oracle.stride)
          };
        let room = padded(len, oracle.align) <= len;
        let aligned = start % oracle.align == 0;
        let want = match () {
          () if !aligned => Err(Error::Alignment),
          () if !fits || !room => Err(Error::Size),
          () if !(oracle.valid)(&bytes) => Err(Error::Validity),
          () => Ok(()),
        };
        let got = hook::try_transmute_ref::<[u8], D>(source).expect("compiles").map(|_| ());
        // The library may tell the errors apart in another order: only `Ok` must match.
        if got.is_ok() != want.is_ok() {
          errors.push(format!(
            "try_transmute_ref::<[u8], {}>({bytes:02x?} at +{start}) = {got:?}, want {want:?}",
            oracle.name
          ));
        }
        if got == Ok(()) {
          let value = hook::try_transmute_ref::<[u8], D>(source).unwrap().unwrap();
          if core::mem::size_of_val(value) != padded(len, oracle.align) {
            errors.push(format!(
              "{}: size_of_val {} for {len} bytes",
              oracle.name,
              core::mem::size_of_val(value)
            ));
          }
        }
        // `unpack`: the value that starts the bytes.
        let want = match (oracle.prefix)(&bytes) {
          Some(size)
            if aligned && padded(size, oracle.align) <= len && (oracle.valid)(&bytes[..size]) =>
          {
            Some(size)
          }
          _ => None,
        };
        match hook::unpack::<D>(source).expect("compiles") {
          Ok((value, rest)) => {
            let size = core::mem::size_of_val(value);
            if want.map(|w| padded(w, oracle.align)) != Some(size) || rest.len() != len - size {
              errors.push(format!(
                "unpack::<{}>({bytes:02x?} at +{start}) took {size}, want {want:?}",
                oracle.name
              ));
            }
          }
          Err(_) if want.is_some() => errors.push(format!(
            "unpack::<{}>({bytes:02x?} at +{start}) failed, want {want:?}",
            oracle.name
          )),
          Err(_) => {}
        }
      }
      // Next pattern.
      let mut i = 0;
      while i < len {
        pattern[i] += 1;
        if pattern[i] < ALPHABET.len() {
          break;
        }
        pattern[i] = 0;
        i += 1;
      }
      if i == len {
        break;
      }
    }
  }
  count
}

#[test]
fn slices_and_tails_from_bytes() {
  let mut errors = vec![];
  let mut count = 0;
  let o = |name: &str| ORACLES.iter().find(|o| o.name == name).unwrap();
  count += check::<[u8]>(o("[u8]"), &mut errors);
  count += check::<[bool]>(o("[bool]"), &mut errors);
  count += check::<[u16]>(o("[u16]"), &mut errors);
  count += check::<[NonZero<u8>]>(o("[NonZero<u8>]"), &mut errors);
  count += check::<[u8in<0, 0x7f>]>(o("[u8in<0, 0x7f>]"), &mut errors);
  count += check::<str>(o("str"), &mut errors);
  count += check::<TailU16>(o("TailU16"), &mut errors);
  count += check::<TailU8>(o("TailU8"), &mut errors);
  count += check::<TailBool>(o("TailBool"), &mut errors);
  count += check::<Counted>(o("Counted"), &mut errors);
  count += check::<Sized_>(o("Sized_"), &mut errors);
  count += check::<ZeroTail>(o("ZeroTail"), &mut errors);
  println!("== slices and tails: {count} sources, {} errors", errors.len());
  for error in errors.iter().take(40) {
    println!("   {error}");
  }
  assert!(errors.is_empty(), "{} errors", errors.len());
}

/// Typed sources: what the verdicts are, and that the bytes are the same.
#[test]
fn slices_and_tails_to_bytes() {
  let v = const { hook::verdict::<[bool], [u8]>() };
  assert!(v.transmute_ref && !v.transmute_mut && !v.try_transmute_mut);
  assert!(const { hook::verdict::<[u16], [u8]>() }.transmute_mut);
  assert!(const { hook::verdict::<str, [u8]>() }.transmute_ref);
  assert!(!const { hook::verdict::<str, [u8]>() }.transmute_mut);
  assert!(!const { hook::verdict::<[u8], str>() }.transmute_ref);
  // `TailU8` has trailing padding when its slice has an odd length: none of it is seen as bytes.
  let words = [0x0102u16, 0x0304, 0x0506];
  let bytes: &[u8; 6] = transmutant::transmute_ref(&words);
  for len in [2usize, 3, 4, 5] {
    let tail: &TailU8 = transmutant::try_transmute_ref::<[u8], TailU8>(&bytes[..len])
      .unwrap_or_else(|e| {
        // An odd number of bytes after the head leaves no room for the padding.
        assert!(len % 2 == 1 && e == Error::Size, "{len}: {e:?}");
        transmutant::try_transmute_ref::<[u8], TailU8>(&bytes[..len - 1]).unwrap()
      });
    let back: &[u8] = transmutant::as_bytes(tail);
    assert_eq!(back, &bytes[..2 + tail.t.len()]);
  }
  // A `str` seen as `[u8in<0, 0x7f>]` only if it is ASCII; and back as `str` without a check.
  let ascii = transmutant::try_transmute_ref::<str, [u8in<0, 0x7f>]>("ab").unwrap();
  assert!(const { hook::verdict::<[u8in<0, 0x7f>], str>() }.transmute_ref);
  assert_eq!(transmutant::transmute_ref::<_, str>(ascii), "ab");
  assert!(transmutant::try_transmute_ref::<str, [u8in<0, 0x7f>]>("é").is_err());
  // Slices of different strides: the length follows.
  let pairs: &[[u8; 2]] = transmutant::transmute_ref(&words[..]);
  assert_eq!(pairs.len(), 3);
  let back: &[u16] = transmutant::try_transmute_ref::<[[u8; 2]], [u16]>(pairs).unwrap();
  assert_eq!(back, &words);
}
