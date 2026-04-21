# bevy_diegetic

Diegetic UI layout engine for Bevy -- in-world panels driven by a Clay-inspired layout algorithm.

> **Work in progress.** This crate is in active development (v0.0.1) and not
> subject to semver stability guarantees. APIs will change without notice
> between commits. Do not depend on this in production code yet.

## What it does

`bevy_diegetic` renders UI panels and text directly in 3D space as regular
Bevy entities -- no screen-space overlay, no separate UI camera. Text is
rendered via MSDF (multi-channel signed distance field) atlas rasterization
with async on-demand glyph generation.

- **Retained-mode layout** inspired by [Clay](https://github.com/nicbarker/clay) -- build a `LayoutTree` once, recompute only when it changes
- **MSDF text rendering** with per-glyph async rasterization, multi-page atlas, and physical font sizing
- **OpenType features** -- ligatures, contextual alternates, discretionary ligatures, kerning
- **Multiple font families** -- load fonts via Bevy `AssetServer`, render with per-element font selection

## Quick start

```rust
use bevy::prelude::*;
use bevy_diegetic::*;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(DiegeticUiPlugin)
    .add_systems(Startup, setup)
    .run();
```

## Text transparency

MSDF text renders anti-aliased edges via fractional alpha computed from the
signed distance field. Two questions determine which `AlphaMode` you want:
**does it preserve the fractional alpha** (for smooth edges), and **is it
subject to depth-sort flicker** on coplanar text (where you'd want
`StableTransparency`).

### Preserves MSDF anti-aliasing?

| Mode | AA preserved? | Notes |
|---|---|---|
| `Blend` (default) | ✅ | Classic alpha compositing |
| `Premultiplied` | ✅ | Like Blend; can look better on some scenes |
| `Add` | ✅ | Alpha modulates additive contribution |
| `Multiply` | ✅ | Alpha modulates multiplicative contribution |
| `AlphaToCoverage` + MSAA | ✅ | Alpha → sub-pixel coverage mask |
| `AlphaToCoverage` without MSAA | ❌ | Degrades to `Mask(0.5)` |
| `Mask(t)` | ❌ | Thresholds alpha to 0/1 — jagged edges |
| `Opaque` | ❌ | Ignores alpha — glyph rectangles |

### Subject to depth-sort flicker?

In Bevy's transparent pass, any mode that writes alpha-blended fragments
can flicker on coplanar text as the camera moves — depth testing and
per-mesh back-to-front sort are not stable across all angles, even for
blend ops that are mathematically commutative like `Add`.

| Mode | Transparent queue? | `StableTransparency` helps? |
|---|---|---|
| `Blend`, `Premultiplied`, `Add`, `Multiply` | Yes | Yes — fixes flicker |
| `AlphaToCoverage` | No (opaque pipeline) | Not needed |
| `Mask(t)`, `Opaque` | No (opaque pipeline) | Not needed |

### The rule

- Want smooth text (any mode except `Mask`/`Opaque`) **and** seeing
  flicker on coplanar text? Add [`StableTransparency`] to your camera.
  Pair with `AlphaMode::Blend` (default) for best-looking text, or any
  other smooth mode for creative effects.
- Want MSAA in your scene? Use `AlphaMode::AlphaToCoverage` — the only
  anti-aliased path that bypasses the transparent queue and leaves MSAA
  intact.

**Hardware constraint:** `StableTransparency` enables Bevy's Order
Independent Transparency. Bevy's OIT plugin panics if a camera has OIT and
MSAA — *"MSAA is not supported when using OrderIndependentTransparency."*
`StableTransparency`'s observer forces `Msaa::Off`, so the two paths above
are mutually exclusive at the camera level.

### Quick recipes

```rust
// Default — works out of the box. Blend + no camera config.
// If coplanar text flickers, add StableTransparency:
commands.spawn((Camera3d::default(), StableTransparency));
```

```rust
// MSAA-friendly alternative. Switch the app-wide default to A2C:
commands.insert_resource(TextAlphaModeDefault(AlphaMode::AlphaToCoverage));
commands.spawn((Camera3d::default(), Msaa::Sample4));
```

```rust
// Mix modes per-style for creative effects:
let neon = LayoutTextStyle::new(Pt(24.0)).with_alpha_mode(AlphaMode::Add);
let tint = LayoutTextStyle::new(Pt(14.0)).with_alpha_mode(AlphaMode::Multiply);
```

See the `text_alpha` example for an interactive walkthrough.

### TAA is optional but recommended

The SDF and MSDF shaders use `fwidth`-based anti-aliasing that produces
clean edges without any post-process AA. Panels look good with no AA at
all.

That said, Temporal Anti-Aliasing smooths everything — geometric edges,
SDF panel boundaries, MSDF text edges, and specular aliasing — with
minimal GPU cost. It works with either transparency path above.

```rust
commands.spawn((
    Camera3d::default(),
    bevy::anti_alias::taa::TemporalAntiAliasing::default(),
    // ...
));
```

## Bevy compatibility

| bevy_diegetic | Bevy  |
|---------------|-------|
| main          | 0.18  |

## License

MIT OR Apache-2.0
