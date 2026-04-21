# Cascade / `Resolved<T>` — unified override-resolution architecture

## Why this exists

The crate has several attributes that cascade through override tiers before a final value is used to render:

| attribute | tier 1 (entity) | tier 2 (panel) | tier 3 (global) |
|---|---|---|---|
| panel-text alpha mode | `PanelTextChild.alpha_mode: Option<AlphaMode>` | `DiegeticPanel.text_alpha_mode: Option<AlphaMode>` | `TextAlphaModeDefault` resource |
| standalone-world-text alpha mode | `WorldTextStyle.alpha_mode(): Option<AlphaMode>` | — | `TextAlphaModeDefault` resource |
| panel-text font unit | — | `DiegeticPanel.font_unit: Option<Unit>` | `UnitConfig.font: Unit` |
| standalone-world-text font unit | `WorldTextStyle.unit(): Option<Unit>` | — | `UnitConfig.world_font: Unit` |

Each is implemented ad hoc with `Option<T>.or(...).unwrap_or(global)` computed inline at read time. None stores the resolved value, so Bevy's `Changed<T>` cannot observe a tier change and propagate. The practical bug: mutating a higher tier at runtime does not invalidate already-rendered downstream state. Today this is patched for alpha mode only, via a synthetic marker component (`StaleAlphaMode`). The other attributes silently have the same bug class.

The fix is not more markers. The fix is one reusable pattern: **store the resolved value per entity in a `Resolved<A>` component, maintain that cache with a small set of generic observers/systems, and let Bevy's native `Changed<T>` fan the invalidation out to readers.**

Readers always query `&Resolved<A>` directly (where `A` is the attribute newtype — e.g., `Resolved<PanelTextAlpha>`). No inline resolution. No per-attribute ad-hoc propagation.

## Non-goals

- No new cascade tiers (no new sub-panel levels, no style inheritance chain between `El` nodes).
- No generalization to attributes that currently have zero real cascade (e.g., `LayoutUnit` — each panel owns its own; `UnitConfig.layout` is a construction default only).
- No generalization to material overrides (`text_material`, `material`) until users ask for runtime-changeable global material defaults.

## Scope — four attributes

Panel text and standalone world text are distinct populations (enforced today via `Without<PanelTextChild>` filters). They carry the same logical attribute (alpha mode, font unit) but with **different cascade shapes** because standalone world text has no panel tier. This plan splits each alpha/font concept into a panel attribute and a world attribute.

| attribute | trait | `Resolved<A>` lives on |
|---|---|---|
| `PanelTextAlpha` | `CascadePanelChild` | **the panel** (caches "default value for this panel's children") AND **each text child** (caches final rendered value) |
| `WorldTextAlpha` | `CascadeEntity` | the standalone world-text entity |
| `PanelFontUnit` | `CascadePanel` | the panel entity |
| `WorldFontUnit` | `CascadeEntity` | the standalone world-text entity |

For 3-tier cascades, **both** the panel and its text children carry `Resolved<A>`. The panel's `Resolved<A>` represents the effective "default my children inherit" — computed as `panel.override.unwrap_or(global_default)`. Each child's `Resolved<A>` is computed as `child.override.unwrap_or(panel.Resolved<A>)`. Change detection fires naturally at both levels.

Three cascade topologies → three generic trait/plugin pairs. `CascadePanelChild` for child-of-panel (3-tier), `CascadePanel` for panel-targeted (2-tier), `CascadeEntity` for standalone-entity-targeted (2-tier). `CascadePanel` and `CascadeEntity` share mechanical implementation but are distinct types so each registration line makes its topology explicit.

## Why change detection uses systems (not observers) and native `Changed<T>` (not synthetic shadows)

Bevy 0.18 lifecycle observers (`On<Add>`, `On<Insert>`, `On<Remove>`, `On<Despawn>`, `On<Discard>`) fire on **component-level** events: the whole component added, replaced, or removed. They do **not** fire on **field-level** mutations of an existing component, and they don't fire on resource mutations at all.

The refactor uses observers for two things only: **spawn initialization** (`On<Add, A::Override>`, `On<Add, A::PanelOverride>`). Everything else polls.

| detection | polls | why observers can't do this |
|---|---|---|
| global default mutation (e.g., `defaults.text_alpha = Opaque`) | `CascadeDefaults` + per-attribute sentinel | no observer exists for resource mutation in 0.18 |
| panel override mutation (e.g., `panel.text_alpha_mode = Some(Opaque)`) | `Changed<A::PanelOverride>` query filter | in-place field mutation fires no `On<Insert>` — the component wasn't re-inserted, just mutated |
| panel's resolved value transitioning (downstream propagation) | `Changed<Resolved<A>>` query filter | native Bevy change detection — no shadow needed |

**Native `Changed<Resolved<A>>` replaces the need for a diff shadow.** Earlier drafts used a `LastSeen<A>` sidecar on panels to detect "did tier-2 actually change, or was some other field mutated?" — needed because `Changed<DiegeticPanel>` fires for any field mutation. In the current design, the panel's *own* `Resolved<A>` is the cached value-we-care-about; its `Changed<Resolved<A>>` signal is precise by construction (it only fires when the resolved value actually transitions to a new value). No shadow, no `==` dance, no sidecar component.

**The tier-3 sentinel is still needed.** It's a consequence of the one-resource design: `CascadeDefaults` bundles every global into one struct, so `Res<CascadeDefaults>.is_changed()` can only say "some field changed" — not which. The sentinel projects to the specific field each cascade cares about. An alternative design — one resource per default (`TextAlphaDefault(AlphaMode)`, `PanelFontUnitDefault(Unit)`, etc.) — would give precise `Res<T>.is_changed()` without any sentinel. We accept the sentinel because:

- One `CascadeDefaults` type doubles as a discoverability manifest.
- Users migrating from `UnitConfig` / `TextAlphaModeDefault` replace two resources with one.
- The per-sentinel cost is trivial (a byte-sized `Local` + one equality check per frame per cascade).

A future maintainer valuing Bevy-native change detection over the single-type manifest would split `CascadeDefaults` into per-field resources and delete the sentinels.

**Why spawn gets observers.** `On<Add, A::Override>` fires exactly once per entity, synchronously during command flush, so `Resolved<A>` is populated before any reader runs. Polling on spawn would be a frame late.

## Public `SystemSet` for sequencing

Users who need their systems to read freshly-propagated `Resolved<A>` values schedule against a public set:

```rust
/// Public system-set handle for bevy_diegetic cascade propagation.
/// Users schedule `.after(CascadeSet::Propagate)` to guarantee they observe
/// propagated values within the same frame.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CascadeSet {
    /// All systems that propagate cascade changes into per-entity
    /// `Resolved<A>` values. Covers both tier-2 propagation (panel
    /// override mutations, detected via `Changed<A::PanelOverride>`) and
    /// tier-3 propagation (global-default mutations, detected via
    /// `CascadeDefaults`). After this set runs in `Update`, every
    /// `Resolved<A>` on affected entities reflects current sources.
    Propagate,
}
```

Every cascade plugin registers its propagation systems into this set:

```rust
app.add_systems(Update, (
    reconcile_panel_resolved::<A>,
    propagate_panel_to_children::<A>,
).in_set(CascadeSet::Propagate));
```

One set covers both tier-2 and tier-3 polling systems. Observers are not in any set (they fire synchronously at command-flush, not on a schedule).

## Module-level rustdoc — the single entry-point for the system

The entire cascade machinery lives in a dedicated module (working name `src/cascade/mod.rs`). Its `//!` doc comment is the single place where the overall flow is documented. A new contributor reading the source from top finds the whole picture in one doc block, then drops down into the individual types and functions.

The module rustdoc must cover:

1. **Purpose and problem.** What cascading attributes are. What today's ad-hoc inline-resolution costs. Why a cache plus change-detection replaces markers.
2. **Three topologies.** `CascadePanelChild` (3-tier), `CascadePanel` (2-tier on panel), `CascadeEntity` (2-tier on any entity). When each applies. An attribute picks exactly one.
3. **Write paths** (listed with their triggers and schedules), referring to the types/systems by name:
   - spawn-time `On<Add, A::Override>` observer → initial `Resolved<A>` insert
   - for 3-tier: spawn-time `On<Add, A::PanelOverride>` observer → initial panel-level `Resolved<A>`
   - for 3-tier: `reconcile_panel_resolved` system → keeps panel's `Resolved<A>` in sync with both sources (panel's override + `CascadeDefaults`)
   - for 3-tier: `propagate_panel_to_children` system → on `Changed<Resolved<A>>` on panels, fans out to non-tier-1 children
   - for 2-tier: `propagate_global_default_to_entity` system → keeps target entity's `Resolved<A>` in sync with `CascadeDefaults`
   - tier-1 re-resolve folded into the existing reader systems (`shape_panel_text_children`, `render_world_text`)
4. **The read path.** Readers query `&Resolved<A>` and filter on `Changed<Resolved<A>>`. No inline resolution, ever.
5. **Links by cross-reference** to the load-bearing doc comments:
   - `CascadeDefaults` — manifest vs. per-field-resource tradeoff
   - `should_propagate_defaults` — the sentinel pattern
   - `reconcile_panel_resolved` + `propagate_panel_to_children` — the two-step 3-tier propagation path
   - `CascadeSet::Propagate` — the system-set users sequence against
6. **The two invariants** contributors must honor:
   - A `DiegeticPanel` mutator must not incidentally touch the tier-2 override field for any `CascadePanelChild` attribute.
   - A reader's tier-1 re-resolve always calls the full `resolve_*` helper, never shortcuts to the entity override alone.

The module rustdoc is the first artifact written in phase 1 (before the types). When a PR adds a new cascade attribute, the doc gets a new line under "Three topologies" showing which trait the attribute implements. When the design changes — e.g., splitting `CascadeDefaults` into per-field resources — the rustdoc changes too; the doc is the contract, not a snapshot.

## Core types

### `Resolved<T>` component

```rust
/// Per-entity cache of a resolved cascading attribute.
/// Maintained by `CascadePanelChildPlugin<A>`, `CascadePanelPlugin<A>`, or `CascadeEntityPlugin<A>`.
/// Crate-internal; readers inside the crate query `&Resolved<A>`.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Resolved<A: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static>(pub A);
```

**`FromReflect` is required on the bound**, not just `Reflect`. The `#[derive(Reflect)]` macro auto-adds per-field bounds `A: FromReflect + TypePath + MaybeTyped + RegisterForReflection`. `FromReflect` is a separate trait; writing only `Reflect` in the bound is a compile-break. `#[derive(Reflect)]` on a concrete type auto-derives `FromReflect` unless `#[reflect(from_reflect = false)]` is set, so the attribute newtypes (`PanelTextAlpha` etc.) get it for free — but the generic container `Resolved<A>` must spell it out. Same for `LastSeen<A>` (its field type `Option<A>` needs `A: FromReflect`).

Per-monomorphization type registration is required: each `CascadePlugin*` calls `app.register_type::<Resolved<A>>()` in `build`. Without this, BRP and the typography overlay can't see the resolved values. See "Plugin build" below.

### `CascadeDefaults` resource

Replaces `UnitConfig` and `TextAlphaModeDefault`. Holds all global cascade defaults plus the non-cascading `layout_unit` for discoverability. The `Local<Option<A>>` sentinel in each propagate-defaults system ignores changes to fields unrelated to that cascade, so grouping non-cascading globals here has no runtime cost.

```rust
/// Global defaults for every cascading attribute the crate honors.
///
/// # Design: why one resource, not several
///
/// Every global default lives here so the type doubles as a discoverability
/// manifest — a new contributor sees the full global surface of the crate
/// by reading one struct. Users setting defaults make one `insert_resource`
/// call instead of several.
///
/// The cost of consolidation: `Res<CascadeDefaults>.is_changed()` only tells
/// us "some field changed," not which. Each cascade's propagate-defaults
/// system therefore uses a `Local<Option<A>>` sentinel that projects to the
/// specific field that cascade cares about, so mutations to unrelated fields
/// don't wake unrelated cascades.
///
/// The alternative — one resource per default (`TextAlphaDefault`,
/// `PanelFontUnitDefault`, etc.) — would give precise `Res<T>.is_changed()`
/// without any sentinel. That choice is not wrong; it trades the manifest
/// for native change-detection. At current scale (four cascades, a byte of
/// sentinel per cascade, one equality check per frame) the sentinel cost
/// is trivial. At ~15 cascades a maintainer should re-evaluate.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource)]
pub struct CascadeDefaults {
    /// Global fallback for text alpha mode (both panel text and standalone world text).
    pub text_alpha:      AlphaMode,
    /// Global fallback for panel-text font unit (used by `DiegeticPanel.font_unit`).
    pub panel_font_unit: Unit,
    /// Global fallback for standalone-world-text font unit.
    pub world_font_unit: Unit,
    /// Default `layout_unit` for newly-built panels. Read at panel construction;
    /// NOT cascade-propagated at runtime (mutating it doesn't affect existing panels).
    pub layout_unit:     Unit,
}

impl Default for CascadeDefaults {
    fn default() -> Self {
        Self {
            text_alpha:      AlphaMode::Blend,
            panel_font_unit: Unit::Points,
            world_font_unit: Unit::Meters,
            layout_unit:     Unit::Meters,
        }
    }
}
```

**Resource registration contract:** the root plugin (`DiegeticUiPlugin`) calls `app.init_resource::<CascadeDefaults>()` exactly once, **before** any `Cascade*Plugin` adds its propagate-defaults system. Users' `insert_resource(CascadeDefaults { ... })` in `Startup` replace the init'd default; the sentinel sees the replacement and triggers the initial cascade.

### Traits

#### 3-tier (entity → panel → global)

```rust
pub(crate) trait CascadePanelChild: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static {
    /// Tier-1 override component on the entity.
    type EntityOverride: Component;
    /// Tier-2 override component on the parent panel.
    type PanelOverride: Component;

    fn entity_value(c: &Self::EntityOverride) -> Option<Self>;
    fn panel_value(c: &Self::PanelOverride) -> Option<Self>;
    fn global_default(d: &CascadeDefaults) -> Self;
}
```

#### 2-tier on the panel entity itself (`CascadePanel`)

```rust
pub(crate) trait CascadePanel: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static {
    /// Component on the panel entity carrying the tier-1 override.
    type Override: Component;

    fn override_value(c: &Self::Override) -> Option<Self>;
    fn global_default(d: &CascadeDefaults) -> Self;
}
```

`Resolved<A>` for a `CascadePanel` is written onto the panel entity itself (the entity that owns `A::Override`).

#### 2-tier on an arbitrary target entity (`CascadeEntity`)

```rust
pub(crate) trait CascadeEntity: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static {
    /// Component on the target entity carrying the tier-1 override.
    type Override: Component;

    fn override_value(c: &Self::Override) -> Option<Self>;
    fn global_default(d: &CascadeDefaults) -> Self;
}
```

`Resolved<A>` for a `CascadeEntity` is written onto the target entity itself. Note: `CascadePanel` and `CascadeEntity` have identical shape; they are two traits (not one with a phantom discriminant) so each attribute's registration site names its exact topology. The underlying machinery is shared internally.

#### Uniform resolvers

```rust
fn resolve_panel_child<A: CascadePanelChild>(
    entity: &A::EntityOverride,
    panel: Option<&A::PanelOverride>,
    defaults: &CascadeDefaults,
) -> A {
    A::entity_value(entity)
        .or_else(|| panel.and_then(A::panel_value))
        .unwrap_or_else(|| A::global_default(defaults))
}

fn resolve_panel<A: CascadePanel>(over: &A::Override, defaults: &CascadeDefaults) -> A {
    A::override_value(over).unwrap_or_else(|| A::global_default(defaults))
}

fn resolve_entity<A: CascadeEntity>(over: &A::Override, defaults: &CascadeDefaults) -> A {
    A::override_value(over).unwrap_or_else(|| A::global_default(defaults))
}
```

## Attribute definitions

### `PanelTextAlpha` (`CascadePanelChild`)

```rust
#[derive(Clone, Copy, Reflect)]
pub(crate) struct PanelTextAlpha(pub AlphaMode);

impl CascadePanelChild for PanelTextAlpha {
    type EntityOverride = PanelTextChild;
    type PanelOverride = DiegeticPanel;

    fn entity_value(c: &PanelTextChild) -> Option<Self> { c.alpha_mode.map(PanelTextAlpha) }
    fn panel_value(c: &DiegeticPanel) -> Option<Self> { c.text_alpha_mode.map(PanelTextAlpha) }
    fn global_default(d: &CascadeDefaults) -> Self { PanelTextAlpha(d.text_alpha) }
}
```

### `WorldTextAlpha` (`CascadeEntity`)

```rust
#[derive(Clone, Copy, Reflect)]
pub(crate) struct WorldTextAlpha(pub AlphaMode);

impl CascadeEntity for WorldTextAlpha {
    type Override = WorldTextStyle;

    fn override_value(c: &WorldTextStyle) -> Option<Self> { c.alpha_mode().map(WorldTextAlpha) }
    fn global_default(d: &CascadeDefaults) -> Self { WorldTextAlpha(d.text_alpha) }
}
```

### `PanelFontUnit` (`CascadePanel`)

```rust
#[derive(Clone, Copy, Reflect)]
pub(crate) struct PanelFontUnit(pub Unit);

impl CascadePanel for PanelFontUnit {
    type Override = DiegeticPanel;

    fn override_value(c: &DiegeticPanel) -> Option<Self> { c.font_unit.map(PanelFontUnit) }
    fn global_default(d: &CascadeDefaults) -> Self { PanelFontUnit(d.panel_font_unit) }
}
```

### `WorldFontUnit` (`CascadeEntity`)

```rust
#[derive(Clone, Copy, Reflect)]
pub(crate) struct WorldFontUnit(pub Unit);

impl CascadeEntity for WorldFontUnit {
    type Override = WorldTextStyle;

    fn override_value(c: &WorldTextStyle) -> Option<Self> { c.unit().map(WorldFontUnit) }
    fn global_default(d: &CascadeDefaults) -> Self { WorldFontUnit(d.world_font_unit) }
}
```

## Plugin registration manifest

The root `DiegeticUiPlugin::build` registers each cascade exactly once. The registration list doubles as a human-readable manifest of every active cascade and its topology:

```rust
app.add_plugins((
    CascadePanelChildPlugin::<PanelTextAlpha>::default(),
    CascadeEntityPlugin::<WorldTextAlpha>::default(),
    CascadeEntityPlugin::<WorldFontUnit>::default(),
    CascadePanelPlugin::<PanelFontUnit>::default(),
));
```

## Plugins

### `CascadePanelChildPlugin<A>` build

```rust
pub(crate) struct CascadePanelChildPlugin<A: CascadePanelChild>(PhantomData<A>);

impl<A: CascadePanelChild> Default for CascadePanelChildPlugin<A> {
    fn default() -> Self { Self(PhantomData) }
}

impl<A: CascadePanelChild> Plugin for CascadePanelChildPlugin<A> {
    fn build(&self, app: &mut App) {
        app.register_type::<Resolved<A>>()
           .add_observer(on_panel_added::<A>)
           .add_observer(on_panel_child_added::<A>)
           .add_systems(Update, (
               reconcile_panel_resolved::<A>,
               propagate_panel_to_children::<A>,
           ).in_set(CascadeSet::Propagate));
    }
}
```

Writer entry points:

1. **`on_panel_added::<A>`** — `On<Add, A::PanelOverride>` observer. When the panel spawns, computes the initial panel-level `Resolved<A>` from the panel's override and the global default; inserts it on the panel.
   ```rust
   fn on_panel_added<A: CascadePanelChild>(
       trigger: On<Add, A::PanelOverride>,
       panels: Query<&A::PanelOverride>,
       defaults: Res<CascadeDefaults>,
       mut commands: Commands,
   ) {
       let Ok(panel_over) = panels.get(trigger.entity) else { return };
       let resolved = A::panel_value(panel_over).unwrap_or_else(|| A::global_default(&defaults));
       commands.entity(trigger.entity).insert(Resolved(resolved));
   }
   ```

2. **`on_panel_child_added::<A>`** — `On<Add, A::EntityOverride>` observer. Reads the child's tier-1 override and the parent panel's **raw** `A::PanelOverride` (not its `Resolved<A>`); inserts the child's `Resolved<A>`.
   ```rust
   fn on_panel_child_added<A: CascadePanelChild>(
       trigger: On<Add, A::EntityOverride>,
       children: Query<(&A::EntityOverride, &ChildOf)>,
       panel_overrides: Query<&A::PanelOverride>,
       defaults: Res<CascadeDefaults>,
       mut commands: Commands,
   ) {
       let Ok((child_over, child_of)) = children.get(trigger.entity) else { return };
       let panel_value = panel_overrides.get(child_of.parent()).ok().and_then(A::panel_value);
       let resolved = A::entity_value(child_over)
           .or(panel_value)
           .unwrap_or_else(|| A::global_default(&defaults));
       commands.entity(trigger.entity).insert(Resolved(resolved));
   }
   ```
   **Why read `A::PanelOverride` and not the panel's `Resolved<A>`:** if panel and child are spawned in the same command batch, `on_panel_added`'s queued `insert(Resolved(...))` for the panel hasn't flushed yet when the child's observer fires — the child's query would see no `Resolved<A>` on the parent and silently fall through to the global default, potentially wrong. The panel's raw `A::PanelOverride` component, however, was inserted atomically with the panel entity's spawn and is visible to the child observer's query immediately. Resolving the chain from raw inputs makes this observer race-free regardless of spawn ordering.

3. **`reconcile_panel_resolved::<A>`** — system in `Update`. Keeps each panel's own `Resolved<A>` in sync with its two input sources (the panel's override field and `CascadeDefaults`). Runs every frame, does real work only when either source changed:
   ```rust
   fn reconcile_panel_resolved<A: CascadePanelChild>(
       defaults: Res<CascadeDefaults>,
       mut sentinel: Local<Option<A>>,
       panels_changed: Query<Entity, Changed<A::PanelOverride>>,
       all_panels: Query<(Entity, &A::PanelOverride, &Resolved<A>)>,
       mut commands: Commands,
   ) {
       let current_global = A::global_default(&defaults);
       let global_changed = should_propagate_defaults(current_global, &mut sentinel);

       let update = |panel_e: Entity, panel_over: &A::PanelOverride, old: &Resolved<A>, commands: &mut Commands| {
           let new = A::panel_value(panel_over).unwrap_or(current_global);
           if old.0 != new {
               commands.entity(panel_e).insert(Resolved(new));
           }
       };

       if global_changed {
           // Global shifted — every panel may need a refresh.
           for (panel_e, panel_over, old) in &all_panels {
               update(panel_e, panel_over, old, &mut commands);
           }
       } else {
           // Only panels whose override field was mutated.
           for panel_e in &panels_changed {
               let Ok((_, panel_over, old)) = all_panels.get(panel_e) else { continue };
               update(panel_e, panel_over, old, &mut commands);
           }
       }
   }
   ```
   When this system writes a new `Resolved(...)` to a panel, native `Changed<Resolved<A>>` fires on that panel, which is the trigger for the next system.

4. **`propagate_panel_to_children::<A>`** — system in `Update`. Walks panels whose `Resolved<A>` changed; for each, updates every non-tier-1 child's `Resolved<A>`:
   ```rust
   fn propagate_panel_to_children<A: CascadePanelChild>(
       panels: Query<(&Resolved<A>, &Children), Changed<Resolved<A>>>,
       children: Query<(&A::EntityOverride, &Resolved<A>)>,
       mut commands: Commands,
   ) {
       for (panel_resolved, kids) in &panels {
           for &child_e in kids.iter() {
               let Ok((child_over, old)) = children.get(child_e) else { continue };
               if A::entity_value(child_over).is_some() { continue; }  // tier-1 wins
               let new = panel_resolved.0;
               if old.0 != new {
                   commands.entity(child_e).insert(Resolved(new));
               }
           }
       }
   }
   ```
   Native `Changed<Resolved<A>>` on the panel is the signal. No shadow, no sentinel — the panel's own cached value **is** the change source. When a child's `Resolved<A>` updates, that fires `Changed<Resolved<A>>` on the child, which the reader system (`shape_panel_text_children`) picks up via its own query filter.

### Shared `should_propagate_defaults` helper

Every propagate-defaults system uses the same sentinel pattern. Factored into one private helper so the pattern has exactly one source-level occurrence (and one place to document it):

```rust
/// Sentinel-gated check for whether a propagate-defaults system should run.
///
/// Each propagate-defaults system holds a `Local<Option<A>>` that remembers
/// the last-seen value of its cascade's global default. This helper returns
/// true only when the current value differs from the sentinel, and updates
/// the sentinel in place.
///
/// Exists because `CascadeDefaults` bundles every global default into one
/// struct, so `Res<CascadeDefaults>.is_changed()` can only say "some field
/// changed," not which. Projecting to the specific field each cascade cares
/// about and comparing against a sentinel gives per-field precision without
/// splitting `CascadeDefaults` into per-field resources. See the doc comment
/// on `CascadeDefaults` for the full design tradeoff.
fn should_propagate_defaults<A: Copy + PartialEq>(
    current: A,
    last_seen: &mut Option<A>,
) -> bool {
    if *last_seen == Some(current) {
        return false;
    }
    *last_seen = Some(current);
    true
}
```

Adding a new cascade type adds one `if !should_propagate_defaults(...) { return; }` line; no sentinel plumbing to re-derive.

### Per-entity change guard: don't write unchanged `Resolved<A>`

Every propagator (3-tier and 2-tier) queries the old `Resolved<A>` and compares before writing. An unconditional insert would fire `Changed<Resolved<A>>` even when the value didn't actually change, waking downstream readers for no reason. For the 2-tier case:

```rust
fn propagate_global_default_to_entity<A: CascadeTarget>(
    defaults: Res<CascadeDefaults>,
    mut last_seen: Local<Option<A>>,
    targets: Query<(Entity, &A::Override, &Resolved<A>)>,   // ← &Resolved<A> added
    mut commands: Commands,
) {
    let current = A::global_default(&defaults);
    if !should_propagate_defaults(current, &mut last_seen) { return; }
    for (entity, over, old) in &targets {
        let new = resolve_target::<A>(over, &defaults);
        if old.0 != new {
            commands.entity(entity).insert(Resolved(new));
        }
    }
}
```

`reconcile_panel_resolved` and `propagate_panel_to_children` (3-tier) apply the same pattern — both query the old `Resolved<A>` and skip the insert when the computed value matches. Requires `A: PartialEq` on all three cascade traits — a free addition because every in-scope attribute wraps a `PartialEq` primitive (`AlphaMode`, `Unit`).

### `CascadePanelPlugin<A>` and `CascadeEntityPlugin<A>` build

Both plugins share mechanical structure (2-tier: one target-entity component + global). They are distinct types so each attribute's registration makes its topology explicit — but the internals should not be duplicated.

**Internal helper trait `CascadeTarget`.** Private to the cascade module. Collapses the line-for-line duplication between `CascadePanelPlugin` and `CascadeEntityPlugin` bodies. Both public traits blanket-forward to it:

```rust
/// Private: the mechanical surface shared by `CascadePanel` and `CascadeEntity`.
/// Not exported; users implement `CascadePanel` or `CascadeEntity` directly,
/// and blanket impls give the internal machinery one generic function to
/// parameterize over.
pub(super) trait CascadeTarget: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static {
    type Override: Component;
    fn override_value(c: &Self::Override) -> Option<Self>;
    fn global_default(d: &CascadeDefaults) -> Self;
}

impl<A: CascadePanel> CascadeTarget for A {
    type Override = <A as CascadePanel>::Override;
    fn override_value(c: &Self::Override) -> Option<Self> { <A as CascadePanel>::override_value(c) }
    fn global_default(d: &CascadeDefaults) -> Self { <A as CascadePanel>::global_default(d) }
}

impl<A: CascadeEntity> CascadeTarget for A { /* analogous */ }
```

Note: the two blanket impls compile because `CascadePanel` and `CascadeEntity` have no overlap (nothing will implement both, by convention). If a type tried to implement both, the blanket impls would conflict — this is the intended enforcement of "pick one topology per attribute."

With `CascadeTarget` in place, the two plugin bodies become one-line wrappers around a single generic impl:

```rust
pub(crate) struct CascadePanelPlugin<A: CascadePanel>(PhantomData<A>);
pub(crate) struct CascadeEntityPlugin<A: CascadeEntity>(PhantomData<A>);

impl<A: CascadePanel> Plugin for CascadePanelPlugin<A> {
    fn build(&self, app: &mut App) { build_cascade_target::<A>(app) }
}

impl<A: CascadeEntity> Plugin for CascadeEntityPlugin<A> {
    fn build(&self, app: &mut App) { build_cascade_target::<A>(app) }
}

// One private function, one set of observers/systems.
fn build_cascade_target<A: CascadeTarget>(app: &mut App) {
    app.register_type::<Resolved<A>>()
       .add_observer(on_cascade_target_added::<A>)
       .add_systems(Update, propagate_global_default_to_entity::<A>);
}
```

The observer `on_cascade_target_added::<A>` and the system `propagate_global_default_to_entity::<A>` are also single generic functions — no per-plugin duplication.

Writer entry points (one set of functions, generic over any `CascadeTarget`):

1. **`on_cascade_target_added::<A>`** — `On<Add, A::Override>`. Resolves the 2-tier chain via `resolve_target`; inserts `Resolved<A>` on the entity that owns `A::Override`.
2. **`propagate_global_default_to_entity::<A>`** — system, sentinel-gated (`Local<Option<A>>`), same structure as `CascadePanelChild`'s propagate-defaults system minus the panel-parent walk.

The 2-tier resolver collapses similarly:

```rust
fn resolve_target<A: CascadeTarget>(over: &A::Override, defaults: &CascadeDefaults) -> A {
    A::override_value(over).unwrap_or_else(|| A::global_default(defaults))
}
```

`resolve_panel` and `resolve_entity` are kept as public-facing helpers (they forward to `resolve_target`), but the internal machinery calls `resolve_target` directly.

## Integration: tier-1 re-resolve in existing reader systems

**Tier-1 changes** (e.g., user mutates `PanelTextChild.alpha_mode` at runtime) are picked up by `Changed<A::EntityOverride>` in the existing reader systems.

**Requirement for the reader's tier-1 re-resolve write:** for 3-tier cascades, it must read the parent panel's `Resolved<A>` (not the panel's raw override field) and compute `child.override.unwrap_or(panel.Resolved<A>.0)`. For 2-tier cascades, read `CascadeDefaults` directly. The reader **must** re-resolve even when the child's new `A::entity_value` is `None` — a `Some → None` transition requires the resolver to fall through to the panel's resolved value; skipping the re-resolve on "override is `None`" would silently keep the stale tier-1-winning value.

Concretely:

- `shape_panel_text_children` — for `PanelTextAlpha`. Adds `Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>` for reading the parent panel's cached value (the `With<DiegeticPanel>` filter is **required** — without it the query also matches text child entities, which carry the same component type). Each iteration touching `PanelTextChild` resolves `entity_value(child).unwrap_or(panel_resolved)` before writing the child's `Resolved<A>`. Reader systems must be scheduled `.after(CascadeSet::Propagate)` or run in a later schedule (`PostUpdate`) to see same-frame propagation.
- `render_world_text` — for `WorldTextAlpha`, `WorldFontUnit`. Gains `Res<CascadeDefaults>`; resolves via `resolve_entity`.
- Panel builder / reconcile path — for `PanelFontUnit` (when `DiegeticPanel.font_unit` is mutated).

This is a named integration point, not a "fold in wherever." Reject PRs that shortcut a reader's re-resolve to entity-only.

## Testing

Integration tests exist per attribute. Boilerplate is unacceptable — a shared harness is required before the first test lands.

### `tests/cascade_harness.rs` (new)

```rust
/// Spin up a minimal App with the root plugin and all CascadePlugins.
pub fn app() -> App { ... }

/// Spawn a panel with an optional tier-2 override, and one child with an
/// optional tier-1 override. Returns (panel_entity, child_entity).
pub fn spawn_panel_with_child(
    app: &mut App,
    panel_alpha: Option<AlphaMode>,
    child_alpha: Option<AlphaMode>,
) -> (Entity, Entity) { ... }

/// Read a Resolved<A> value off an entity, after update().
pub fn resolved<A>(app: &App, entity: Entity) -> A
where
    A: Copy + Send + Sync + Reflect + TypePath + 'static
{ ... }

/// Mutate CascadeDefaults via a callback.
pub fn mutate_defaults(app: &mut App, f: impl FnOnce(&mut CascadeDefaults)) { ... }
```

### Test matrix per attribute

For each of `PanelTextAlpha`, `WorldTextAlpha`, `PanelFontUnit`, `WorldFontUnit`:

- Spawn with no overrides → `Resolved<A>` equals global default.
- Spawn with tier-1 override → `Resolved<A>` equals the override; tier-3 mutation doesn't change it.
- (3-tier only) Spawn with tier-2 override → `Resolved<A>` equals tier-2; tier-3 mutation doesn't change it; then set tier-1 and verify tier-1 wins.
- Mutate tier-3 → `Resolved<A>` on no-override entities updates within one frame; entities with any override unchanged.
- (3-tier only) Mutate tier-2 to a new value → children without tier-1 update; children with tier-1 unchanged.
- (3-tier only) Mutate tier-2 to the same value → no child updates (shadow short-circuit test).
- Mutate an unrelated field of `CascadeDefaults` → no `Resolved<A>` writes (sentinel test, verifiable via change-detection sniff).

Each test: ~20 lines thanks to the harness.

## Migration path

### Breaking public-API changes

- `UnitConfig` deleted.
- `TextAlphaModeDefault` deleted.
- `CascadeDefaults` added with four fields (`text_alpha`, `panel_font_unit`, `world_font_unit`, `layout_unit`).
- `lib.rs` re-exports updated: `pub use CascadeDefaults;`.

Migration table:

| before | after |
|---|---|
| `UnitConfig::new().with_font(Unit::Millimeters)` | `CascadeDefaults { panel_font_unit: Unit::Millimeters, ..default() }` |
| `UnitConfig::new().with_world_font(Unit::Points)` | `CascadeDefaults { world_font_unit: Unit::Points, ..default() }` |
| `UnitConfig::new().with_layout(Unit::Inches)` | `CascadeDefaults { layout_unit: Unit::Inches, ..default() }` |
| `TextAlphaModeDefault(AlphaMode::Opaque)` | `CascadeDefaults { text_alpha: AlphaMode::Opaque, ..default() }` |

~10 call sites in repo (examples + tests). Users get clear compile errors.

### Internal deletions

- `StaleAlphaMode` component.
- `queue_alpha_default_refresh` system.
- `DiegeticPanel::resolved_font_unit(&UnitConfig)` method.
- Inline `.or_else(...).unwrap_or(...)` at `src/render/text_renderer.rs:773-775` (alpha) and `src/render/world_text.rs:225,271` (unit, alpha).
- `UnitConfig::font_scale()` and similar methods: audit per phase (some read global, some read panel-resolved — distinction matters once panels can override `font_unit`).

### Call sites to audit (for `UnitConfig` usage)

- `src/render/panel_geometry.rs:110`
- `src/render/text_renderer.rs:330,441`
- `src/render/panel_rtt.rs:128`
- `src/render/world_text.rs:189`

## Phased implementation

| phase | deliverable | risk |
|---|---|---|
| 1 | Framework: `Resolved<T>`, `CascadeDefaults`, `CascadePanelChild` / `CascadePanel` / `CascadeEntity` traits and their plugins, resolvers, `tests/cascade_harness.rs`. Tests: harness round-trip against a throwaway test attribute. | low |
| 2 | `PanelTextAlpha` (3-tier) migration. Delete `StaleAlphaMode` and `queue_alpha_default_refresh`. Readers switch. | medium — validates 3-tier machinery |
| 3 | `WorldTextAlpha` (2-tier entity) migration. | low — mechanical after phase 2 |
| 4 | `PanelFontUnit` (2-tier panel) migration. `DiegeticPanel::resolved_font_unit` deleted. | medium — audit call sites |
| 5 | `WorldFontUnit` (2-tier entity) migration. | low |
| 6 | Documentation, examples migrated, release notes. | low |

Phases 3–6 gated on phase 2 being clean.

## Ordering and re-entrancy

Per-frame execution for a given attribute `A`:

1. **Spawn `Add` observers** — synchronous during command flush (first thing to run when new entities land).
2. **3-tier: `reconcile_panel_resolved`** — runs in `Update`, `CascadeSet::Propagate`. Keeps panel's `Resolved<A>` in sync with both sources (`Changed<A::PanelOverride>` + `should_propagate_defaults` sentinel).
3. **3-tier: `propagate_panel_to_children`** — runs in `Update`, `CascadeSet::Propagate`. On `Changed<Resolved<A>>` on panels, updates children without tier-1.
4. **2-tier: `propagate_global_default_to_entity`** — runs in `Update`, `CascadeSet::Propagate`. Sentinel-gated.
5. **Tier-1 re-resolve** — in the reader's schedule (shape system for alpha in `PostUpdate`). Reads the parent panel's `Resolved<A>` directly (3-tier) or `CascadeDefaults` (2-tier); no tier walking at read time.

**Spawn-ordering invariant (observers).** Bevy 0.18 does not guarantee a deterministic firing order between `on_panel_child_added` for the child and any observer on the parent's `DiegeticPanel` if both fire in the same command flush. Today's crate spawns panels before their children, so when `on_panel_child_added` fires, `ChildOf.parent()` resolves and the parent's `A::PanelOverride` is readable. If a future spawn pattern batches parent and child in one command, `ChildOf → None` can occur and the child falls through to tier-3. Document the spawn-order contract in the cascade module header.

Re-entrancy: if multiple writers fire on one entity in one frame, last-writer-wins. Because `Resolved<A>` is a pure function of its current inputs, every writer produces the same final value regardless of order. This is why `reconcile_panel_resolved` can collapse tier-2 and tier-3 into one system without ordering constraints.

`reconcile_panel_resolved` must run before `propagate_panel_to_children` in the same frame (otherwise the child fan-out uses a stale panel `Resolved<A>`). Enforce with `.before(propagate_panel_to_children::<A>)` on the reconciler.

**Same-frame `Changed<T>` visibility.** Bevy flushes `Commands` buffers between systems within a schedule, so a `.insert(Resolved(new))` issued by `reconcile_panel_resolved` is visible to `propagate_panel_to_children`'s `Changed<Resolved<A>>` filter in the same `Update` run. No explicit `apply_deferred` is needed; the default scheduling provides it. Readers in `PostUpdate` see the final propagated values because of the schedule boundary flush.

## Risks and mitigations

| risk | likelihood | mitigation |
|---|---|---|
| Standalone `WorldText` entities get no writer | would-be critical | Fixed: alpha split into `PanelTextAlpha` + `WorldTextAlpha` |
| Tier-2 in-place mutation doesn't fire `On<Insert, DiegeticPanel>` | would have been critical | Fixed: tier-2 detection is part of `reconcile_panel_resolved` (polling system filtered on `Changed<A::PanelOverride>`) that writes panel's `Resolved<A>`. Downstream propagation uses Bevy-native `Changed<Resolved<A>>`. |
| Spawn order: child before parent panel has tier-2 component | possible | Documented precondition; current crate honors it |
| Reader re-resolve uses only tier-1, misses tier-2/3 | high if implemented naively | Integration point explicitly documented; the appropriate `resolve_*` helper called with full chain; reject PRs that shortcut |
| `CascadeDefaults.is_changed()` fan-out | certain | `Local<Option<A>>` per-attribute sentinel; compares only the relevant field's projection |
| `Changed<A::PanelOverride>` fires on every panel field mutation | certain, small | `reconcile_panel_resolved` computes a fresh `Resolved<A>` for the panel; inequality check skips the insert when the value didn't actually transition. `propagate_panel_to_children` only fires on real `Changed<Resolved<A>>`. Cost per unrelated panel-field mutation: one equality check. **Invariant: no panel mutator incidentally touches the tier-2 field.** |
| `Resolved<A>` missing from type registry | certain if forgotten | `app.register_type::<Resolved<A>>()` in plugin `build`; codified |
| Derive bounds missing `FromReflect` on `Resolved<A>` | compile-break if omitted | Bound on the generic struct includes `FromReflect` explicitly; same on `CascadePanelChild` / `CascadePanel` / `CascadeEntity` supertraits; verified against Bevy 0.18 reflect derive |
| Child observer races with panel observer on same-batch spawn | would have been critical | Fixed: `on_panel_child_added` reads the panel's raw `A::PanelOverride` (present atomically at spawn) rather than its `Resolved<A>` (still queued) |
| Generic trait plumbing fights Bevy type system | moderate | Phase 1 validates with throwaway test attribute |
| Breaking `UnitConfig` / `TextAlphaModeDefault` surprises users | high, intentional | Release notes, migration table, compile errors |
| `DiegeticPanel` future mutator adds `text_alpha_mode` side-effect | low but possible | API review rule: mutating `text_alpha_mode` from another setter is prohibited without discussion |

## Alternatives considered

### Required components (`#[require]`) instead of `On<Add>` observer

Bevy 0.15+ supports `#[require(T = init_fn())]` on a component, causing `T` to be inserted when the requiring component is added. Could replace the Add observers with:

```rust
#[require(Resolved<A> = Resolved(<some default>))]
struct PanelTextChild { ... }
```

**Rejected because** the correct initial value depends on reading the parent `DiegeticPanel` and the `CascadeDefaults` resource — neither is available to a `#[require]` default function, which takes no arguments. A placeholder default (`Resolved(AlphaMode::Blend)`) would be visibly wrong until the first system run. An observer does both the insert and the correct-value compute in one step. Keep the observer.

### Single trait with `type PanelOverride = ()` for 2-tier

Instead of three traits, one trait with `type PanelOverride` allowed to be `()` and a `TIER` const for the 2-tier target. **Rejected** because `()` as a component is awkward (needs a phantom impl), the registration site loses its topology-at-a-glance quality, and three traits is a small, bounded cost. Reconsider if a future cascade needs 4+ tiers.

### Single-resource model (keep `UnitConfig` + `TextAlphaModeDefault`)

Keep today's two resources, add the cascade machinery on top. **Rejected** because having each resource holding its own cascade globals fragments the `CascadeDefaults` contract and complicates future additions. The migration cost is real but one-time.

## Open questions

1. **`Resolved<A>` reflection.** Needed for BRP / typography overlay. `A: Reflect + TypePath` bound. Per-monomorphization `register_type` call in plugin build. Specified above; flagged here for implementation reviewers.
2. **Naming.** Resolved: traits use the `Cascade*` prefix (`CascadePanelChild`, `CascadePanel`, `CascadeEntity`) matching `CascadeDefaults`. Plugins follow Bevy's `*Plugin` suffix (`CascadePanelChildPlugin`, etc.). Attribute newtypes keep domain names (`PanelTextAlpha`, etc.).
3. **Shadow components removed.** The earlier `LastSeen<A>` sidecar is gone in the current architecture; the panel's own `Resolved<A>` serves that role via native `Changed<T>`.
4. **Observer ordering (resolved).** The child observer no longer depends on the panel observer's flush order — it reads the panel's raw `A::PanelOverride` component (visible immediately after bundle spawn) rather than the panel's `Resolved<A>` (still queued). Cross-observer ordering within a single command batch is no longer load-bearing.

## Success criteria

- `Resolved<A>` is the single source of truth for every cascading attribute in scope.
- `StaleAlphaMode`, `queue_alpha_default_refresh`, `DiegeticPanel::resolved_font_unit`, and every inline `Option<T>.or(...).unwrap_or(...)` resolution for in-scope attributes are deleted.
- `CascadeDefaults` replaces `UnitConfig` and `TextAlphaModeDefault`.
- Runtime mutation of any tier propagates within one frame without reshaping text unnecessarily.
- Adding a new cascading attribute is: one attribute newtype + one trait impl + one `Cascade*Plugin::<NewAttribute>::default()` registration.
- `tests/cascade_harness.rs` exists; each attribute has the test matrix above; tests are ~20 lines each.
- `app.register_type::<Resolved<A>>()` verified present for every cascade registered.

## Out of scope

- User-facing material overrides.
- Style inheritance between `El` nodes in the layout tree.
- Deprecation pathway for old resources.
