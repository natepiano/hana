# bevy_diegetic

Diegetic UI layout engine for Bevy -- in-world panels driven by a Clay-inspired layout algorithm.

> **Work in progress.** This crate is in active development (v0.0.1) and not
> subject to semver stability guarantees. APIs will change without notice
> between commits. Do not depend on this in production code yet.

## What it does

`bevy_diegetic` renders UI panels and text directly in 3D space as regular
Bevy entities -- no screen-space overlay, no separate UI camera. Text is
rendered with the slug glyph backend, which builds one mesh per text run
from quadratic Bézier contours and computes analytic per-pixel coverage in
the shader.

- **Retained-mode layout** inspired by [Clay](https://github.com/nicbarker/clay) -- build a `LayoutTree` once, recompute only when it changes
- **Slug text rendering** -- one mesh per run from Bézier contours with analytic per-pixel coverage, and physical font sizing
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

Slug renders anti-aliased glyph edges from per-pixel coverage. It emits one
mesh per text run and orders coplanar text with `depth_bias`, so blended text
composites correctly as the camera moves with a default `Camera3d`.

The `AlphaMode` you pick determines how that coverage is composited:

| Mode | Coverage AA? | Notes |
|---|---|---|
| `Blend` (default) | ✅ | Classic alpha compositing |
| `Premultiplied` | ✅ | Like Blend; can look better on some scenes |
| `Add` | ✅ | Coverage modulates additive contribution |
| `Multiply` | ✅ | Coverage modulates multiplicative contribution |
| `AlphaToCoverage` + MSAA | ✅ | Coverage → sub-pixel sample mask |
| `AlphaToCoverage` without MSAA | ❌ | Degrades to `Mask(0.5)` |
| `Mask(t)` | ❌ | Thresholds coverage to 0/1 — jagged edges |
| `Opaque` | ❌ | Ignores coverage — glyph rectangles |

Set the app-wide default via `CascadeDefaults::text_alpha`, per-panel via
`DiegeticPanel::text_alpha_mode`, or per-style via
`WorldTextStyle`/`LayoutTextStyle::with_alpha_mode`.

### Quick recipes

```rust
// Default — Blend, works out of the box with no camera config.
commands.spawn(Camera3d::default());
```

```rust
// MSAA scene — switch the app-wide default to AlphaToCoverage:
commands.insert_resource(CascadeDefaults {
    text_alpha: AlphaMode::AlphaToCoverage,
    ..default()
});
commands.spawn((Camera3d::default(), Msaa::Sample4));
```

```rust
// Mix modes per-style for creative effects:
let neon = LayoutTextStyle::new(Pt(24.0)).with_alpha_mode(AlphaMode::Add);
let tint = LayoutTextStyle::new(Pt(14.0)).with_alpha_mode(AlphaMode::Multiply);
```

See the `text_alpha` example for an interactive walkthrough.

### TAA is optional but recommended

The SDF panel shader's `fwidth`-based edges and slug's analytic per-pixel
coverage both produce clean edges without any post-process AA. Panels look
good with no AA at all.

That said, Temporal Anti-Aliasing smooths everything — geometric edges,
SDF panel boundaries, slug text edges, and specular aliasing — with
minimal GPU cost.

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
