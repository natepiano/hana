# Cascade

`hana_diegetic` resolves inherited render and layout properties from an entity,
then its ancestors, then a required root default. The implementation has two
deliberately separate layers:

- `bevy_kana::Cascade<T>` represents one private authored slot as either
  `Inherit` or `Override(T)` and provides storage-independent resolution.
- `bevy_kana` also owns generic ECS propagation over `CascadeFrom`, including
  the `Resolved<A>` cache and lifecycle handling.
- `hana_diegetic` chooses attributes, inserts `CascadeFrom` where diegetic
  inheritance is intended, and exposes domain-specific authoring commands.

`Cascade<T>` is an implementation detail of `hana_diegetic`. It is not
re-exported and does not appear in the crate's public signatures. Callers use
verbs such as `shadow_casting`, `override_text_alpha`, `inherit_text_alpha`,
and `resolved_text_alpha` instead.

## Pure authored state

`crates/bevy_kana/src/cascade.rs` defines:

```rust
pub enum Cascade<T> {
    Inherit,
    Override(T),
}
```

`Inherit` means that this scope contributes no value and resolution should
continue at the next lower-precedence scope. `Override(value)` supplies the
winning value. A required root value completes every resolution, so the stored
state does not need a `Missing` variant.

The authored enum supports inspection, borrowing, transformation, resolving
one layer, and resolving ordered layers from highest to lowest precedence. The
same type can also be inserted as an ECS component for the shared engine. It
intentionally has no `Option<T>` conversions: `Inherit` is authored cascade
state, not a generic missing value.

```rust
use bevy_kana::Cascade;
use bevy_kana::resolve_cascade;

let member = Cascade::Inherit;
let stage = Cascade::Override(0.25_f32);
let sequence_default = 1.0;

assert_eq!(
    resolve_cascade([member, stage], sequence_default),
    0.25,
);
```

## Runtime ECS propagation

`crates/bevy_kana/src/cascade.rs` owns the ECS-specific layer:

- `Cascade<A>` is present on participating entities and stores `Inherit` or a
  local override.
- `CascadeFrom` points at the next inheritance source; Bevy maintains the
  reverse `CascadeChildren` collection without linked despawn.
- `Resolved<A>` is the cached result for one participating entity.
- `CascadeDefault<A>` is the public root-default resource.
- `CascadePlugin<A>` installs initial resolution and later propagation for
  `Resolved<A>`.
- `bevy_kana::CascadeEntityCommandsExt` supplies generic authored-value
  commands; `hana_diegetic::CascadeEntityCommandsExt` supplies typed domain
  adapters.
- typed `resolved_*` readers return the computed domain value without exposing
  raw ECS storage through the `hana_diegetic` API.

For an entity and attribute `A`, resolution chooses:

1. the entity's own `Cascade::Override(value)`, when present;
2. otherwise the first override found by following `CascadeFrom`;
3. otherwise `CascadeDefault<A>`.

An entity without `Cascade<A>` does not participate in that attribute, but it
is transparent when another entity's relationship walk reaches it.
The insertion observer gives a new participant its initial `Resolved<A>` after
companion commands queued for the same spawn settle. `CascadeSet::Propagate`
then handles dirty authored changes, authored or relationship removals,
relationship retargeting, and root changes, writing `Resolved<A>` only when the
result changes. Writes scheduled before that set are observable by readers
scheduled after it in the same update.

The propagation system runs every frame because Bevy's `RemovedComponents`
buffer is consumed when read. Relationship and authored-state removal,
retargeting, root changes, cycles, and excessive depth are handled by the
shared engine.

## Registered attributes

| Attribute | Payload | Root default | Notes |
| --- | --- | --- | --- |
| `TextAlpha` | `AlphaMode` | `AlphaMode::Blend` | Text alpha mode. |
| `FontUnit` | `Unit` | `Unit::Meters` | Text font-size unit. |
| `HdrTextCoverageBias` | `f32` | `0.0` | Analytic text coverage adjustment for HDR targets. |
| `SdfMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for SDF backgrounds, borders, and element surfaces. |
| `TextMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for text runs. |
| `ShapeMaterial` | `Handle<StandardMaterial>` | `Handle::default()` | Source material for panel-shape primitives. |
| `Lighting` | `Lighting` | `Lighting::Lit` | Text and panel-shape lighting policy. |
| `ShadowCasting` | `ShadowCasting` | `ShadowCasting::On` | Common shadow-casting policy. |
| `GlyphShadowMode` | `GlyphShadowMode` | `GlyphShadowMode::Cast` | Text silhouette shadow policy. |
| `Sidedness` | `Sidedness` | `Sidedness::BothSides` | Text and panel-shape sidedness policy. |
| `AntiAlias` | `AntiAlias` | `AntiAlias::Both` | Mirrors the authored `AntiAlias` resource into the root default. |
| `HairlineFade` | `HairlineFade` | `HairlineFade::Full` | Mirrors `HairlineWidth::fade` into the root default. |

`cascade_attribute!` declares wrapper values and each attribute's local
`CascadeRoot` choice. Registration constructs the shared plugin from that root.

## Domain authoring

Panel builders, elements, text styles, lines, and circles privately retain
explicit inherit-or-override construction state. Their public APIs express
intent with domain verbs:

```rust
let panel = DiegeticPanel::world()
    .shadow_casting(ShadowCasting::Off)
    .material(material.clone());

commands
    .entity(text_entity)
    .override_text_alpha(AlphaMode::Add);
commands.entity(text_entity).inherit_text_alpha();
```

Panel builder methods provide one-time construction seeds. On
`Add<DiegeticPanel>`, `seed_panel_overrides` inserts the corresponding
`Cascade<A>` components only when an explicit component command has not already
done so. After spawn, those components are the only live authored state;
runtime changes use typed entity commands such as `override_sdf_material`,
`inherit_text_material`, `override_text_alpha`, and
`inherit_shadow_casting`. Layout-tree replacement, resizing, and other
`DiegeticPanel` changes never replay construction seeds.

Panel font units are the deliberate construction exception: when the builder
omits one, the initial bridge seeds an override from
`PanelDefaults::panel_font_unit`. An explicit `inherit_font_unit` command then
switches the live component to the registered `CascadeDefault<FontUnit>`. A
caller never constructs or matches `Cascade<T>` through the `hana_diegetic`
API.

## Shadow casting

`ShadowCasting` is the common on/off cascade. Its root default is
`ShadowCasting::On`, matching Bevy mesh behavior.

`GlyphShadowMode` is a separate text-silhouette cascade. Text casts a glyph
silhouette shadow only when resolved `ShadowCasting` is `On` and resolved
`GlyphShadowMode` is `Cast`.

`SurfaceShadow` remains compatibility API and converts to and from
`ShadowCasting`. New code should author `ShadowCasting` directly.

Panel images participate in the same panel-level policy. Their router reads the
panel's resolved `ShadowCasting` and `RenderLayers` each frame and includes both
in `ImageBatchKey`. Image tint is per-record data, not a cascade attribute.

## Invariants

- `hana_diegetic` public signatures use domain values and verbs, never
  `bevy_kana::Cascade<T>`.
- Private authored slots use explicit `Inherit` or `Override(T)` state, not
  `Option<T>`.
- `Cascade<A>`, `CascadeFrom`, and `Resolved<A>` are imported privately by
  `hana_diegetic`; the crate does not re-export them.
- Render sites read `Resolved<A>` or use a typed `resolved_*` helper; they do
  not repeat parent/default resolution.
- Panel builder values seed `Cascade<A>` once. After spawn, unrelated
  `DiegeticPanel` changes never rewrite live cascade components.
- New propagated values require an attribute declaration, root default,
  plugin registration, typed commands/readers, and the private authoring
  bridge used by their domain API.
- `CascadeSet::Propagate` remains active every frame so authored-state and
  relationship removals are observed.
