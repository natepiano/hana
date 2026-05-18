# NonZeroSize for bevy_diegetic

Exploration of whether `bevy_diegetic` could have a `NonZeroSize` type analogous
to std's `NonZeroUsize` / `NonNull`.

## The std prior art

- `NonZeroUsize`, `NonZeroU32`, etc. work because integers have a *niche*: the
  representation `0` is reserved, so the compiler can pack `Option<NonZeroU32>`
  into the same 4 bytes as `u32` (the `None` variant uses the `0` bit pattern).
- `NonNull<T>` does the same for pointers (null is the niche).
- Std deliberately does **not** ship `NonZeroF32` / `NonZeroF64`. Floats have no
  clean niche — NaN, ±0.0, and subnormals all complicate the representation, and
  there's no single bit pattern the compiler can claim as "the None slot".

## Why this matters here

`bevy_diegetic`'s sizes are `f32`, carried inside `Dimension`:

```rust
// crates/bevy_diegetic/src/layout/sizing.rs
pub struct Dimension {
    pub value: f32,
    pub unit:  Option<Unit>,
}
```

So a `NonZeroSize` built on top of `Dimension` cannot get the
`Option<NonZeroSize>` == `sizeof(Dimension)` packing that `NonZeroUsize` gives
you. The only benefit available is **type-level validation** — "this value was
checked to be nonzero (or strictly positive) at construction."

## Two viable approaches

### 1. Validating newtype on `Dimension` (recommended)

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NonZeroSize(Dimension);

impl NonZeroSize {
    pub fn new(d: Dimension) -> Option<Self> {
        (d.value > 0.0 && d.value.is_finite()).then_some(Self(d))
    }

    /// # Safety
    /// Caller asserts `d.value > 0.0 && d.value.is_finite()`.
    pub const unsafe fn new_unchecked(d: Dimension) -> Self { Self(d) }

    pub const fn get(self) -> Dimension { self.0 }
    pub const fn value(self) -> f32 { self.0.value }
}
```

- Cheap, no niche, `Option<NonZeroSize>` is larger than `NonZeroSize` by one
  discriminant byte (plus padding).
- Invariant choice: **positive and finite** is more useful than literal "nonzero"
  for layout — it rules out `0.0`, negatives, `NaN`, `±inf` in one check. The
  existing `Sizing` enum already uses `f32::INFINITY` and `f32::MAX` as
  sentinels, which `NonZeroSize` would correctly reject if you want strict
  positivity. If you want to allow `f32::MAX` / `INFINITY` (as `Sizing::Fit.max`
  does), drop the `is_finite()` check.

### 2. Integer-backed (gets the niche, costs precision)

Store dimensions as `NonZeroU32` of micro-points (e.g. `1 unit = 1/1000 pt`).
Then `Option<NonZeroSize>` is the same size as `u32`.

This is a much larger change — `Dimension` is `f32` throughout the layout
engine (`sizing.rs`, `engine/sizing.rs`, `engine/positioning.rs`, etc.) and
mixing the two would force conversions at every boundary. Not worth it unless
there's a memory-density goal that doesn't currently exist.

## Where it would slot in

If we go with approach 1, the candidate call sites are:

- `Sizing::Fixed(Dimension)` → `Sizing::Fixed(NonZeroSize)` — a fixed size of
  zero is almost certainly a bug.
- `Sizing::fixed(size)` constructor — currently accepts any `Into<Dimension>`;
  would either return `Option<Sizing>` or panic on zero.
- `min` field of `Grow { min, max }` — debatable. `min = 0.0` is a legitimate
  "no floor" value and is used as the default.
- `max` field — must keep `f32::INFINITY` / `f32::MAX` semantics, so probably
  not a fit.

The cleanest first target is `Sizing::Fixed`, since "fixed zero size" has no
sensible meaning.

## Open question

What is `NonZeroSize` actually guarding? Three different invariants are
plausible:

1. **nonzero** (`value != 0.0`) — rules out only `0.0`.
2. **strictly positive** (`value > 0.0`) — also rules out negatives.
3. **positive and finite** (`value > 0.0 && value.is_finite()`) — also rules
   out `NaN`, `±inf`.

Picking the right one depends on which call sites it's defending. The layout
engine appears to assume non-negative finite values most of the time, so (3) is
the safest default.

## Recommendation

Approach 1 with a **positive-and-finite** invariant, applied first to
`Sizing::Fixed` and the `Sizing::fixed()` constructor. Re-evaluate after seeing
how it affects the builder API in `panel/builder.rs` and `layout/builder.rs`.
