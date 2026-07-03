# Image UV Rect — sub-rect sampling and atlas support

> **Status: DESIGN — not started.** Follow-on to
> [`as-built/image-batching.md`](./as-built/image-batching.md). The GPU path is
> already built and live; this feature is the authoring plumbing that exposes it.

## Goal

Let an image element sample a sub-rectangle of its texture instead of the full
0..1 range. Two payoffs:

1. **Cropping / sprite frames** — draw one region of a larger texture (a frame
   of a sprite sheet, a cropped photo) without preprocessing the asset.
2. **Atlasing** — pack many small images into one texture. Because
   `ImageBatchKey` splits on `texture` (plus layers/shadow/z-index), images that
   today occupy N batches (N distinct handles) collapse into one batch when they
   share an atlas texture and differ only by UV rect.

## What already exists (do not rebuild)

The record and shader path shipped with image batching, forward-compat:

- `ImageUvRect { min: Vec2, max: Vec2 }` (`render/image_batch.rs:75`),
  `Default` = full rect (`ZERO..ONE`), `as_vec4()` packs `(min.x, min.y, max.x, max.y)`.
- `ResolvedImageRecord.uv_rect` (`:133`) → `ImageRenderRecord.uv_rect: Vec4`
  (`:185`, part of the const-asserted 128-byte GPU record — no layout change needed).
- Shader `shaders/image_panel.wgsl:85`: `let uv = mix(record.uv_rect.xy, record.uv_rect.zw, box_uv);`
  — sub-rect sampling works today; nothing feeds it anything but the default.
- Record equality includes `uv_rect`, so a changed rect re-uploads only that
  batch's record buffer (no batch churn, no entity churn). Animating the rect
  per frame (sprite animation) is therefore a record-upload-only cost by
  construction.

The single gap: the authoring chain hardcodes `ImageUvRect::default()` at
`render/image_batch.rs:588`.

## Plumbing (the actual work)

Carry a UV rect from the builder to the record:

1. **Authoring type.** `ImageUvRect` is `pub(crate)`. Either promote it to `pub`
   (with `min`/`max` docs stating UV space: `(0,0)` = texture top-left, matching
   Bevy image UV convention — the record's `box_uv` derivation already matches
   the SDF mesh convention, verify against a non-square test texture) or accept
   a `bevy::math::Rect` at the API and convert internally. Decide at
   implementation time; promotion is simpler.
2. **Builder API.** `b.image(el, handle, tint)` (`layout/builder.rs:889`) stays
   as-is. Add a sibling that takes the rect — e.g.
   `b.image_region(el, handle, tint, uv_rect)` — rather than widening the
   existing signature at every call site.
3. **Element content.** `ElementContent::Image { handle, tint }` gains
   `uv_rect` (defaulted in the existing constructor path so current callers are
   untouched).
4. **Render command.** `RenderCommandKind::Image { handle, tint }`
   (`layout/render.rs:103`) gains `uv_rect`; the two construction sites
   (`:183`, `:241`) pass it through.
5. **Route pass.** `image_record_source` / `collect_panel_image_records`
   (`render/image_batch.rs`) forward the authored rect instead of
   `ImageUvRect::default()` at `:588`. `PrecomposeLdr` keeps the full rect —
   precompose targets are sized to the boundary; a sub-rect has no meaning
   there.

## Constraints (from the as-built — must hold)

- `uv_rect` stays **per-record, never in `ImageBatchKey`** — that is the whole
  point (differing rects share a batch). It is `f32` data anyway (`PartialEq`,
  not `Eq`/`Hash`).
- The GPU record layout is const-asserted at `SHADER_SIZE == 128`; the field
  already exists, so no assertion change — do not grow the record for this.
- The router's equality re-upsert (stored `transform` carried onto the rebuilt
  record before comparison) must keep treating an unchanged rect as clean; a
  changed rect must dirty `record_upload` only.
- Element `Sizing` still controls the drawn quad; the rect only remaps sampling.
  Non-matching aspect ratios stretch, same as today's full-rect behavior —
  aspect-preserving fit modes are out of scope here.
- The precompose absent-entry skip and empty-clip cull are untouched.

## Out of scope (name them so they don't creep in)

- **Atlas building/management** — this feature consumes authored rects; packing
  textures into an atlas (offline or at load) is the caller's problem, or a
  later `TextureAtlasLayout` integration that resolves an atlas index to an
  `ImageUvRect` at authoring time.
- **Bindless / texture arrays** — orthogonal batching axis; `ImageBatchKey`
  keeps splitting per texture handle.
- **Nine-slice / tiling** — different sampling math, separate feature.
- **Aspect-fit modes** (contain/cover) — layout-level sizing concern.

## Acceptance sketch

- Data test: two `image_region` elements sharing one texture with different
  rects land in ONE batch with two records carrying distinct `uv_rect` values;
  a rect change dirties only `record_upload`.
- On-screen (`batch_validation`): one image card switched to a quadrant of the
  shared test texture — visually a crop, batch count unchanged.
