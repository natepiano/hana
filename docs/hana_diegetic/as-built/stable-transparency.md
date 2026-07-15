# OIT / `StableTransparency` for diegetic text

## Context

Coplanar diegetic world text (e.g. `typography.rs`) shows view-angle **color
shifts**: text is `AlphaMode::Blend`, and Blend transparency sorts view-dependently,
so at grazing angles the per-pixel composite order flips. OIT (order-independent
transparency) makes Blend stable **without** sacrificing slug's analytic coverage AA
— the only other order-independent option, `AlphaToCoverage`, re-coarsens the AA to
MSAA sample granularity, which is the aliasing removed during the slug work.

OIT is **opt-in** via `.with_stable_transparency()`; there is no crate default. Slug
emits one mesh per run and orders via `depth_bias`, but `depth_bias` does not reach
fragments under OIT, so coplanar ordering inside the OIT buffer runs through a
per-command `oit_depth_offset` (see below). Screen-space overlay cameras that share
the window receive OIT **and** `Msaa::Off` while the marker is active — screen panels
need the same per-command `oit_depth_offset` path as world panels for same-z-index
SDF/text/shape ordering, because material `depth_bias` advances once per
`DrawZIndexRank`, not once per command.

## Current behavior

- Call `.with_stable_transparency()` → that camera gets OIT + `Msaa::Off`; screen-space
  cameras on the window are matched to OIT + `Off`.
- Don't call it → no OIT, MSAA stays on (`Msaa::default()` everywhere).
- **hana_diegetic stays opt-in.** The `StableTransparency` marker is the only
  thing that forces `Msaa::Off` or OIT onto screen-space overlay cameras. No
  blanket crate default.

| Setup | Main cam | Screen-space | Result |
|-------|----------|--------------|--------|
| `.with_stable_transparency()` | OIT, `Off` | OIT, forced `Off` | stable coplanar text and screen panels |
| not called, with panels | `Sample4` | stays `Sample4` | AA'd meshes + panels, no OIT |
| not called, no panels | `Sample4` | — | plain MSAA scene |

## Contract

- **Opt-in via `.with_stable_transparency()`.** Not a default.
- The **`StableTransparency` marker lives in hana_diegetic**; fairy_dust is one producer.
- **Three observers form the contract:**
  - `on_stable_transparency_added` — OIT on: insert OIT + `Msaa::Off` on self, propagate
    OIT + `Msaa::Off` to existing `ScreenSpaceCamera`s.
  - `on_screen_space_camera_added` — panel spawns while OIT already on: force OIT +
    `Msaa::Off` on the new screen-space camera (reverse spawn order).
  - `on_stable_transparency_removed` — OIT off: strip OIT, restore `Msaa::default()`
    everywhere.
- Screen-space camera spawn **keeps `Msaa::default()`** in its bundle; the observer
  overrides to `Off` only when OIT is present. It is not hardcoded `Off` — that would
  break the no-OIT MSAA-plus-panels case.
- Text must be `Blend`/`Premultiplied` (the cascade default; `Opaque`/`Mask` bypass OIT).
- The parent-walking cascade model applies. HueOffset does not exist.

## Where it lives

- **`src/render/transparency.rs`** — the `StableTransparency` marker and all three
  observers. Uses `bevy::core_pipeline::oit::OrderIndependentTransparencySettings`,
  `bevy::render::view::Msaa`, `bevy::camera::Camera3d`,
  `bevy::render::render_resource::TextureUsages`. The settings struct ORs
  `TEXTURE_BINDING` into the camera depth-texture usage.
- **`src/render/mod.rs`** — `mod transparency;`, `pub use transparency::StableTransparency;`,
  and the three observers registered in `RenderPlugin::build`.
- **`src/lib.rs`** — `pub use render::StableTransparency;`.
- **`src/render/constants.rs`** — `OIT_DEPTH_STEP: f32 = 0.000_001` (1e-6), the coplanar
  step applied inside the OIT buffer.
- **Shader OIT path** — `src/render/analytic_paths/analytic_path.wgsl` (text/shape),
  `src/shaders/sdf_panel.wgsl` (panel fills), and `src/shaders/image_panel.wgsl`
  (batched images) each carry an `#ifdef OIT_ENABLED` block:
  `#import bevy_core_pipeline::oit::oit_draw`, add `oit_depth_offset` to `position.z`
  (floored at `OIT_MIN_DEPTH`), then `oit_draw(...)` + `discard` instead of returning the
  color. The analytic block must not disturb the winding/coverage functions
  (`n`, `lane_n`, `lanes_n`).
- **`oit_depth_offset` threading** — computed in `src/render/draw_order.rs` (from
  `DrawOrderIndex`, text-anchored) and threaded into draw commands / materials through
  `panel_geometry.rs`, `panel_text/*`, `panel_shapes/*`, `analytic_paths/*`, and
  `image_batch.rs`.
- **fairy_dust** — `src/transparency.rs` `install(app)` ensures `DiegeticUiPlugin` and
  adds the `Add<FairyDustOrbitCam>` observer that inserts `StableTransparency`; the
  `.with_stable_transparency()` method lives on `SprinkleBuilder<WithOrbitCam>`
  (`src/lib.rs`) and `CameraHomeBuilder<WithOrbitCam>` (`src/builder/camera_home.rs`).

## Text-transparency dependencies

1. OIT ⟂ MSAA — both on one camera panics.
2. All cameras sharing a window must match MSAA — else macOS Metal swap-chain stall
   (window only repaints on OS-level events).
3. Screen-space cameras sharing the window need OIT too, not only `Msaa::Off`;
   same-z-index screen panels rely on `oit_depth_offset` because material `depth_bias`
   advances per `DrawZIndexRank`, not per command.
4. OIT needs the shader `oit_draw` blocks — the camera setting alone is inert.
5. `depth_bias` does not reach OIT fragments → manual `OIT_DEPTH_STEP` for coplanar order.
6. Text must be `Blend`/`Premultiplied` — `Opaque`/`Mask` bypass OIT and render in the
   normal passes.

For mesh-edge AA *with* OIT, use a post-process AA (FXAA/SMAA/TAA) — MSAA is the one AA
incompatible with OIT.

## OIT fragment pool

Bevy's shared OIT fragment pool is sized at
`OIT_FRAGMENTS_PER_PIXEL_AVERAGE = 8.0`; fragments past the budget are discarded for the
frame (flashing black blocks). The default was too small for close-up diegetic scenes
that stack glyph quads, overlay boxes, and panels on most pixels. Details in
`src/render/transparency.rs`.

## Slug-quality invariants

- `render/analytic_paths/packing.rs` `DEFAULT_BAND_COUNT = 96`.
- The OIT `#ifdef OIT_ENABLED` block in `analytic_path.wgsl` must not disturb the
  winding/coverage functions (`n`, `lane_n`, `lanes_n`).

## Verifying it works

1. `typography.rs` with `.with_stable_transparency()` → orbit the coplanar ground text →
   no color shift.
2. A scene with screen panels + OIT → screen panels receive OIT, composite correctly,
   the window does not freeze.
3. Late-spawn: trigger a screen panel after startup with OIT on → no stall (exercises
   `on_screen_space_camera_added`); the new screen-space camera has OIT + `Msaa::Off`.
4. A scene **without** `.with_stable_transparency()` + panels → still works (MSAA on,
   no stall).
5. g-seam fix + 96-band slug quality intact.
