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

## Examples

```sh
cargo run --example world_text
cargo run --example text_panel
cargo run --example font_loading
cargo run --example font_features
cargo run --example typography --features typography_overlay
```

See the `examples/` directory for the full list.

## Bevy compatibility

| bevy_diegetic | Bevy  |
|---------------|-------|
| main          | 0.18  |

## License

MIT OR Apache-2.0
