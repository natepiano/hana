# Shadow Casting Cascade

## What it is

Diegetic panels, elements, and text decide whether they cast Bevy 3D shadows
through one shared, inheritance-aware policy. A single `ShadowCasting` value
(`Off`/`On`) flows down a `global default -> panel -> element -> element-specific
override` cascade, so a caller can turn shadows off (or on) at any level and have
descendants inherit that choice unless they author their own. Shadow
participation is a per-node visual fact that must both (a) inherit like every
other diegetic style attribute and (b) split render batches — one GPU batch
entity either casts shadows or carries `NotShadowCaster`, never both.

## How it works

Three layers, matching the cascade machinery already used for font unit,
materials, text alpha, anti-alias, and hairline fade.

### Authoring layer — `Cascade<T>` on public structs

`Cascade<T>` (`crates/bevy_diegetic/src/cascade/authoring.rs`) is the authored slot:

```rust
pub enum Cascade<T> { Inherit, Override(T) }   // #[default] Inherit
```

It carries the usual combinators (`is_inherit`, `as_override`, `resolve_or`,
`map`, `copied`, `From<Option<T>>`). Public structs store `Cascade<T>` fields,
all defaulting to `Cascade::Inherit`:

- `DiegeticPanel` (`crates/bevy_diegetic/src/panel/diegetic_panel.rs`):
  `font_unit: Cascade<Unit>`, `shadow_casting: Cascade<ShadowCasting>`,
  `material` / `text_material` / `shape_material: Cascade<Handle<StandardMaterial>>`,
  `text_alpha_mode: Cascade<AlphaMode>`, `hdr_text_coverage_bias: Cascade<f32>`.
- `El` builder (`crates/bevy_diegetic/src/layout/builder.rs`):
  `Cascade<Handle<StandardMaterial>>`, `Cascade<AntiAlias>`,
  `Cascade<HairlineFade>`, `Cascade<ShadowCasting>`. (`Element`, in
  `layout/element.rs`, is the internal element representation; `El` is the public
  builder — the authoring verb is `El::shadow_casting`.)
- `TextStyle` (`crates/bevy_diegetic/src/layout/text_props.rs`):
  `shadow_mode: Cascade<GlyphShadowMode>`, `shadow_casting: Cascade<ShadowCasting>`,
  `material: Cascade<Handle<StandardMaterial>>`, `hdr_text_coverage_bias: Cascade<f32>`.
- `PanelLine` / `LineStyle` / `PanelCircle` (`crates/bevy_diegetic/src/layout/line.rs`):
  `shadow_casting: Cascade<ShadowCasting>`, plus material and hairline-fade
  cascades. The public authoring entry point is `PanelLine::shadow_casting(...)`.

### Runtime ECS propagation — `Override<T>` -> `CascadePlugin<T>` -> `Resolved<T>`

Defined in `crates/bevy_diegetic/src/cascade/`. `ShadowCasting` and
`GlyphShadowMode` join the cascade in `resolved.rs` via
`cascade_attr!(existing ShadowCasting, default = ShadowCasting::On)` and
`cascade_attr!(existing GlyphShadowMode, default = GlyphShadowMode::Cast)` — the
value type *is* the attribute, no wrapper struct.

- `Override<A>(pub A)` is the authored input component (present = overrides,
  absent = inherit); `Resolved<A>(pub A)` is the per-entity cached output. Both
  are `pub(crate)`.
- `CascadePlugin<A>` (`plugin.rs`) registers reflection and runs
  `propagate_cascade::<A>` in `CascadeSet::Propagate` every frame. It re-resolves
  any node whose own `Override<A>` changed/was removed, whose `ChildOf` changed,
  or (sentinel-gated) whenever `CascadeDefault<A>` changed, fanning subtree
  dirtiness down `Children`.
- Resolution is `resolve_walk` (`resolved.rs`): first ancestor with an
  `Override<A>` wins, else the global default; bounded by `CASCADE_DEPTH_CAP` with
  cycle detection.

Panel construction seeds `Override<ShadowCasting>` from the authored `Cascade`
via `apply_cascade_override` (`diegetic_panel.rs`); text-label bridges do the same
for `GlyphShadowMode` / `ShadowCasting`. Render systems query `&Resolved<T>` and
filter on `Changed<Resolved<T>>`.

### Render routing — `ShadowCasting` -> `VisualShadow` in batch keys

`VisualShadow` (`crates/bevy_diegetic/src/render/batch_key.rs`) is the internal
batch-key discriminant:

```rust
pub(crate) enum VisualShadow { Cast, None }
```

It has `From<ShadowCasting>` (`On -> Cast`, `Off -> None`), `From<GlyphShadowMode>`
(`Cast -> Cast`, `None -> None`), and `From<SurfaceShadow>`. It is a field in
`SdfBatchKey` (`render/fill_batch.rs`, `shadow: VisualShadow`), `PathBatchKey`
(`render/analytic_paths/batching.rs`), and `ImageBatchKey`
(`render/image_batch.rs`). When a batch entity is spawned,
`key.shadow == VisualShadow::None` inserts `NotShadowCaster`; mixed participation
therefore produces distinct keys and separate draws.

### Applied cascades

The cascade model is not text-only. These authored values preserve inheritance
with `Cascade<T>`:

| Area | Authored field | Runtime use |
| --- | --- | --- |
| Panel font unit | `DiegeticPanel::font_unit: Cascade<Unit>` | seeds `Override<FontUnit>` when authored |
| Panel materials | `material` / `text_material` / `shape_material: Cascade<Handle<StandardMaterial>>` | seed material overrides |
| Panel text alpha | `text_alpha_mode: Cascade<AlphaMode>` | inherited by text children |
| Panel HDR text bias | `hdr_text_coverage_bias: Cascade<f32>` | inherited by text children |
| Element material | `El::material: Cascade<Handle<StandardMaterial>>` | resolves before SDF/path material rows |
| Element antialias | `El::anti_alias: Cascade<AntiAlias>` | resolves for SDF records |
| Element hairline fade | `El::hairline_fade: Cascade<HairlineFade>` | resolves for SDF and path records |
| Panel shadow casting | `DiegeticPanel::shadow_casting: Cascade<ShadowCasting>` | seeds `Resolved<ShadowCasting>` |
| Element shadow casting | `El::shadow_casting: Cascade<ShadowCasting>` | inherited by element-owned content |
| Text shadow casting | `TextStyle::shadow_casting: Cascade<ShadowCasting>` | combines with glyph shadow mode |
| Text glyph shadow mode | `TextStyle::shadow_mode: Cascade<GlyphShadowMode>` | controls text silhouette contribution |
| Panel-line shadow casting | `PanelLine`/`LineStyle::shadow_casting: Cascade<ShadowCasting>` | local line override |
| Panel-circle shadow casting | `PanelCircle::shadow_casting: Cascade<ShadowCasting>` | local primitive override |

### Public API

Use `shadow_casting` for "does this thing participate in Bevy 3D shadow casting?":

```rust
DiegeticPanel::world().shadow_casting(ShadowCasting::Off);
El::new().shadow_casting(ShadowCasting::Off);
TextStyle::new(18.0).with_shadow_casting(ShadowCasting::Off);
PanelLine::new(a, b).shadow_casting(ShadowCasting::Off);
PanelCircle::new(center, radius).shadow_casting(ShadowCasting::Off);
```

Use `shadow_mode` only for the text-specific glyph silhouette policy:

```rust
TextStyle::new(18.0).with_shadow_mode(GlyphShadowMode::None);
```

### Text's dual policy

Text resolves `ShadowCasting` and `GlyphShadowMode` independently and combines
them in `visual_shadow` (`render/panel_text/batching.rs`):

```rust
match shadow_casting {
    ShadowCasting::Off => VisualShadow::None,
    ShadowCasting::On  => glyph_shadow_mode.into(),
}
```

| `ShadowCasting` | `GlyphShadowMode` | Text shadow result |
| --- | --- | --- |
| `Off` | any | no text shadow |
| `On` | `None` | no text shadow |
| `On` | `Cast` | text casts with glyph silhouettes |

### Panel-shape resolution ordering

`effective_shape_shadow` (`render/panel_shapes/batching.rs`):
shape-local `line.shadow_casting().resolve_or(element_shadow)`, where
`element_shadow = element_shadow_casting(...).resolve_or(panel_shadow_casting)`,
and `panel_shadow_casting` falls back to `ShadowCasting::On`. So the order is
shape -> element -> panel -> global default.

### SDF opaque/Mask special case

`sdf_batch_alpha_mode` (`render/fill_batch.rs`):

```rust
match (AlphaMode::from(alpha), shadow) {
    (AlphaMode::Opaque, VisualShadow::Cast) => AlphaMode::Mask(0.0),
    (mode, _)                               => mode,
}
```

An opaque SDF fill/border that casts shadows is batched as `Mask(0.0)` so Bevy
keeps the material bind group on the shadow/prepass pipeline and the SDF prepass
shader can discard by coverage (rounded corners, borders, empty fill regions).
The user's authored material stays opaque; only the batch material is adjusted. A
non-casting opaque batch enters no prepass and stays `Opaque`. Text applies the
analogous mapping at a **separate** code site in `render/panel_text/batching.rs`
(opaque caster -> `Mask(0.0)` to satisfy wgpu depth-only shadow-pipeline
validation) — same observable mapping, not shared code with the SDF helper.

### Image batches

`route_image_batch_records` (`render/image_batch.rs`) reads the owning panel's
`Resolved<ShadowCasting>` (defaulting to `ShadowCasting::On`) and its
`RenderLayers` (defaulting to `RenderLayers::layer(0)`) and hashes both into
`ImageBatchKey { shadow, layers, .. }`. `reconcile_image_batch_entities` inserts
`NotShadowCaster` on a batch entity whose `key.shadow == VisualShadow::None` and
clones the key's layers onto it — same key-splitting model as SDF and path
batches.

### Precompose

Helper panels (`render/precompose.rs`) copy authored `Cascade::Override` values
from the source panel and leave `Cascade::Inherit` alone:

```rust
if let Cascade::Override(shadow_casting) = source.shadow_casting() {
    builder = builder.shadow_casting(shadow_casting);
}
```

Likewise for `material` / `text_material` / `shape_material` / `text_alpha_mode`.
It never freezes a resolved value into an override.

## Invariants

- **Mixed shadow participation must split batches.** `VisualShadow` stays a field
  of `SdfBatchKey`, `PathBatchKey`, and `ImageBatchKey`, because one batch entity
  either casts or carries `NotShadowCaster`.
- **`ShadowCasting::On` is the global default**, matching Bevy's default
  `Mesh3d` / `MeshMaterial3d<StandardMaterial>` behavior. `GlyphShadowMode`
  defaults to `Cast`. Every fallback uses `ShadowCasting::On`.
- **For text, both policies must independently allow a shadow.**
  `ShadowCasting::Off` short-circuits to `VisualShadow::None`; only `On` + `Cast`
  casts.
- **Precompose copies authored `Cascade` values and preserves `Inherit`.** It must
  never freeze a `Resolved` shadow value into a local override (that would block
  later parent/global changes on the helper).
- **Authored public fields default to `Cascade::Inherit`, never `Override(On)`**,
  so inheritance stays live.
- **Shadow changes are visual/render changes only.** Invalidation runs off
  `Changed<Resolved<ShadowCasting>>` (SDF, shape, text routing) and
  `Changed<Resolved<GlyphShadowMode>>` (text); the image router rebuilds per
  frame and re-reads `Resolved<ShadowCasting>` + `RenderLayers` into
  `ImageBatchKey`. They must not trigger text reshaping or structural layout
  rebuilds.
- **The opaque -> `Mask(0.0)` remap is a batch-material adjustment only.** The
  caller's authored material must remain `Opaque`.

## Calibration / gotchas

- **`Mask(0.0)` threshold:** `0.0` discards nothing by threshold alone —
  visibility comes entirely from the SDF shader's coverage — while still selecting
  a maskable pipeline that retains the material bind group in shadow/prepass.
  `sdf_batch_alpha_mode` is the single SDF chokepoint; `opaque_fill_depth_push`
  keys off its result so `Opaque`/`Mask` fills share the depth-buffer regime.
- **Opaque SDF needs the SDF-aware prepass path:** a plain-opaque mesh would cast
  its whole rectangle; the `Mask(0.0)` route forces Bevy into the SDF prepass
  shader so rounded corners, borders, and empty fill regions don't cast as one
  solid quad.
- **Text has its own opaque -> `Mask(0.0)` remap** for a different reason (wgpu
  validation of the depth-only shadow pipeline for opaque casters), implemented
  separately in `panel_text/batching.rs`, not shared with the SDF helper.
- **Image shadow is a prepass discard, not an alpha remap.** Images are always
  `Blend`, so their material bind group survives the shadow pipeline without an
  opaque -> `Mask(0.0)` remap; the prepass fragment samples texture alpha and
  discards (see `as-built/image-batching.md`).
- **`SurfaceShadow`** (`crates/bevy_diegetic/src/panel/coordinate_space.rs`,
  `enum SurfaceShadow { Off, On }`) still exists but is compatibility-only:
  bidirectional `From<SurfaceShadow>` / `From<ShadowCasting>`,
  `From<SurfaceShadow> for VisualShadow`, and `DiegeticPanel::surface_shadow()` /
  builder `surface_shadow()` adapters. New code should use `ShadowCasting`.
- **`Resolved<A>` / `Override<A>` are `pub(crate)`** — external inspectors/tests
  cannot name them; use the `resolved_*` readers (`resolved_shadow_casting`,
  `resolved_glyph_shadow_mode`, etc.).

## Why

- **`Cascade<T>` distinguishes `Inherit` from `Override(On)` deliberately.** If
  every node stored a concrete `ShadowCasting::On`, a later change to a parent's
  or the global default would be silently blocked, because each descendant would
  already carry a winning local value. `Inherit` keeps resolution live so
  parent/global changes still propagate.
- **`VisualShadow` lives in the batch key** because shadow participation is a
  pipeline/routing fact, not a scalar material value: a batch entity is a single
  draw that either casts or wears `NotShadowCaster`, so two nodes with different
  shadow participation cannot share one batch. Putting it in the key is what
  forces the split.
- **opaque -> `Mask(0.0)`:** for a normal mesh, `Opaque` means the whole mesh is
  solid, but an SDF fill's real silhouette is computed in-shader. Remapping to
  `Mask(0.0)` is the minimal change that keeps the material bind group alive on
  the shadow/prepass pipeline so the SDF shader can still discard by coverage —
  without altering the user's authored (opaque) material or the normal opaque
  phase for non-casters.
