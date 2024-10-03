# mutant

***Efficient, safe, transmutation. Even for dynamically-sized types.***

## Example

Here's a simple example demonstrating how to use mutant:

```rust
use mutant::{Data, num::u16le};

#[derive(Data, Eq, PartialEq, Debug)]
#[repr(C)]
struct Loot<T: ?Sized> {
  position: (u8, u8),
  player: u16le,
  items: T,
}

#[derive(Copy, Clone, Data, Eq, PartialEq, Debug)]
#[repr(C)]
struct Item {
  archetype: u8,
  legendary: bool,
}

// Create a `Loot` instance with some items.
let mut loot = Loot {
  position: (21, 12),
  player: 0xcafe.into(),
  items: [
    Item { archetype: 0x42, legendary: true },
    Item { archetype: 0x21, legendary: false },
  ]
};

#[derive(Copy, Clone, Data, Eq, PartialEq, Debug)]
#[repr(C)]
struct Position { x: u8, y: u8 }

// Transmute the `position` field to a `Position` struct.
assert_eq!(&Position { x: 21, y: 12 }, mutant::transmute_ref(&loot.position));
assert_eq!(Position { x: 21, y: 12 }, mutant::transmute(loot.position)); // Position is `Copy`

// Modify the `position` field in place.
let mut position: &mut Position = mutant::transmute_mut(&mut loot.position);
position.x *= 2;

// Transmute the entire `Loot` struct to bytes.
let loot: &Loot<[Item]> = &loot;
let loot_bytes: &[u8] = mutant::transmute_ref(loot);
assert_eq!(
  &[21 * 2, 12, 0xfe, 0xca, 0x42, 1, 0x21, 0],
  loot_bytes
);

// This fail to compile as it requires runtime checks.
// let loot2: &Loot<[Item]> = mutant::transmute_ref(loot_bytes);

// Transmute the bytes back to a `Loot` struct.
let loot2: Result<&Loot<[Item]>, _> = mutant::try_transmute_ref(loot_bytes);
assert_eq!(Ok(loot), loot2);
```

Transmute is checked at compile time:
```rust,compile_fail
let _: &mut u8 = mutant::transmute_mut(&mut true);
```

## Naming

#### DST
According to the [Rust reference]:
> *Most types have a fixed size that is known at compile time and implement the trait [`Sized`].
> A type with a size that is known only at run-time is called a dynamically sized type (DST)
> or, informally, an unsized type. [Slices] and [trait objects] are two examples of DSTs.*

Where `mutant` only support [Slices], structs with a [tail slice](#tail-slice), and
[maybe DST](#maybe-dst) struct types, but not [trait objects]. Enums can't be DST are they
aren't allowed to contain any DST, i.e. enums are always [`Sized`] where struct may not.

[Rust reference]: https://doc.rust-lang.org/reference/dynamically-sized-types.html
[Slices]: https://doc.rust-lang.org/reference/types/slice.html
[trait objects]: https://doc.rust-lang.org/reference/types/trait-object.html

##### Tail slice
The following `TailSliceDst` struct is a DST because the size of `data` is only known at
runtime, hence not [`Sized`]:
```rust
struct TailSliceDst {
  info: u32,
  data: [u8], // `tail slice` of type `[u8]`.
}
```

**But** according to the [Rust reference]:
> *Structs can actually store a single DST directly as their last field, but this makes them a
> DST as well.*

The following `TailDst` struct is also a DST, **but** with a [tail](#tail) DST instead:
```rust
struct TailSliceDst {
  info: u32,
  data: [u8], // `tail slice` of type `[u8]`.
}

struct TailDst {
  version: u32,
  payload: TailSliceDst, // `tail` DST of type `TailSliceDst`.
}
```

In `mutant`, the [tail slice](#tail-slice) term **always** refer to the **last slice** of a
[DST](#dst):
- The [tail slice](#tail-slice) type of `TailSliceDst` is `[u8]`.
- The [tail slice](#tail-slice) type of `TailDst` is **also** `[u8]`.

##### Maybe DST
The relaxed [`?Sized`](Sized) bound allow for types that are maybe [DST](#dst):
```rust
struct MaybeDst<T: ?Sized> {
  info: u32,
  data: T, // `tail` of type `T`.
}

type Dst = MaybeDst<[u8]>; // Is a `DST`.
type NotDst = MaybeDst<u32>; // Not a `DST`.
```

#### Tail
In `mutant`, the [tail](#tail) term **always** refer to **last field** of a struct, **either for
[DST](#dst) and [`Sized`]**.
As per the examples above in the [tail slice](#tail-slice) section:
- The [tail](#tail) of `TailSliceDst` is slice of type `[u8]`.
- The [tail](#tail) of `TailDst` is a [DST](#dst) of type `TailSliceDst`.

