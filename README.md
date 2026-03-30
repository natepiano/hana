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

## Anti-Aliasing

bevy_diegetic uses Order Independent Transparency (OIT) for correct layering
of coplanar panel elements. This has implications for anti-aliasing.

### ⛔ MSAA must be disabled

> **⚠️ CRITICAL: You must set `Msaa::Off` on any camera that renders diegetic
> panels. Bevy will panic if MSAA > 1 is active alongside OIT.**

Bevy's default camera includes `Msaa::Sample4`. If you use Geometry mode
panels (the default), the library enables OIT on your camera — and MSAA + OIT
causes an immediate panic.

```rust
commands.spawn((
    Camera3d::default(),
    Msaa::Off, // Required — OIT panics with MSAA > 1
    // ...
));
```

MSAA would not help regardless — it only smooths geometric triangle edges,
not the shader-computed SDF and MSDF boundaries that define panel corners,
borders, and text.

### TAA is optional but recommended

The SDF and MSDF shaders use `fwidth`-based anti-aliasing that produces
clean edges without any post-process AA. Panels look good with no AA at all.

That said, Temporal Anti-Aliasing smooths everything — geometric edges, SDF
panel boundaries, MSDF text edges, and specular aliasing — with minimal GPU
cost. It works correctly with OIT and provides a visible improvement at
extreme viewing angles.

```rust
commands.spawn((
    Camera3d::default(),
    Msaa::Off,
    bevy::anti_alias::taa::TemporalAntiAliasing::default(),
    // ...
));
```

| | MSAA | TAA |
|---|---|---|
| SDF panel edges | No effect | Smoothed |
| MSDF text edges | No effect | Smoothed |
| OIT compatible | **No (panics)** | Yes |
| GPU cost | High (2-8x framebuffer) | Low (history buffer + resolve) |
| Tradeoff | — | Slight ghosting on fast camera motion |

All examples include TAA and `Msaa::Off`. The `panel_rendering` example lets
you toggle TAA with the `T` key — zoom into a panel edge at an angle to see
the difference.

## Bevy compatibility

| bevy_diegetic | Bevy  |
|---------------|-------|
| main          | 0.18  |

## License

MIT OR Apache-2.0
