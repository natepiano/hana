# Plan: Revive OIT / `StableTransparency` for diegetic text (bevy 0.19)

> **Archived 2026-06-07 — implemented.** Lived at the docs root; archived under
> bevy_diegetic since the `StableTransparency` marker and OIT activation live in
> `crates/bevy_diegetic/src/render/transparency.rs`, surfaced through fairy_dust's
> `.with_stable_transparency()` (builder/sprinkle.rs). Subsequent OIT history on
> bevy 0.19: the shared fragment pool was raised to
> `OIT_FRAGMENTS_PER_PIXEL_AVERAGE = 8.0` after close-up exhaustion artifacts,
> and a resize-crash investigation added then removed an in-process shader
> guard — see
> [`../investigation/oit-resize-crash-investigation.md`](../investigation/oit-resize-crash-investigation.md).
>
> **Updated 2026-06-30.** Screen-space overlay cameras now receive OIT as well
> as `Msaa::Off` while `StableTransparency` is active. This is required because
> `StandardMaterial::depth_bias` now advances once per `DrawZIndexRank`, not
> once per command. Screen-space panels therefore need the same per-command
> `oit_depth_offset` path as world panels for same-z-index SDF/text/shape
> ordering.

## Context

Coplanar diegetic world text (e.g. `typography.rs`) shows view-angle **color
shifts**: text is `AlphaMode::Blend`, and Blend transparency sorts view-dependently,
so at grazing angles the per-pixel composite order flips. OIT (order-independent
transparency) makes Blend stable **without** sacrificing slug's analytic coverage AA
— the only other order-independent option, `AlphaToCoverage`, re-coarsens the AA to
MSAA sample granularity, which is the aliasing we removed during the slug work.

The OIT/`StableTransparency` stack was retired in **f45cef9** on the rationale that
slug emits one mesh per run and orders via `depth_bias`. The shift persists, so we
revive the same **opt-in** `.with_stable_transparency()` API and keep the no-default
policy. The 2026-06-30 update changes the screen-space camera propagation: screen
cameras now receive OIT as well as `Msaa::Off` while the marker is active.

## Current behavior

- Call `.with_stable_transparency()` → that camera gets OIT + `Msaa::Off`; screen-space
  cameras on the window are matched to OIT + `Off`.
- Don't call it → no OIT, MSAA stays on (`Msaa::default()` everywhere).
- **bevy_diegetic stays opt-in.** The `StableTransparency` marker is the only
  thing that forces `Msaa::Off` or OIT onto screen-space overlay cameras. No
  blanket crate default.

| Setup | Main cam | Screen-space | Result |
|-------|----------|--------------|--------|
| `.with_stable_transparency()` | OIT, `Off` | OIT, forced `Off` | stable coplanar text and screen panels |
| not called, with panels | `Sample4` | stays `Sample4` | AA'd meshes + panels, no OIT |
| not called, no panels | `Sample4` | — | plain MSAA scene |

## Decisions locked in

- **Opt-in via `.with_stable_transparency()`**, exactly as before. Not a default.
- The **`StableTransparency` marker lives in bevy_diegetic**; fairy_dust is one producer.
- **All three observers are the current contract:**
  - `on_stable_transparency_added` — OIT on: insert OIT + `Msaa::Off` on self, propagate
    OIT + `Msaa::Off` to existing `ScreenSpaceCamera`s.
  - `on_screen_space_camera_added` — panel spawns while OIT already on: force OIT +
    `Msaa::Off` on the new screen-space camera (reverse spawn order).
  - `on_stable_transparency_removed` — OIT off: strip OIT, restore `Msaa::default()`
    everywhere. **Kept.**
- Screen-space camera spawn **keeps `Msaa::default()`** in its bundle; the observer
  overrides to `Off` only when OIT is present. Do **not** hardcode it `Off` — that would
  break the no-OIT MSAA-plus-panels case.
- Text must be `Blend`/`Premultiplied` (already the cascade default; `Opaque`/`Mask`
  bypass OIT).
- **Do NOT revert** f45cef9's cascade `Exclude`/`ExcludeNone` fix. HueOffset stays deleted.

## Work items

### A. bevy_diegetic — marker + observers (port to 0.19)
- **Recreate `src/render/transparency.rs`** (~138 lines, port imports):
  `bevy::core_pipeline::oit::OrderIndependentTransparencySettings`,
  `bevy::render::view::Msaa`, `bevy::camera::Camera3d`,
  `bevy::render::render_resource::TextureUsages`. All three observers as above.
  0.19 OIT WGSL/Rust API confirmed intact (`oit_draw(position, color)` under
  `#ifdef OIT_ENABLED`; the settings struct still ORs `TEXTURE_BINDING` into depth usage).
- **`src/render/mod.rs`**: re-add `mod transparency;`, `pub use transparency::StableTransparency;`,
  register the 3 observers in `RenderPlugin::build`, re-add `pub(crate) use constants::OIT_DEPTH_STEP;`.
- **`src/lib.rs`**: re-add `pub use render::StableTransparency;`.
- **`src/render/constants.rs`**: re-add `OIT_DEPTH_STEP: f32 = 0.000_001` (1e-6).

### B. bevy_diegetic — shader OIT path (the bulk of the work)
- **`src/text/slug/shaders/slug_text.wgsl`**: restore the `#ifdef OIT_ENABLED` block —
  `#import bevy_core_pipeline::oit::oit_draw`, apply `oit_depth_offset` to `position.z`,
  call `oit_draw(...)` + `discard` instead of returning the color in the main fragment.
  Must not disturb the committed g-seam functions (`winding_at`, `any_outside_neighbor`).
- **`src/shaders/sdf_panel.wgsl`**: restore the same OIT block.
- **`src/render/sdf_material.rs`**: re-thread the `oit_depth_offset` uniform into the
  material/extension (~39-line hunk in f45cef9).
- **`src/render/panel_geometry.rs`** + **`src/callouts/render.rs`**: re-thread
  `oit_depth_offset` into the draw commands.
- Rationale: pipeline `depth_bias` does NOT reach `in.position.z` under OIT, so
  `OIT_DEPTH_STEP` is the coplanar-ordering mechanism *inside* the OIT buffer.
- Pull exact hunks with `git show f45cef9 -- <file>` and reverse, adjusting for 0.19.

### C. fairy_dust — revive the opt-in builder method (as before)
- **Recreate `src/transparency.rs`**: `install(app)` ensures `DiegeticUiPlugin` and
  adds the `Add<FairyDustOrbitCam>` observer that inserts `StableTransparency`.
- **`src/lib.rs`**: re-add `mod transparency;` and the `.with_stable_transparency()`
  method on `SprinkleBuilder<WithOrbitCam>` (calls `transparency::install`).
- **`src/builder/camera_home.rs`**: re-add `with_stable_transparency()` on
  `CameraHomeBuilder<WithOrbitCam>`.
- **Example call sites**: re-add `.with_stable_transparency()` to the examples that had
  it (world_text, slug_text, typography, units).

### D. Documentation — the six dependencies
Rewrite the README "Text transparency" section to cover:
1. OIT ⟂ MSAA — both on one camera panics.
2. All cameras sharing a window must match MSAA — else macOS Metal swap-chain stall.
3. Screen-space cameras sharing the window need OIT too, not only `Msaa::Off`;
   same-z-index screen panels rely on `oit_depth_offset` after material
   `depth_bias` moved to `DrawZIndexRank`.
4. OIT needs the shader `oit_draw` blocks — the camera setting alone is inert.
5. `depth_bias` doesn't reach OIT fragments → manual `OIT_DEPTH_STEP` for coplanar order.
6. Text must be `Blend`/`Premultiplied` — `Opaque`/`Mask` bypass OIT.
Plus: for mesh-edge AA *with* OIT, use a post-process AA (FXAA/SMAA/TAA) — MSAA is the
one AA incompatible with OIT.

### E. MSAA-vs-OIT comparison example
Add an example with a **hand-coded camera spawn** that lets people see the difference
between the two paths:
- MSAA on, no `StableTransparency` → AA'd mesh edges, but coplanar text shifts at angles.
- MSAA off + `StableTransparency` (OIT) → stable coplanar text, aliased mesh edges.
Hand-coding the camera (not via fairy_dust) is the point — the contrast is the lesson.

### F. text_alpha example
Rework to demo blend modes **without** MSAA. Drop the `AlphaToCoverage` entry (degrades
to `Mask(0.5)` without MSAA) or annotate "needs MSAA". Under OIT, `Opaque`/`Mask` still
render in the normal passes; only `Blend`/`Premultiplied` route through `oit_draw`.

### G. RTT removal (separate step)
Delete `panel_rtt.rs`, its plugin registration, and any RTT panel API (no current use
case). Independent of the OIT work — can land before or after.

## Must preserve (do NOT undo from f45cef9)
- cascade `Exclude` / `ExcludeNone` machinery.
- HueOffset stays deleted; text_stress rework stays.

## Committed slug-quality work to keep intact (commit c3cfcbd)
- `packing.rs` `DEFAULT_BAND_COUNT = 96`.
- `slug_text.wgsl` dedup gate OFF + the g-seam silhouette/interior detection.
- The OIT `#ifdef` block restored in B must not disturb the g-seam functions.

## Verification
1. `typography.rs` with `.with_stable_transparency()` → orbit the coplanar ground text →
   color shift gone.
2. A scene with screen panels + OIT → screen panels receive OIT, composite correctly,
   and the window does NOT freeze.
3. Late-spawn: trigger a screen panel after startup with OIT on → no stall (exercises
   `on_screen_space_camera_added`) and the new screen-space camera has OIT + `Msaa::Off`.
4. A scene **without** `.with_stable_transparency()` + panels → still works (MSAA on,
   no stall) — confirms the opt-in didn't break the default path.
5. g-seam fix + 96-band slug quality still intact.
6. Comparison example (E) visibly shows the MSAA-vs-OIT trade.
