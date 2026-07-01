# Shadow Casting Cascade

## Current Model

Diegetic shadow casting follows one shared cascade:

```text
global default -> panel -> element -> element-specific override
```

The shared policy type is `ShadowCasting`:

```rust
pub enum ShadowCasting {
    Off,
    On,
}
```

`ShadowCasting::On` is the global default, matching Bevy's default for normal
`Mesh3d` / `MeshMaterial3d<StandardMaterial>` entities. A diegetic thing casts
shadows unless it inherits or authors `ShadowCasting::Off`.

Authored values use `Cascade<T>`:

```rust
pub enum Cascade<T> {
    Inherit,
    Override(T),
}
```

`Cascade::Inherit` means "do not author a local value; keep resolving through
the cascade." `Cascade::Override(value)` means "this node explicitly sets the
value." This distinction matters because storing `ShadowCasting::On` everywhere
would block later parent or global changes.

Runtime ECS propagation still uses the existing cascade components:

```text
Override<T> -> CascadePlugin<T> -> Resolved<T>
```

Public panel, element, and style structs store `Cascade<T>`. Entity readers and
render systems query `Resolved<T>`.

## Applied Cascades

The cascade model is not text-only. These authored values now preserve
inheritance with `Cascade<T>`:

| Area | Authored field | Runtime use |
| --- | --- | --- |
| Panel font unit | `DiegeticPanel::font_unit: Cascade<Unit>` | seeds `Override<Unit>` when authored |
| Panel materials | `material`, `text_material`, `shape_material` as `Cascade<Handle<StandardMaterial>>` | seeds material cascade overrides |
| Panel text alpha | `text_alpha_mode: Cascade<AlphaMode>` | inherited by text children |
| Panel HDR text bias | `hdr_text_coverage_bias: Cascade<f32>` | inherited by text children |
| Element material | `Element::material: Cascade<Handle<StandardMaterial>>` | resolves before SDF/path material rows |
| Element antialias | `Element::anti_alias: Cascade<AntiAlias>` | resolves for SDF records |
| Element hairline fade | `Element::hairline_fade: Cascade<HairlineFade>` | resolves for SDF and path records |
| Panel shadow casting | `DiegeticPanel::shadow_casting: Cascade<ShadowCasting>` | seeds `Resolved<ShadowCasting>` |
| Element shadow casting | `Element::shadow_casting: Cascade<ShadowCasting>` | inherited by element-owned content |
| Text shadow casting | `TextStyle::shadow_casting: Cascade<ShadowCasting>` | combines with glyph shadow mode |
| Text glyph shadow mode | `TextStyle::shadow_mode: Cascade<GlyphShadowMode>` | controls text silhouette contribution |
| Panel-line shadow casting | `LineStyle::shadow_casting: Cascade<ShadowCasting>` | local line override |
| Panel-circle shadow casting | `PanelCircle::shadow_casting: Cascade<ShadowCasting>` | local primitive override |

## Public API

Use `shadow_casting` when the question is "does this thing participate in Bevy
3D shadow casting?"

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

For text, both policies must allow a shadow:

| `ShadowCasting` | `GlyphShadowMode` | Text shadow result |
| --- | --- | --- |
| `Off` | any | no text shadow |
| `On` | `None` | no text shadow |
| `On` | `Cast` | text casts with glyph silhouettes |

`SurfaceShadow` remains only as a compatibility adapter for older panel-surface
call sites. New code should use `ShadowCasting`.

## Render Routing

The renderer converts resolved public policy into the internal batch-key value
`VisualShadow`:

```text
ShadowCasting::On  -> VisualShadow::Cast
ShadowCasting::Off -> VisualShadow::None
```

That value stays in `SdfBatchKey` and `PathBatchKey` because one batch entity
either casts shadows or carries `NotShadowCaster`; mixed shadow participation
must split.

SDF fills and borders resolve `ShadowCasting` from the panel/element cascade.
Panel shapes resolve from shape-local value, then element value, then panel
value, then global default. Text resolves `ShadowCasting` and
`GlyphShadowMode` independently and combines them before constructing the path
batch key.

Image child entities are still not batched. Until the image batching pass, they
read the owning panel's resolved `ShadowCasting` and insert or remove
`NotShadowCaster` directly. They also inherit the owning panel's `RenderLayers`
instead of hard-coding layer 0.

Precompose helper panels copy authored `Cascade::Override` values from the
source panel and preserve `Cascade::Inherit` when the source inherits. They do
not freeze a resolved shadow value into a local override.

## SDF Opaque/Mask Special Case

This is renderer plumbing, not a separate public policy.

For a normal Bevy mesh, `AlphaMode::Opaque` means the whole mesh is solid. For
an SDF panel fill or border, the visible region is computed in the SDF shader.
When an SDF batch is authored as opaque and casts shadows, the shadow/prepass
path still needs the SDF-aware shader path so rounded corners, borders, and
empty fill regions do not cast as one solid rectangle.

The SDF renderer therefore maps:

```text
authored AlphaMode::Opaque + ShadowCasting::On -> batch AlphaMode::Mask(0.0)
otherwise                                      -> authored alpha mode
```

The user's authored material remains opaque. The batch material is adjusted so
Bevy selects a pipeline that can still discard by SDF coverage in shadow and
prepass paths.

## Invalidation Rules

Shadow casting changes are visual/render changes. They must wake routing and
batch-key updates, but they should not force text reshaping or structural
layout rebuilds.

Relevant change inputs include:

- `Changed<Resolved<ShadowCasting>>` for SDF, shape, text, image, and
  precompose routing.
- `Changed<Resolved<GlyphShadowMode>>` for text routing.
- `Changed<RenderLayers>` for image children while images remain entity-based.

## Follow-On

Image batching is still a separate project. The immediate correctness fix is
already in the entity path: image children inherit panel `RenderLayers` and
resolved `ShadowCasting`. The batching pass will replace those child entities
with image records and an image batch key.
