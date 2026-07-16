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

### Authoring layer — explicit private state, domain-specific public verbs

`bevy_kana::Cascade<T>` (`crates/bevy_kana/src/cascade.rs`) is the private
authored slot:

```rust
pub enum Cascade<T> { Inherit, Override(T) }   // #[default] Inherit
```

It provides storage-independent inspection, borrowing, transformation, and
ordered resolution. It deliberately has no `Option<T>` conversions because
`Inherit` is authored state, not a generic missing value. `hana_diegetic` does
not re-export this type or return it from public methods. Public callers use
domain verbs; the corresponding structs privately store authored slots that
default to `Cascade::Inherit`:

- `DiegeticPanel` (`crates/hana_diegetic/src/panel/diegetic_panel.rs`) exposes
  construction builders such as `shadow_casting` and `material`. Runtime
  panel changes use typed entity commands such as `override_shadow_casting`,
  `inherit_shadow_casting`, `override_sdf_material`, and
  `inherit_sdf_material`.
- `El` builder (`crates/hana_diegetic/src/layout/builder.rs`):
  `El::shadow_casting`, `El::material`, `El::anti_alias`, and
  `El::hairline_fade` author private element slots. (`Element`, in
  `layout/element.rs`, is the internal element representation.)
- `TextStyle` (`crates/hana_diegetic/src/layout/text_props.rs`):
  `with_shadow_mode`, `with_shadow_casting`, `with_material`, and the other
  `with_*` / `set_*` methods author private text slots.
- `PanelLine` / `LineStyle` / `PanelCircle` (`crates/hana_diegetic/src/layout/line.rs`):
  builders such as `shadow_casting`, `material`, and `hairline_fade` author
  private primitive slots.

### Runtime ECS propagation — `Cascade<T>` -> `CascadePlugin<T>` -> `Resolved<T>`

`bevy_kana` owns the generic ECS engine in
`crates/bevy_kana/src/cascade.rs`. `ShadowCasting` and `GlyphShadowMode` remain
domain values chosen by `hana_diegetic`, with roots `ShadowCasting::On` and
`GlyphShadowMode::Cast`.

- `Cascade<A>` is the authored input component. Absence means the entity does
  not participate, `Inherit` follows `CascadeFrom`, and `Override(value)`
  authors the winning local value.
- `CascadeFrom` is independent of `ChildOf`. Panel text runs carry both because
  transform/despawn ownership and cascade inheritance are separate facts.
  Bevy maintains `CascadeChildren` without linked despawn.
- Shared `CascadePlugin<A>` runs propagation in `CascadeSet::Propagate`. It
  reacts to authored changes/removal, relationship insertion/retargeting/
  removal, and root-default changes, then updates affected descendants.
- Resolution follows `CascadeFrom` until it finds an override, otherwise it
  uses `CascadeDefault<A>`. Cycles and excessive depth use the root default.
  `Resolved<A>` is written only when the effective value changes.

Panel construction conditionally seeds its private authored slot as
`Cascade<ShadowCasting>` through `seed_panel_value` (`diegetic_panel.rs`); an
explicit cascade command queued with the spawn takes precedence. Text-label
bridges likewise seed `GlyphShadowMode` and `ShadowCasting` only when no
explicit authored component is present, and insert `CascadeFrom(panel)`.
Render systems query `&Resolved<T>` and filter on `Changed<Resolved<T>>`.

### Render routing — `ShadowCasting` -> `VisualShadow` in batch keys

`VisualShadow` (`crates/hana_diegetic/src/render/batch_key.rs`) is the internal
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

The cascade model is not text-only. These domain APIs preserve inheritance in
private authored slots:

| Area | Public authoring | Runtime use |
| --- | --- | --- |
| Panel font unit | `DiegeticPanel` font-unit builder; typed runtime entity commands | seed once, then author `Cascade<FontUnit>` directly |
| Panel materials | `material` / `text_material` / `shape_material` builders; typed `override_*_material` / `inherit_*_material` entity commands | seed once, then author live material components directly |
| Panel text alpha | `text_alpha_mode` builder | inherited by text children |
| Panel HDR text bias | `hdr_text_coverage_bias` builder | inherited by text children |
| Element material | `El::material` | resolves before SDF/path material rows |
| Element antialias | `El::anti_alias` | resolves for SDF records |
| Element hairline fade | `El::hairline_fade` | resolves for SDF and path records |
| Panel shadow casting | `DiegeticPanel::shadow_casting` | authors `Cascade<ShadowCasting>` |
| Element shadow casting | `El::shadow_casting` | inherited by element-owned content |
| Text shadow casting | `TextStyle::with_shadow_casting` | combines with glyph shadow mode |
| Text glyph shadow mode | `TextStyle::with_shadow_mode` | controls text silhouette contribution |
| Panel-line shadow casting | `PanelLine::shadow_casting` | local line override |
| Panel-circle shadow casting | `PanelCircle::shadow_casting` | local primitive override |

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
primitive-local `line.shadow_casting().resolve(element_shadow)`, where
`element_shadow = element_shadow_casting(...).resolve(panel_shadow_casting)`,
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

Helper panels (`render/precompose.rs`) carry `CascadeFrom::new(source_panel)`.
Their inheriting cascade components therefore follow the source panel's live
authored values and later runtime changes. The helper authors only the values
required by precomposition itself:

```rust
Cascade::Override(FontUnit(Unit::Points))
Cascade::Override(Lighting::Unlit)
Cascade::Override(HdrTextCoverageBias::NO_BIAS)
```

Tree refreshes replace only the helper's structural panel data; they do not
copy builder material, alpha, or shadow seeds over the helper's live cascade
components.

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
- **Precompose inherits from the live source panel.** Its dedicated
  `CascadeFrom` relationship preserves later panel and global changes; only
  font unit, lighting, and HDR coverage bias are intentional helper-local
  overrides.
- **Private authored slots default to `Cascade::Inherit`, never
  `Override(On)`,** so inheritance stays live. Public APIs express this through
  domain verbs and do not expose `Cascade<T>`.
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
- **`SurfaceShadow`** (`crates/hana_diegetic/src/panel/coordinate_space.rs`,
  `enum SurfaceShadow { Off, On }`) still exists but is compatibility-only:
  bidirectional `From<SurfaceShadow>` / `From<ShadowCasting>`,
  `From<SurfaceShadow> for VisualShadow`, and the panel builder
  `surface_shadow()` adapter. New code should use `ShadowCasting`.
- **Raw shared components are not re-exported by `hana_diegetic`** — use the
  typed authoring verbs and `resolved_*` readers (`resolved_shadow_casting`,
  `resolved_glyph_shadow_mode`, etc.).

## Why

- **The private authored state distinguishes `Inherit` from `Override(On)`
  deliberately.** If every node stored a concrete `ShadowCasting::On`, a later
  change to a parent's or the global default would be silently blocked because
  each descendant would already carry a winning local value. `Inherit` keeps
  resolution live so parent/global changes still propagate.
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
