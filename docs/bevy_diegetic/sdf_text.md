# SDF / MSDF Text Rendering Toggle

## Status

Implementation plan. Adds plain SDF (single-channel) as an alternative to
the current MSDF (multi-channel) text path, with a runtime toggle in the
typography example for A/B comparison.

## Motivation

The current MSDF pipeline rasterizes each glyph at a fixed canonical pixel
size into a 3-channel signed distance field. The shader takes the median
of the three channels per pixel; the median trick preserves sharp corners
that a single-channel SDF would round off.

The cost: on glyphs with thin tapered features at low canonical resolution,
two of three channels disagree about which side of the sub-pixel feature a
pixel is on, the median jumps abruptly, and the rendered silhouette shows
a pointy/wedge artifact instead of the actual curve. The EB Garamond `V`
ear tip is the canonical example — at canonical 64 it renders as a
single-pixel needle; at 128 it's a small wedge; at 256 it's mostly round
with a tiny corner visible only at extreme zoom.

Plain SDF stores one signed distance per pixel. No median, no channel
disagreement. Smooth curves stay smooth at any resolution. The trade-off
is that sharp corners (terminals, intersections, the inside angle of a
`V` apex) get rounded off — single-channel distance fields cannot
represent two intersecting edges meeting at a point.

For typography that's curve-heavy (Old Style serifs like EB Garamond,
italic scripts, hairline weights), plain SDF can render visibly closer to
the source outline. For typography with strong corners (sans-serif,
slab-serif, monospace), MSDF stays superior.

A per-atlas mode toggle lets each app pick the right trade for its font
family, and lets the typography overlay compare both renderings on the
same glyph.

## Non-goals

- Per-glyph mode selection within a single render pass. The toggle is
  per-atlas — all glyphs sampled from an atlas share a single mode at
  any moment.
- Per-font mode selection (e.g., serif → SDF, sans → MSDF
  simultaneously). Deferred. Most apps and the typography example are
  single-font. When a concrete user case appears, the refactor to N
  atlases (one per font) is mechanical given the `AtlasSlot` design.
- Eliminating the MSDF path. Both modes ship; MSDF stays the default.
- Vector / outline rendering. Plain SDF is still a rasterized distance
  field with the same atlas + per-glyph-quad pipeline. Genuine vector
  rendering is a separate, much larger project.
- Switching MSDF/SDF mode mid-frame or per-draw within one atlas.
  Mode switch builds a parallel atlas in the background and swaps when
  the new atlas is fully populated (no flicker, no in-place clear).

## Approach

Three structural changes carry the design:

1. **Trait-based rasterizer.** `pub trait Rasterizer { fn rasterize(...)
   -> Option<RasterizedBitmap>; fn mode(&self) -> DistanceField; }` with two
   concrete implementations: `MsdfRasterizer` (current behavior) and
   `SdfRasterizer` (new, single-channel). `MtsdfRasterizer` lands later
   as a third implementation — no churn to dispatch sites.

2. **Typed bitmap output.** `pub enum RasterizedBitmap { Msdf(MsdfBitmap),
   Sdf(SdfBitmap) }`. Each variant has the channel layout right for its
   mode (MSDF = 3 distinct channels, SDF = 1 channel). MTSDF later adds
   an `Mtsdf(MtsdfBitmap)` variant with 4 channels.

3. **Atlas slot for parallel-swap.** The world-level resource is now
   `AtlasSlot`, not `GlyphAtlas` directly. `AtlasSlot` is an enum:
   `Single(GlyphAtlas)` during normal operation, `Swapping { active,
   pending }` while a mode switch is mid-flight. Materials always sample
   from `active`; new rasterizations go to `pending`. When `pending` is
   fully populated, swap to `Single(pending)`. **No flicker, no race
   conditions, no in-place clear.**

```text
AtlasConfig.distance_field
        │
        ▼
AtlasSlot::Single(GlyphAtlas { rasterizer: Arc<dyn Rasterizer>, ... })
        │
   on mode toggle:
        │
        ▼
AtlasSlot::Swapping {
    active:  GlyphAtlas { rasterizer: MsdfRasterizer, ... },  // ← materials still sample here
    pending: GlyphAtlas { rasterizer: SdfRasterizer,  ... },  // ← workers re-rasterize all glyphs here
}
        │
   when pending is fully populated:
        │
        ▼
AtlasSlot::Single(GlyphAtlas { rasterizer: SdfRasterizer, ... })
        (old active atlas dropped)
```

Everything else — atlas packing, glyph quad construction, layout, shaping,
panel clipping, shadow rendering — is unchanged. The shader gains a one-
line branch (median vs. R) on a uniform; the material gains a typed
`distance_field: DistanceField` field that converts to `u32` only at the GPU boundary.

**Renames** (mechanical, all-at-once before SDF work begins):

- `MsdfAtlas` → `GlyphAtlas`
- `MsdfTextMaterial` → `GlyphMaterial`
- `MsdfTextMaterialInput` → `GlyphMaterialInput`
- `MsdfTextUniform` → `GlyphMaterialUniform`
- `MsdfExtension` → `GlyphMaterialExtension`
- `shaders/msdf_text.wgsl` → `shaders/glyph_text.wgsl`
- `MsdfBitmap` stays as the name for the MSDF-specific bitmap struct
  (it really is MSDF — it's now one variant of `RasterizedBitmap`).

## Phases

### Phase 1 — Rasterizer trait + RasterizedBitmap enum

**Files:** `text/msdf_rasterizer/mod.rs` (refactor + add SDF path),
`text/msdf_rasterizer/parity.rs` (no logic change; type updates only),
possibly split into `text/rasterizer/{mod.rs, msdf.rs, sdf.rs,
rasterized_bitmap.rs}` if the file grows past comfort.

**fdsm API (verified against fdsm 0.8 source):**
- `fdsm::generate::generate_msdf(...)` — RGB output, already in use.
- `fdsm::generate::generate_sdf(component, range, dest)` — `Luma<P>`
  output (single channel). Takes the un-colored `PreparedComponent`
  directly; no `edge_coloring_simple` step needed for SDF.
- `fdsm::render::correct_sign_sdf(sdf, component, fill_rule)` — same role
  as `correct_sign_msdf` but for `Luma`.
- **No `correct_error` step for SDF** — that's MSDF-specific (fixes
  channel-disagreement artifacts that single-channel can't have).
- (Future) `fdsm::generate::generate_mtsdf(...)` — `Rgba<P>` output. The
  trait design accommodates this without API changes.

**New types:**

```rust
/// Tag describing which signed-distance-field variant a rasterizer
/// produces. Used at the API boundary (config, material uniform).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum DistanceField {
    #[default]
    Msdf,
    Sdf,
    // Mtsdf,  ← reserved; lands when MTSDF rasterizer ships
}

impl From<DistanceField> for u32 {
    fn from(mode: DistanceField) -> u32 {
        match mode {
            DistanceField::Msdf => 0,
            DistanceField::Sdf  => 1,
        }
    }
}

/// Rasterized glyph bitmap, tagged by channel layout.
pub enum RasterizedBitmap {
    /// 3 channels (R, G, B) carrying independent signed pseudo-distances.
    Msdf(MsdfBitmap),
    /// 1 channel carrying a single signed distance.
    Sdf(SdfBitmap),
    // Mtsdf(MtsdfBitmap)  ← future, 4 channels
}

impl RasterizedBitmap {
    pub fn dimensions(&self) -> UVec2        { /* shared accessor */ }
    pub fn bearings(&self)   -> (f64, f64)   { /* shared accessor */ }
    pub fn distance_field(&self) -> DistanceField { /* discriminant → variant tag */ }
}

pub struct SdfBitmap {
    pub data:      Vec<u8>,  // width × height × 1 byte
    pub width:     u32,
    pub height:    u32,
    pub bearing_x: f64,
    pub bearing_y: f64,
}

/// Trait every rasterizer implements. Allows MTSDF (and others) to plug
/// in later without touching the dispatch sites.
///
/// Each impl owns its rasterization constants (px_size, sdf_range,
/// padding) so the trait method needs only the per-glyph inputs. This
/// eliminates a class of parameter-mismatch bugs across the active/
/// pending atlas pair during a mode swap.
// Top of file: `use std::fmt::Debug;` per the imports-at-the-top rule.
pub trait Rasterizer: Send + Sync + 'static + Debug {
    fn rasterize(
        &self,
        font_data: &[u8],
        glyph_index: u16,
    ) -> Option<RasterizedBitmap>;

    fn mode(&self) -> DistanceField;
}

#[derive(Debug)]
pub struct MsdfRasterizer {
    px_size:   u32,
    sdf_range: f64,
    padding:   u32,
}

#[derive(Debug)]
pub struct SdfRasterizer {
    px_size:   u32,
    sdf_range: f64,
    padding:   u32,
}

// #[derive(Debug)]
// pub struct MtsdfRasterizer { ... }  ← future: fdsm::generate::generate_mtsdf
```

**Work:**
- Refactor the existing `rasterize_glyph` body into
  `MsdfRasterizer::rasterize`. Behavior identical; just moved.
- Implement `SdfRasterizer::rasterize`: skip `edge_coloring_simple`
  (it's a multi-channel concern); call `outline.prepare()` directly on
  the un-colored outline, then `generate_sdf` into a `Luma<f32>` image,
  then `correct_sign_sdf`. No error correction. Convert `f32 → u8`. Pack
  into `RasterizedBitmap::Sdf(SdfBitmap { data, width, height,
  bearing_x, bearing_y })` — single channel, not replicated. The atlas
  insert (Phase 2) is the one place that decides how to lay it out on
  the RGBA atlas texture (e.g., write the single channel into R, leave
  G/B/A as zero — the shader's branch ignores them in SDF mode).
- The atlas owns an `Arc<dyn Rasterizer>` instead of calling a free
  function. Construction-time choice (from `AtlasConfig.distance_field`)
  picks the implementation. `Arc` (not `Box`) so async workers can
  hold an independent clone without atlas-locking.
- Tests:
  - Per-rasterizer round-trip tests: feed a known glyph, assert the
    returned variant matches the rasterizer's `mode()`.
  - Both rasterizers return a bitmap of the same dimensions for the same
    glyph (verifies bearing/bbox parity).
  - Degenerate-glyph tests for the SDF path matching MSDF coverage:
    empty outline returns `None`; very small glyphs don't panic;
    self-intersecting paths produce non-trivial bitmaps. Use the
    existing `eb_garamond_*` and `rasterize_*` tests as the template.
- Parity test (`parity.rs`) stays MSDF-only — msdfgen reference is
  multi-channel; an SDF parity test would need a separate reference and
  isn't part of this scope.

### Phase 2 — Renames + GlyphAtlas owns a rasterizer

**Files:** `text/atlas.rs` (rename + behavior changes), `text/atlas_config.rs`
(new field + test), `text/mod.rs` (plugin init), and the many call sites
that reference the renamed types.

**Renames (mechanical, one commit):**
- `MsdfAtlas` → `GlyphAtlas`
- `MsdfTextMaterial` → `GlyphMaterial`
- `MsdfTextMaterialInput` → `GlyphMaterialInput`
- `MsdfShadowProxyMaterialInput` → `GlyphShadowProxyMaterialInput`
- `MsdfTextUniform` → `GlyphMaterialUniform`
- `MsdfExtension` → `GlyphMaterialExtension`
- `shaders/msdf_text.wgsl` → `shaders/glyph_text.wgsl`
- `EMBEDDED_MSDF_TEXT_SHADER_PATH` → `EMBEDDED_GLYPH_TEXT_SHADER_PATH`
- `MsdfBitmap` keeps its name (it's MSDF-specific; one variant of
  `RasterizedBitmap`).
- `msdf_text_material()` / `msdf_shadow_proxy_material()` →
  `glyph_material()` / `glyph_shadow_proxy_material()`.

**Behavior changes:**
- Add `distance_field: DistanceField` field to `AtlasConfig`. Default `Msdf`.
- Add `const fn with_distance_field(mut self, mode: DistanceField) -> Self` builder.
- `GlyphAtlas` stores `rasterizer: Arc<dyn Rasterizer>` (was `Box<dyn>`
  in earlier drafts; `Arc` so async workers can hold an independent
  clone without atlas-locking) instead of hardcoding the MSDF path.
  Constructors pick the implementation from the `DistanceField` argument and
  pass the atlas's `canonical_size` / `sdf_range` / `padding` into the
  rasterizer at construction:
  ```rust
  impl GlyphAtlas {
      pub fn with_config(
          page_size: u32, canonical_size: u32,
          glyph_worker_threads: usize, distance_field: DistanceField,
          shared_worker_pool: Option<Arc<TaskPool>>,
      ) -> Self {
          let rasterizer: Arc<dyn Rasterizer> = match distance_field {
              DistanceField::Msdf => Arc::new(MsdfRasterizer::new(
                  canonical_size, DEFAULT_SDF_RANGE, DEFAULT_GLYPH_PADDING,
              )),
              DistanceField::Sdf  => Arc::new(SdfRasterizer::new(
                  canonical_size, DEFAULT_SDF_RANGE, DEFAULT_GLYPH_PADDING,
              )),
          };
          /* ... */
      }
  }
  ```
- **Worker pool sharing.** Currently each `MsdfAtlas` creates its own
  `TaskPool`. With two atlases coexisting during a swap, that would
  double the live thread count. Refactor: `with_config` accepts an
  optional `Arc<TaskPool>`. If `Some`, use the shared pool; if `None`,
  create a private pool. `AtlasSlot::Swapping` constructs `pending`
  with `Some(active.worker_pool.clone())` so both atlases share threads.
- The atlas no longer stores a redundant `distance_field: DistanceField` field —
  `self.rasterizer.mode()` is the single source of truth.
  `pub fn distance_field(&self) -> DistanceField { self.rasterizer.mode() }`
  accessor for downstream (material build) code.
- Add `pub fn is_ready(&self, key: GlyphKey) -> bool { self.glyphs.contains_key(&key) }`
  — used by the Phase 4 swap-completion check. The existing
  `get_metrics(key) -> Option<GlyphMetrics>` could be used instead, but
  `is_ready` reads better at the call site.
- All call sites of the old free function `rasterize_glyph` now go
  through `self.rasterizer.rasterize(...)`. The async worker closure
  needs the rasterizer too — wrap it in `Arc<dyn Rasterizer>` so the
  worker can hold a clone independent of the atlas lock. (`Send + Sync`
  bound on the trait makes this work.)
- The atlas insert path now matches on `RasterizedBitmap` variant when
  copying bytes into the page texture. For `Msdf`, write 3 bytes/pixel
  into RGB. For `Sdf`, write 1 byte/pixel into R, leave G/B/A as zero
  (the shader's branch reads only R in SDF mode).
- Plugin init (`text/mod.rs`) plumbs `AtlasConfig.distance_field` into atlas
  construction.
- Update `default_config_values` test to assert
  `config.distance_field == DistanceField::Msdf`.
- `GlyphMetrics` is **mode-agnostic**: UV rect, bearings, and pixel
  dimensions are identical regardless of which rasterizer produced the
  underlying bitmap. Document this in the `GlyphMetrics` doc comment
  and add a test asserting MSDF and SDF rasterizations of the same
  glyph yield identical `GlyphMetrics`.

### Phase 3 — Material + shader branch on the mode

**Files:** `render/glyph_material.rs` (renamed from msdf_material.rs),
`shaders/glyph_text.wgsl` (renamed from msdf_text.wgsl), material spawn
sites in `render/text_renderer/batching.rs` (5 spawn sites: lines 429,
445, 522, 590, 606) and `render/world_text/mesh_spawning.rs` (2 spawn
sites: lines 106, 153).

**Spawn-site coverage:** all current spawn sites flow through the shared
helpers `glyph_material(...)` and `glyph_shadow_proxy_material(...)`,
which both delegate to the private `build_glyph_material(...)`. Adding
`distance_field: DistanceField` (typed, not `u32`) to `GlyphMaterialInput` +
`GlyphShadowProxyMaterialInput` + the helper signature makes every
spawn site a compile error until it passes the mode. The `u32`
conversion happens **inside the helper, once**, via
`u32::from(distance_field)` when constructing `GlyphMaterialUniform`. Spawn
sites never deal with the magic 0/1 values.

**Uniform layout:** `GlyphMaterialUniform` uses `#[derive(ShaderType)]`
(encase), which handles WGSL std140 alignment automatically. Adding a
`u32 distance_field` between `is_shadow_proxy: u32` and `clip_rect: Vec4`
keeps two `u32`s together (8 bytes), pads to 16 for the `Vec4`, and
encase derives the same padding in the WGSL struct via the
`AsBindGroup` machinery. **There is no manual byte-offset arithmetic to
get wrong** as long as the WGSL struct is updated in the same field
order. Add a test that round-trips a known `GlyphMaterialUniform`
instance through encase's `WriteInto` and asserts the byte length
matches `<GlyphMaterialUniform as ShaderType>::min_size()`.

**Shader change:**
- Current (`shaders/msdf_text.wgsl:95`):
  ```wgsl
  let sd = median(msdf_sample.r, msdf_sample.g, msdf_sample.b) - 0.5;
  ```
- New (`shaders/glyph_text.wgsl`):
  ```wgsl
  let distance = select(
      atlas_sample.r,
      median(atlas_sample.r, atlas_sample.g, atlas_sample.b),
      uniforms.distance_field == 0u,
  );
  let sd = distance - 0.5;
  ```
- The `-0.5` bias and the `screen_px_range()` adaptive AA step
  (lines 76-86) apply identically to both modes. Don't introduce a new
  `msdf_median` function — reuse the existing `median()` at line 72.
- The branch is uniform across all fragments of a draw call; modern GPUs
  have zero perf penalty for uniform control flow.
- The `msdf_sample` local variable name should be renamed to something
  mode-agnostic (e.g., `atlas_sample`) as part of the shader rename.

### Phase 4 — Parallel-atlas swap on mode change

**Files:** `text/atlas_slot.rs` (new — the slot enum + transition logic),
`text/atlas.rs` (no longer the resource type; becomes a worker owned by
`AtlasSlot`), `text/mod.rs` (plugin init + driver system),
`DistanceFieldPreference` resource (new, small module).

**Design:** instead of clearing the live atlas and re-rasterizing in
place, build a fresh atlas with the new mode in the background. While
it populates, the existing atlas keeps serving renders unchanged. When
the new atlas is fully populated with the same glyph set as the active
one, swap them atomically (one resource-mutation), drop the old atlas.

This approach has **no flicker, no race conditions, and no in-place
mutation of the live atlas**. The cost is temporary 2× atlas memory
during the swap window (acceptable for a debug toggle, and short-lived).

**The slot type:**

```rust
/// World-level resource. Owns the currently-active atlas and, during a
/// mode-switch, the pending replacement.
#[derive(Resource)]
pub enum AtlasSlot {
    /// Steady state. One atlas; everyone uses it.
    Single(GlyphAtlas),

    /// Mid-swap state.
    /// - `active`      : materials sample from here this frame
    /// - `pending`     : being populated by background workers
    /// - `target_keys` : glyph keys that must be populated in
    ///   `pending` before the swap completes. Ordered `Vec` (not
    ///   `HashSet`) so the pre-warm enqueue order is deterministic,
    ///   which keeps `pending`'s page allocation order matching
    ///   `active`'s. Sorted by `(font_id, glyph_index)`.
    /// Snapshot includes both `active.glyphs.keys()` (already
    /// rasterized) AND `active.in_flight` (queued/in-progress) so
    /// pending pre-warms everything active was working on.
    Swapping {
        active:      GlyphAtlas,
        pending:     GlyphAtlas,
        target_keys: Vec<GlyphKey>,
    },
}

impl AtlasSlot {
    /// What materials sample from this frame.
    pub fn active(&self) -> &GlyphAtlas {
        match self {
            Self::Single(a)             => a,
            Self::Swapping { active, .. } => active,
        }
    }

    /// Where new rasterizations go.
    pub fn rasterize_target_mut(&mut self) -> &mut GlyphAtlas {
        match self {
            Self::Single(a)              => a,
            Self::Swapping { pending, .. } => pending,
        }
    }

    /// Distance-field variant the world is rendering with right now.
    pub fn distance_field(&self) -> DistanceField { self.active().distance_field() }

    /// Variant the slot is transitioning to, if any.
    pub fn target_distance_field(&self) -> Option<DistanceField> {
        match self {
            Self::Single(_)               => None,
            Self::Swapping { pending, .. } => Some(pending.distance_field()),
        }
    }

    /// Convenience accessors that delegate to `active()`. Saves every
    /// read-only call site from writing `.active().width()` etc.
    pub fn width(&self)  -> u32 { self.active().width() }
    pub fn height(&self) -> u32 { self.active().height() }
    pub fn image_handle(&self, page: u32) -> Option<&Handle<Image>> {
        self.active().image_handle(page)
    }
    pub fn page_count(&self) -> usize { self.active().page_count() }
}
```

**Driver:**
- Introduce `DistanceFieldPreference(DistanceField)` resource. App-level UI writes
  this. Initial value matches `AtlasConfig.distance_field` (read at plugin
  init to avoid an immediate first-frame swap).
- A system runs each frame in `PostUpdate`, **before**
  `shape_panel_text_children` (so mode changes are picked up before any
  shaping work queues new glyphs onto the wrong atlas):
  1. If `slot` is `Single(active)` and `preference.0 != active.distance_field()`,
     transition to `Swapping`:
     - Build `target_keys: Vec<GlyphKey>` = sorted union of
       `active.glyphs.keys()` (cached) and `active.in_flight_keys()`
       (queued/in-progress). Sort by `(font_id, glyph_index)` for
       deterministic pre-warm order → matching page allocation order
       in `pending`.
     - Build a fresh `pending = GlyphAtlas::with_config(... preference.0,
       Some(active.worker_pool.clone()))` (shared worker pool).
     - Move out of `slot`: `*slot = AtlasSlot::Swapping { active, pending,
       target_keys }` — destructure the prior `Single(active)` to move
       its `GlyphAtlas` into the new variant. No placeholder needed.
     - During the swap window, the slot's `rasterize_target_mut()`
       routes new rasterizations to `pending`, and user text continues
       rendering from `active`. The shared worker pool services both.
  2. **Pre-warm `pending`** by enqueueing all `target_keys` not yet in
     `pending`. The worker closures capture `pending`'s
     `Arc<dyn Rasterizer>` at dispatch time — results can only come
     back from the rasterizer they were dispatched with, so there is
     no cross-atlas contamination by construction.
  3. Each frame thereafter, check `target_keys.iter().all(|k|
     pending.is_ready(k))`. When all are populated, complete the swap.
     `AtlasSlot` implements `Default` (returning `Single(GlyphAtlas::default())`
     where `GlyphAtlas::default()` is the zero-page empty atlas), so
     `std::mem::take` is the idiomatic way to move `pending` out:
     ```rust
     impl AtlasSlot {
         /// Finalize a Swapping → Single transition. Old `active` is
         /// dropped; `pending` becomes the new active. No-op if not
         /// currently Swapping.
         pub fn complete_swap(&mut self) {
             *self = match std::mem::take(self) {
                 Self::Swapping { pending, .. } => Self::Single(pending),
                 single                         => single,
             };
         }
     }
     ```
     `GlyphAtlas::default()` returns an empty atlas (no pages, no
     glyphs, no worker pool) — used internally only for this `mem::take`
     transition. It is `pub(crate)`; no public API surface exposes a
     "create an empty atlas" affordance to apps.
- New glyphs requested *during* the swap go through `rasterize_target_mut()`
  → land in `pending`. When the swap completes, pending becomes active
  with those new glyphs already present (or still in-flight) — they
  continue rendering correctly after the swap. No data loss.
- In-flight glyphs in `active` whose workers complete after the swap
  has finalized post to a dropped channel: `send` returns Err, result
  is dropped. The next render request for that glyph cache-misses
  against the new active and re-rasterizes in the new mode — redundant
  work, no user-visible bug.
- Materials don't need rebuilding: their `Handle<Image>` and `uv_rect`
  values point into `active`. When the slot becomes `Single(new)`, the
  *next* material spawn (or next material update) reads from the new
  atlas. Existing materials still rendering from the old atlas keep
  working until they're respawned — Bevy holds the dropped images
  alive while strong refs exist.

**Why no flicker:** `active` is never mutated during the swap. Every
frame either renders from the old atlas (slot = Swapping) or from the
new atlas (slot = Single(new)). There's no in-between frame with a
half-cleared texture.

**Why no race conditions:** workers writing to `pending` use
`pending`'s `Arc<dyn Rasterizer>` (captured at dispatch). They
*cannot* write into `active`. Results posted back to `pending`'s channel
flow into `pending`'s glyph cache. The `active` atlas is read-only for
the duration of the swap.

**Edge cases:**
- User toggles back during a swap (Sdf → Msdf while Msdf → Sdf is mid-
  flight): the simplest correct behavior is to abandon the in-flight
  pending, return to `Single(active)`, then immediately re-trigger.
  `pending` and its outstanding worker results are dropped (workers
  hold an `Arc<dyn Rasterizer>`; when pending drops, its receiver
  channel drops; workers writing to a dropped channel get `Err` and
  abandon their result harmlessly).
- New glyphs requested during the swap (user types a character not in
  `target_keys`): these go to `pending` via `rasterize_target_mut()`.
  They eventually appear when the swap completes. During the swap, the
  user sees the glyph as a fallback (queued) — same as any cold-cache
  glyph request today.
- Memory pressure during the swap: peak usage is roughly 2× normal
  atlas memory. Document this in the typography example UI ("expect a
  brief memory spike during mode switch").

### Phase 5 — Typography example UI

**Files:** `examples/typography.rs`

The example writes directly to the existing `DistanceFieldPreference`
resource; **no new enum or wrapper state introduced in the example** —
that would duplicate `DistanceField` without adding information.

- Key binding (suggest `S`): on press, flip
  `preference.0 = match preference.0 { DistanceField::Msdf => DistanceField::Sdf,
  DistanceField::Sdf => DistanceField::Msdf, }`.
- Add a chip to the title bar wired via `wire_chip_to_state` against
  `DistanceFieldPreference` (same pattern as `T Overlay` and `←/→ Cycle
  Word`):
  ```rust
  .with_title_bar(
      TitleBar::new()
          .control("T Overlay")
          .control("←/→ Cycle Word")
          .control("S SDF/MSDF"),
  )
  .wire_chip_to_state::<DistanceFieldPreference, _>("S SDF/MSDF", |pref| match pref.0 {
      DistanceField::Sdf  => ControlActivation::Active,
      DistanceField::Msdf => ControlActivation::Inactive,
  })
  ```
- Initial preference value is `DistanceField::Msdf` (matches `AtlasConfig`
  default).
- The chip's "active" indicator may optionally reflect
  `AtlasSlot::target_distance_field().is_some()` (swap in progress) with a third
  visual state; implementation detail for the UI pass.

## Public API surface

Minimal. App code touches:

- `pub enum DistanceField { Msdf, Sdf }` (config + preference)
- `pub struct AtlasConfig` with `const fn with_distance_field(mode: DistanceField)`
- `pub struct DistanceFieldPreference(pub DistanceField)` resource (writable from
  app systems / UI)
- `pub struct GlyphAtlas` (read-only access to `.distance_field()`,
  `.glyph_count()`, etc.)

Everything else stays `pub(crate)`:
- `Rasterizer` trait
- `MsdfRasterizer` / `SdfRasterizer` impl structs
- `RasterizedBitmap` enum
- `MsdfBitmap` / `SdfBitmap` structs
- `AtlasSlot` enum (apps go through methods on it via the `Res<AtlasSlot>`)

## Risks

1. **SDF may round visible corners more than expected.** This is the
   architectural trade and the whole point of letting the user toggle.
   No mitigation needed beyond confirming via the typography overlay.
2. **Atlas memory.** Both modes use the same RGBA texture format
   regardless of channel count needed. SDF mode wastes 3 channels but
   keeps everything else identical. A future optimization could use a
   single-channel R8 texture for SDF-only atlases, but that requires a
   second pipeline and is out of scope.
3. **Temporary 2× atlas memory during a mode switch.** While
   `AtlasSlot::Swapping` holds both `active` and `pending`, memory
   usage roughly doubles. Resolves automatically when the swap
   completes and the old atlas drops. Acceptable for a debug toggle;
   could matter for production apps that toggle frequently.
4. **Mode-switch latency scales with cached glyph count.** A swap
   doesn't complete until every key in the snapshot has been
   re-rasterized in the new mode. For an atlas with thousands of
   glyphs this could be seconds. Mitigation: the user can keep typing /
   using the app during the swap because `active` is still serving
   renders — only the visible mode change is delayed.
5. **GPU OOM during pending allocation.** If `pending`'s first page
   allocation fails (e.g., near GPU memory limit), the swap can't
   complete. Behavior: log a warning, abandon `pending`, return slot
   to `Single(active)`. App keeps rendering in the original mode.
6. **Shader hot-reload during a swap.** Not supported. If a developer
   modifies `glyph_text.wgsl` mid-swap, behavior is undefined (some
   materials use the old shader, others the new). Document: don't
   hot-reload shaders while a swap is in progress.
7. **Swap progress visibility.** A long swap (thousands of glyphs)
   takes seconds with no user feedback. Mitigation: typography example
   chip reflects `AtlasSlot::target_distance_field().is_some()` to show "mode
   switch in progress." For production apps, expose a progress
   accessor (`current_progress: (ready, total)`) for custom UI.

**Investigated and addressed in the phases above:**
- fdsm 0.8 has `generate_sdf` (`Luma` output), `correct_sign_sdf`, and
  `generate_mtsdf` (for the future MTSDF rasterizer). Spec'd in
  Phase 1.
- Uniform struct layout: `GlyphMaterialUniform` uses `ShaderType` derive
  (encase), which handles WGSL std140 padding. Spec'd in Phase 3 with a
  round-trip size test.
- All material spawn sites flow through `build_glyph_material`; adding
  the field to the helper signature makes every spawn site a compile
  error until updated. Spec'd in Phase 3.
- Shader change details (preserve `-0.5` bias, preserve adaptive AA,
  reuse existing `median()` function). Spec'd in Phase 3.
- Default config test + GlyphMetrics doc + GlyphMetrics mode-agnostic
  test. Spec'd in Phase 2.
- **Atlas regeneration race conditions.** Eliminated by parallel-atlas
  design (Phase 4). The active atlas is read-only during the swap;
  pending workers are bound to pending's rasterizer at dispatch.
- **Stale `Handle<Image>` references.** Eliminated by parallel-atlas
  design. `pending` allocates its own page handles; the swap to
  `Single(new)` happens after pending is fully populated and uploaded.

## Out of scope (deliberately deferred)

- **Per-font mode selection** (e.g., serif → SDF, sans → MSDF
  simultaneously). Deferred. Most apps and the typography example are
  single-font. When a concrete user case appears, the refactor to N
  atlases (one per font, each wrapped in its own `AtlasSlot`) is
  mechanical given the trait-based rasterizer design.
- **R8-only atlas texture format for SDF / Sdf rasterizers.** Memory
  optimization. Requires a second GPU pipeline (different texture
  format = different bind group layout = different shader pipeline).
  Defer until justified by an app hitting GPU memory pressure.
- **Mixed-mode atlases** (some glyphs MSDF, some SDF in one atlas).
  Adds per-glyph mode dispatch in the shader and per-glyph mode
  metadata in `GlyphMetrics`. Not justified without a concrete need;
  the `RasterizedBitmap` enum + `Rasterizer` trait don't preclude
  this future, but the current code assumes one rasterizer per atlas.
- **Sub-pixel positioning improvements** (orthogonal concern).
- **A general atlas control-panel UI** (runtime canonical size, SDF
  range, etc.). The toggle here is one specific debug affordance, not a
  general atlas tuning surface.

## Future direction: MTSDF

The `Rasterizer` trait + `RasterizedBitmap` enum are designed
specifically to accommodate MTSDF (Multi-channel + True Signed
Distance Field) as a future drop-in:

- fdsm 0.8 already has `generate_mtsdf` returning `Rgba<P>`.
- Adding MTSDF later means:
  1. Add `Mtsdf(MtsdfBitmap)` variant to `RasterizedBitmap`.
  2. Add `DistanceField::Mtsdf` variant + `From<DistanceField> for u32` mapping
     (e.g., `Msdf=0, Sdf=1, Mtsdf=2`).
  3. Implement `MtsdfRasterizer: Rasterizer`.
  4. Add an atlas insert branch for the 4-channel `Mtsdf` variant
     (write all 4 channels into RGBA).
  5. Add a shader branch reading R/A and using A for outlines / glows /
     drop shadows.
- No churn to spawn sites, no churn to the slot, no churn to existing
  rasterizers.

MTSDF is the path for high-quality text effects (drop shadows,
outlines, glows) that plain MSDF can't compute cleanly from its
median-of-RGB. That work is out of scope here but unblocked by the
trait-based design.

## Estimated cost

- **Phase 1 (rasterizer trait + RasterizedBitmap + SDF impl):** ~1 day.
- **Phase 2 (renames + GlyphAtlas owns rasterizer):** ~half day; the
  renames touch many files but are mechanical.
- **Phase 3 (material + shader branch + spawn site updates):** ~half day.
- **Phase 4 (parallel-atlas swap):** ~1.5 days. The design is sound but
  the implementation needs careful tests for the swap state machine,
  worker channel lifecycle on `pending` drop, and the "toggle back
  mid-swap" edge case.
- **Phase 5 (typography example UI):** ~half day.

**Total: ~4 days of focused work.** Phases 1–3 can land in one PR
(rename + plumbing — both modes selectable at startup but not at
runtime yet). Phase 4 is its own PR (parallel swap). Phase 5 follows
in a small UI-only PR.
