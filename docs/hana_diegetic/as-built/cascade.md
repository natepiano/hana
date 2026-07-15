# Cascade

`hana_diegetic` uses one parent-walking cascade for authored render/layout
properties that should resolve from an entity, then its ancestors, then a root
default.

The public authoring state is:

```rust
pub enum Cascade<T> {
    Inherit,
    Override(T),
}
```

`Cascade::Inherit` means "this panel, element, shape, or text style does not
author a local value." `Cascade::Override(value)` means "this node authors the
local value." This replaces older `Option<T>`-based authoring for cascade
fields. Public structs keep `Cascade<T>` so callers and internal bridges can
distinguish "inherit" from "explicitly author the default value."

The runtime ECS propagation state is separate:

- `Override<A>` is the crate-internal component that stores an entity-local
  authored cascade value.
- `Resolved<A>` is the crate-internal cached output for one entity.
- `CascadeDefault<A>` is the root fallback value.
- `CascadePlugin<A>` seeds and updates `Resolved<A>` for one attribute.

Do not treat public `Cascade<T>` and internal `Override<A>` as interchangeable.
`Cascade<T>` is authoring state stored on panels, elements, and styles.
`Override<A>` is the ECS input component consumed by the propagation pass.

## Files

- `src/cascade/authoring.rs` defines `Cascade<T>`.
- `src/cascade/resolved.rs` defines `CascadeProperty`, `CascadeAttr`,
  `Override<A>`, `Resolved<A>`, `resolve_walk`, and the attribute declarations.
- `src/cascade/defaults.rs` defines `CascadeDefault<A>` and `PanelDefaults`.
- `src/cascade/plugin.rs` defines `CascadePlugin<A>` and the propagation pass.
- `src/cascade/attributes.rs` defines typed entity commands and readers such as
  `override_text_alpha`, `inherit_text_alpha`, `resolved_text_alpha`,
  `override_shadow_casting`, and `resolved_shadow_casting`.

## Resolution Rule

For an entity and attribute `A`, the resolved value is:

1. the entity's own `Override<A>`, when present;
2. otherwise the nearest ancestor value found by walking `ChildOf`;
3. otherwise `CascadeDefault<A>`.

The resolved value is cached as `Resolved<A>`. Propagation writes that component
only when the value changes.

Direct override commands self-heal the target entity during command flush so
the entity has an immediately usable `Resolved<A>`. Descendants observe the new
value after `CascadeSet::Propagate`.

The propagation system must run every frame. Bevy's `RemovedComponents` buffer
is consumed when read, so skipping a frame can miss an `Override<A>` removal.

The parent walk has an explicit depth cap and falls back to the root default if
the hierarchy is malformed. A self-parent, cycle, parentless node, or dangling
`ChildOf` must not hang or panic the resolver.

## Registered Attributes

| Attribute | Payload | Root default | Notes |
| --- | --- | --- | --- |
| `TextAlpha` | `AlphaMode` | `AlphaMode::Blend` | Text alpha mode. |
| `FontUnit` | `Unit` | `Unit::Meters` | Text font-size unit. Panel builders can seed panel-local overrides. |
| `HdrTextCoverageBias` | `f32` | `0.0` | Analytic text coverage adjustment for HDR targets. |
| `SdfMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for SDF backgrounds, borders, and element surfaces. |
| `TextMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for text runs. |
| `ShapeMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for panel-shape primitives. |
| `Lighting` | `Lighting` | `Lighting::Lit` | Text and panel-shape lighting policy. |
| `ShadowCasting` | `ShadowCasting` | `ShadowCasting::On` | Common shadow-casting on/off policy. |
| `GlyphShadowMode` | `GlyphShadowMode` | `GlyphShadowMode::Cast` | Text silhouette shadow policy. |
| `Sidedness` | `Sidedness` | `Sidedness::BothSides` | Text and panel-shape sidedness policy. |
| `AntiAlias` | `AntiAlias` | `AntiAlias::Both` | Mirrors the authored `AntiAlias` resource into the cascade root default. |
| `HairlineFade` | `HairlineFade` | `HairlineFade::Full` | Mirrors `HairlineWidth::fade` into the cascade root default. |

Attributes that are already pure value types use
`cascade_attr!(existing Type, default = ...)`. Attributes that need a wrapper
use `cascade_attr!(Name(Payload), default = ...)`.

## Authoring Storage

Panels store authored defaults as `Cascade<T>`:

- `font_unit`
- `shadow_casting`
- `material`
- `text_material`
- `shape_material`
- `text_alpha_mode`
- `hdr_text_coverage_bias`

Elements store authored values as `Cascade<T>` for:

- `material`
- `anti_alias`
- `hairline_fade`
- `shadow_casting`

Text styles store authored values as `Cascade<T>` for:

- `shadow_mode`
- `shadow_casting`
- `sidedness`
- `lighting`
- `material`
- `unit`
- `alpha_mode`
- `hdr_text_coverage_bias`

Panel-shape authoring uses the same model:

- `LineStyle` stores line material, hairline fade, and shadow casting.
- `PanelCircle` stores circle material, hairline fade, and shadow casting.
- `PanelLine` forwards shadow and fade authoring into its `LineStyle`.

Builder methods set `Cascade::Override(value)`. Newly constructed panels,
elements, and styles default cascade fields to `Cascade::Inherit` unless the
constructor receives an explicit unit-bearing value such as `Pt(...)`.

## Shadow Casting

`ShadowCasting` is the common on/off cascade. Its root default is
`ShadowCasting::On`, matching Bevy mesh behavior.

`GlyphShadowMode` is a separate text-silhouette cascade. Text casts a glyph
silhouette shadow only when both of these are true:

- resolved `ShadowCasting` is `ShadowCasting::On`;
- resolved `GlyphShadowMode` is `GlyphShadowMode::Cast`.

`SurfaceShadow` remains only as compatibility API. It converts to and from
`ShadowCasting`, but new code should author `ShadowCasting` directly.

## Images

Panel images render through the batched image family
(`render/image_batch.rs`). The image router reads the panel's resolved
`ShadowCasting` and `RenderLayers` each frame and hashes them into
`ImageBatchKey`, so images participate in the same panel-level policy as the
rest of the subtree. Image tint is per-record data, not a cascade attribute.

## Invariants

- Use `Cascade<T>` for public authored cascade fields.
- Use `Override<A>` and `Resolved<A>` only for crate-internal ECS propagation.
- Do not use `Option<T>` as the authoring state for a value that needs to
  distinguish inheritance from an explicit default override.
- Do not inline parent/default fallback at render sites. Read `Resolved<A>` or
  call the typed `resolved_*` helper.
- Add new propagated values through an attribute declaration, a
  `CascadeDefault<A>`, `CascadePlugin<A>`, typed commands/readers, and an
  authoring bridge where the public API stores `Cascade<T>`.
- Keep `CascadeSet::Propagate` unskipped so override removals are observed.
