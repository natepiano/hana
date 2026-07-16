<div align="center">

<img src="assets/kana.svg" alt="仮名" width="260"/>

# bevy_kana

**Ergonomic, opinionated utilities for Bevy — type-safe math, input wiring, and more.**

[![CI](https://github.com/natepiano/bevy_kana/actions/workflows/ci.yml/badge.svg)](https://github.com/natepiano/bevy_kana/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/bevy_kana.svg)](https://crates.io/crates/bevy_kana)
[![docs.rs](https://docs.rs/bevy_kana/badge.svg)](https://docs.rs/bevy_kana)
[![license](https://img.shields.io/crates/l/bevy_kana.svg)](LICENSE-MIT)

</div>

---

> **Work in progress.** This crate is in active development (v0.1.0) and not
> subject to semver stability guarantees. APIs will change without notice
> between commits. Do not depend on this in production code yet.

**仮名** (*kana*) — from Japanese 仮 (*ka*, "simplified, borrowed") + 名 (*na*, "name, character"). The kana writing systems — hiragana (ひらがな) and katakana (カタカナ) — were born as simplified characters borrowed from complex kanji, making written language more accessible without losing meaning.

`bevy_kana` follows the same philosophy: small, named abstractions borrowed from Bevy's existing types, making game code more expressive and type-safe without adding complexity. It is a growing collection of ergonomic utilities — not limited to any single category.

## What's in the box

### Semantic math types

Zero-cost newtype wrappers around Bevy's math primitives that prevent accidental mixing at compile time.

| Type | Wraps | Purpose |
|------|-------|---------|
| `Position` | `Vec3` | A point in 3D space |
| `Displacement` | `Vec3` | A delta or offset |
| `Velocity` | `Vec3` | Rate of position change |
| `ScreenPosition` | `Vec2` | Pixel-space coordinates |
| `Orientation` | `Quat` | A rotation |

**Key properties:**

- **`Deref`** to the inner type — access `.x`, `.length()`, `.dot()` directly
- **`From`/`Into`** both directions — easy interop with Bevy APIs
- **Type-safe arithmetic** — `Position + Position` works, `Position + Velocity` won't compile
- **`Reflect`** support — compatible with Bevy's reflection and inspector tools

```rust
use bevy::math::Vec3;
use bevy_kana::Position;
use bevy_kana::Velocity;

let start_position = Position(Vec3::new(1.0, 0.0, 0.0));
let end_position = Position(Vec3::new(3.0, 0.0, 0.0));

// Same-type arithmetic works
let centroid = (start_position + end_position) / 2.0;

// Cross-type mixing is a compile error
// let bad = start_position + Velocity(Vec3::X); // ERROR
```

### Numeric cast traits

Convenience traits that replace bare `as` casts for common numeric conversions, centralizing the clippy `#[allow]` so call sites stay clean.

| Trait | From | Suppresses |
|-------|------|------------|
| `ToU8` | `f32`, `u32`, `usize` | `cast_possible_truncation`, `cast_sign_loss` |
| `ToU16` | `usize`, `u32`, `f32` | `cast_possible_truncation`, `cast_sign_loss` |
| `ToF32` | `i32`, `u32`, `usize`, `f64` | `cast_precision_loss`, `cast_possible_truncation` |
| `ToI32` | `usize`, `u32`, `f32`, `f64` | `cast_possible_truncation`, `cast_possible_wrap` |
| `ToU32` | `usize`, `i32`, `f32`, `f64`, `u64` | `cast_possible_truncation`, `cast_sign_loss` |
| `ToUsize` | `u32`, `f32` | `cast_possible_truncation`, `cast_sign_loss` |
| `ToF64` | `usize`, `u32`, `i32`, `f32`, `u64` | `cast_precision_loss` |

**These conversions are deliberately lossy.** They will silently produce wrong results if the input exceeds the target type's representable range. It is the caller's responsibility to ensure values are in bounds. Typical safe usage: loop indices, mesh vertex counts, and other small geometry values.

```rust
use bevy_kana::ToF32;
use bevy_kana::ToU32;

let sides: u32 = 8;
let angle = (j.to_f32() / sides.to_f32()) * std::f32::consts::TAU;
let index = positions.len().to_u32();
```

### Shared cascades

`Cascade<T>` represents an authored value that either inherits from the next
lower-precedence scope or overrides it. Ordinary structs can resolve authored
layers without ECS storage:

```rust
use bevy_kana::Cascade;
use bevy_kana::resolve_cascade;

let member = Cascade::Inherit;
let stage = Cascade::Override(0.25_f32);
let sequence_default = 1.0;

assert_eq!(
    resolve_cascade([member, stage], sequence_default),
    0.25,
);
```

Resolution examines layers from highest to lowest precedence and uses the first
override. A required root value completes the cascade. There are deliberately
no conversions to or from `Option<T>`: `Inherit` is explicit authored state,
not a generic missing value.

For ECS attributes, `CascadePlugin<A>` propagates the same authored component
over an explicit `CascadeFrom` relationship.

```rust
use bevy::prelude::*;
use bevy_kana::Cascade;
use bevy_kana::CascadeEntityCommandsExt;
use bevy_kana::CascadeFrom;
use bevy_kana::CascadePlugin;

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
struct Opacity(f32);

fn register(app: &mut App) {
    app.add_plugins(CascadePlugin::new(Opacity(1.0)));
}

fn author(commands: &mut Commands, parent: Entity, child: Entity) {
    commands.entity(parent).override_cascade(Opacity(0.5));
    commands
        .entity(child)
        .insert(CascadeFrom::new(parent))
        .set_cascade(Cascade::<Opacity>::Inherit);
}
```

The engine maintains `Resolved<A>` only on entities carrying `Cascade<A>`.
It handles root-default changes, local authoring changes, relationship
retargeting and removal, participant removal, multi-level propagation, cycle
and depth termination, and change-guarded cache writes.

Run the interactive generic cascade example:

```bash
cargo run --example cascade
```

### More to come

`bevy_kana` will grow to include other convenience macros and generic utilities that are broadly useful across Bevy projects.

## Version Compatibility

| bevy_kana | Bevy |
|-----------|------|
| 0.1.0 | 0.19 |
| 0.0.6 | 0.18 |

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
bevy_kana = "0.1.0"
```

Run the example:

```bash
cargo run --example basics
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
