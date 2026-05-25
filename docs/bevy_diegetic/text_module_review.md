# Text module restructure plan

The text module should read as two internal domains behind the existing `text` facade: font infrastructure for asset loading, registration, and measurement, and Slug infrastructure for glyph preparation, runtime storage, and GPU render data. The public crate-root API stays unchanged unless a later public-API review approves narrowing it.

## Phase overview

| Phase | What | Risk | Rough size |
|-------|------|------|------------|
| 1 | Placement - group font and Slug internals into directories, then tighten internal facades | Medium | about 35 import and call-site updates, one commit |

## Phase 1 - Placement

### Proposed layout

```text
crates/bevy_diegetic/src/text/
  mod.rs
  font/
    mod.rs
    constants.rs
    loader.rs
    measurer.rs
    registry.rs
  slug/
    mod.rs
    glyph/
      mod.rs
      outline.rs
      packing.rs
    render/
      mod.rs
      constants.rs
      material.rs
      run_data.rs
    runtime/
      mod.rs
      backend.rs
      input.rs
      run.rs
    support/
      mod.rs
      fixtures.rs
```

This tree leaves no above-budget singleton layer. `text/` has only its plugin root; `text/slug/` has only responsibility directories; `font/`, `glyph/`, `render/`, `runtime/`, and `support/` each stay under the six-singleton budget.

### Moves, with rationale

#### `text/font/`

Move the font infrastructure into one directory:

- `text/font.rs` -> `text/font/mod.rs`
- `text/constants.rs` -> `text/font/constants.rs`
- `text/font_loader.rs` -> `text/font/loader.rs`
- `text/font_registry.rs` -> `text/font/registry.rs`
- `text/measurer.rs` -> `text/font/measurer.rs`

These files all serve font assets, font registration, and text measurement. `Font` remains in `font/mod.rs` because the module is named for the primary `Font` type. `FontMetrics`, `GlyphBounds`, and `GlyphTypographyMetrics` stay with `Font` because they are returned only by `Font` methods and do not have independent behavior.

`TextPlugin` stays in `text/mod.rs`. It is the Bevy plugin root registered by `lib.rs`, so it should keep the app wiring while `text/font/` owns the font-specific types and helpers.

#### `text/slug/glyph/`

Move glyph outline and packing code into one Slug glyph directory:

- `text/slug/geometry.rs` -> `text/slug/glyph/outline.rs`
- `text/slug/packing.rs` -> `text/slug/glyph/packing.rs`

`outline.rs` owns `QuadraticSegment`, `SlugBounds`, `SlugContour`, `SlugGlyph`, `SlugOutlineError`, and glyph-outline loading from `ttf_parser`. `packing.rs` owns `SlugCurveRecord`, `SlugBandRecord`, `SlugGlyphRecord`, `SlugPackedGlyph`, `DEFAULT_BAND_COUNT`, and `build_packed_glyph`. Keeping them under `glyph/` makes the outline-to-packed-glyph pipeline local without mixing it with runtime storage or materials.

#### `text/slug/runtime/`

Move runtime cache and prepared-run state into one directory:

- `text/slug/backend.rs` -> `text/slug/runtime/backend.rs`
- `text/slug/run.rs` -> `text/slug/runtime/run.rs`
- add `text/slug/runtime/input.rs`

`backend.rs` owns `SlugBackend`, `SlugPreparedTextRun`, `SlugRunStorageKey`, and `SlugRunStorage`. `run.rs` owns `SlugFontKey`, `SlugGlyphKey`, `SlugGlyphInstance`, `SlugTextRun`, `SlugBuiltTextRun`, and `SlugGlyphCache`.

`input.rs` should own a text-side positioned glyph input, for example `SlugPositionedGlyph<'a> { glyph: &'a ShapedGlyph, font: ResolvedFontData<'a> }`. This removes the current reverse dependency where `text/slug/backend.rs` imports `crate::render::PositionedGlyph`. Render code should create `SlugPositionedGlyph` values and pass those into `SlugBackend`.

#### `text/slug/render/`

Move Slug GPU material and run render data into one render-data directory:

- `text/slug/constants.rs` -> `text/slug/render/constants.rs`
- `text/slug/material.rs` -> `text/slug/render/material.rs`
- `text/slug/run_render.rs` -> `text/slug/render/run_data.rs`

`material.rs` owns `SlugRenderMode`, `SlugTextMaterial`, `SlugTextMaterialInput`, and `slug_text_material`. `run_data.rs` owns `SlugRunRenderData`, `SlugRunRenderError`, `build_slug_run_render_data_with_clip`, `RunPacker`, `RunMeshBuilder`, and `GlyphQuadExtents`.

This keeps the existing text facade stable for render callers while separating GPU material/run-data code from glyph extraction and runtime cache state.

`slug/mod.rs` should also define `SlugPlugin`, register `SlugBackend`, embed `shaders/slug_text.wgsl`, and add `MaterialPlugin::<SlugTextMaterial>`. The shader path changes because the `embedded_asset!` call moves from `text/mod.rs` to `text/slug/mod.rs`. `TextPlugin` should add `SlugPlugin` instead of owning Slug registration directly.

#### `text/slug/support/`

Move shared Slug test fixtures into a test-only support directory:

- `text/slug/test_support.rs` -> `text/slug/support/fixtures.rs`

Use `#[cfg(test)]` on `support/` and update in-module tests to import `crate::text::slug::support::{fixture_run_with_cache, prepare_fixture_run}`. Re-export only those shared helpers from `support/mod.rs`; keep the fixture module itself private. Keep test modules with the production files they test; only shared fixture construction moves here.

### What stays where

`text/mod.rs` stays as the plugin and facade. It should declare only `mod font; mod slug;`, expose the public text API through `pub use`, expose crate-internal Slug and font items through `pub(crate) use`, and keep `TextPlugin` after the module table of contents.

`text/slug/mod.rs` stays as the Slug facade and plugin root. It should declare `glyph`, `render`, `runtime`, and test-only `support`, re-export only the crate-internal Slug items currently consumed by render and panel systems, and place `SlugPlugin` after the module table of contents.

The crate-root exports in `lib.rs` stay unchanged. Sibling workspace crates currently import text API only through `bevy_diegetic` crate-root exports, and examples/benches rely on several of those names. Any public narrowing needs explicit approval.

### Module re-exports

`text/mod.rs`:

```rust
mod font;
mod slug;

pub(crate) use font::DEFAULT_FAMILY;
pub use font::DiegeticTextMeasurer;
pub use font::Font;
pub use font::FontId;
pub use font::FontLoadFailed;
pub use font::FontMetrics;
pub use font::FontRegistered;
pub use font::FontRegistry;
pub use font::FontSource;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphBounds;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphTypographyMetrics;
pub use font::create_parley_measurer;
pub(crate) use font::ResolvedFontData;
pub(crate) use slug::DEFAULT_BAND_COUNT;
pub(crate) use slug::SlugBackend;
pub(crate) use slug::SlugPositionedGlyph;
pub(crate) use slug::SlugPreparedTextRun;
pub(crate) use slug::SlugRenderMode;
pub(crate) use slug::SlugRunStorage;
pub(crate) use slug::SlugRunStorageKey;
pub(crate) use slug::SlugTextMaterial;
pub(crate) use slug::SlugTextMaterialInput;
pub(crate) use slug::slug_text_material;

use self::font::FontLoader;
use self::slug::SlugPlugin;
```

`text/font/mod.rs`:

```rust
mod constants;
mod loader;
mod measurer;
mod registry;

pub(crate) use constants::DEFAULT_FAMILY;
pub(super) use loader::FontLoader;
pub use measurer::DiegeticTextMeasurer;
pub use measurer::create_parley_measurer;
pub use registry::FontId;
pub use registry::FontLoadFailed;
pub use registry::FontRegistered;
pub use registry::FontRegistry;
pub use registry::FontSource;
pub(crate) use registry::ResolvedFontData;
```

`text/slug/mod.rs`:

```rust
mod glyph;
mod render;
mod runtime;
#[cfg(test)]
pub(crate) mod support;

pub(crate) use glyph::DEFAULT_BAND_COUNT;
pub(crate) use render::SlugRenderMode;
pub(crate) use render::SlugTextMaterial;
pub(crate) use render::SlugTextMaterialInput;
pub(crate) use render::slug_text_material;
pub(crate) use runtime::SlugBackend;
pub(crate) use runtime::SlugPositionedGlyph;
pub(crate) use runtime::SlugPreparedTextRun;
pub(crate) use runtime::SlugRunStorage;
pub(crate) use runtime::SlugRunStorageKey;

pub(super) struct SlugPlugin;
```

`text/slug/glyph/mod.rs`:

```rust
mod outline;
mod packing;

pub(super) use outline::QuadraticSegment;
pub(super) use outline::SlugBounds;
pub(super) use outline::SlugContour;
pub(super) use outline::SlugGlyph;
pub(super) use outline::SlugOutlineError;
pub(super) use outline::glyph_id_has_visible_outline;
pub(super) use outline::load_glyph_by_id_from_face;
pub(crate) use packing::DEFAULT_BAND_COUNT;
pub(super) use packing::SlugBandRecord;
pub(super) use packing::SlugCurveRecord;
pub(super) use packing::SlugGlyphRecord;
pub(super) use packing::SlugPackedGlyph;
pub(super) use packing::build_packed_glyph;
```

`text/slug/runtime/mod.rs`:

```rust
mod backend;
mod input;
mod run;

pub(crate) use backend::SlugBackend;
pub(crate) use backend::SlugPreparedTextRun;
pub(crate) use backend::SlugRunStorage;
pub(crate) use backend::SlugRunStorageKey;
pub(crate) use input::SlugPositionedGlyph;
pub(super) use run::SlugBuiltTextRun;
pub(super) use run::SlugFontKey;
pub(super) use run::SlugGlyphCache;
pub(super) use run::SlugGlyphInstance;
pub(super) use run::SlugGlyphKey;
pub(super) use run::SlugTextRun;
```

`text/slug/render/mod.rs`:

```rust
mod constants;
mod material;
mod run_data;

pub(crate) use material::SlugRenderMode;
pub(crate) use material::SlugTextMaterial;
pub(crate) use material::SlugTextMaterialInput;
pub(crate) use material::slug_text_material;
pub(super) use run_data::SlugRunRenderError;
pub(super) use run_data::build_slug_run_render_data_with_clip;
```

The `pub(crate)` child-facade exports above are the items re-exported by `slug/mod.rs` and then by `text/mod.rs`. Keep non-facade helpers at `pub(super)`.

`text/slug/support/mod.rs`:

```rust
mod fixtures;

pub(crate) use fixtures::fixture_run_with_cache;
pub(crate) use fixtures::prepare_fixture_run;
```

### Sequencing

1. Add `text/slug/runtime/mod.rs` with `mod input; pub(crate) use input::SlugPositionedGlyph;`, declare `mod runtime;` from `text/slug/mod.rs`, add `SlugPositionedGlyph` under `text/slug/runtime/input.rs`, and re-export it through the existing `slug` and `text` facades. Update `render/text_shaping.rs` to return that type, update `text/slug/test_support.rs` to construct it, remove the old `render::PositionedGlyph` re-export, and update `SlugBackend::prepare_positioned_run*` plus world/panel shaping helpers to take `&[SlugPositionedGlyph<'_>]`. Checkpoint: `cargo build -p bevy_diegetic` and `cargo nextest run -p bevy_diegetic`.
2. Add narrow accessors on `SlugPreparedTextRun`, at minimum `glyph_count()` and `storage_key()`, then replace render call sites that read `prepared.run.run.glyphs().len()` or `prepared.storage_key` directly. Make nested fields private after the call sites are updated. Checkpoint: `cargo build -p bevy_diegetic` and `cargo nextest run -p bevy_diegetic`.
3. Move `text/{constants,font_loader,font_registry,measurer}.rs` into `text/font/`, move `text/font.rs` to `text/font/mod.rs`, add the `text/font/mod.rs` re-export block, and update imports from `super::constants` or `super::font` to `super::constants`/`super::Font` relative to the new parent. Checkpoint: `cargo build -p bevy_diegetic` and `cargo nextest run -p bevy_diegetic`.
4. Move Slug glyph files into `text/slug/glyph/`, move runtime files into `text/slug/runtime/`, move render files into `text/slug/render/`, and move fixture support into `text/slug/support/fixtures.rs`. Add the new `mod.rs` files and update sibling imports to use the new facades. Checkpoint: `cargo build -p bevy_diegetic` and `cargo nextest run -p bevy_diegetic`.
5. Move Slug registration from `TextPlugin` into `SlugPlugin` in `text/slug/mod.rs`; `TextPlugin` should call `app.add_plugins(SlugPlugin)` before the font-registry setup that may early-return on an embedded-font parse failure. Checkpoint: `cargo build -p bevy_diegetic` and `cargo nextest run -p bevy_diegetic`.
6. Remove any obsolete old module declarations and direct deep imports, then run `cargo +nightly fmt --all`, `cargo build -p bevy_diegetic`, and `cargo nextest run -p bevy_diegetic`. The whole phase lands as one commit after these checks pass.

Run the Cargo commands outside the sandbox in this environment.

## Deferred findings

- Minor: `text/slug/runtime/run.rs` still contains both cache identity/storage and positioned-run data. This is acceptable for the placement phase because it remains under 500 lines, but a later focused cleanup can split cache key/cache types into `runtime/glyph_cache.rs` if that file keeps growing.
- Minor: `text/slug/render/run_data.rs` still contains both run buffer packing and mesh quad building. This is acceptable for the placement phase because the production body is about 273 lines, but a later focused cleanup can split it into `run_packer.rs` and `run_mesh_builder.rs` if the renderer grows.
- Minor: `text/measurer.rs` currently imports `crate::FontSlant` through the public crate facade. During the font move, switch it to `crate::layout::FontSlant` so internal code does not depend on public re-exports.

## Pass 2 - over-large file check

No over-large files were found. The largest production-line counts are below the `when-to-split-a-module.md` threshold: `geometry.rs` 328, `packing.rs` 307, `font.rs` 304, `run_render.rs` 273 before tests, and `font_registry.rs` 252. No follow-on split phase is needed.
