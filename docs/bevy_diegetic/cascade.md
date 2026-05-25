# Cascade unification — one parent-walking hierarchy

Design and phased plan for replacing the cascade module's three fixed-depth topologies
(entity → global, panel → global, child → panel → global) with one parent-walking resolution,
together with the split that moves cascade overrides off `WorldTextStyle` (and the panel/label
components) into one generic override component per attribute — so cascade membership is a property of
the entity tree, never of an incidentally shared component.

## Goal

One cascade. One `Resolved<A>` per logical attribute. One rule, applied by following `ChildOf`:
*my own override, else my parent's `Resolved<A>`, else the global default at the root.* A standalone
text is depth-1 off the root, a panel is depth-1, a panel label is depth-2; deeper nesting needs no
new type.

A node declares an override by carrying `Override<A>` — one generic component per attribute. Because
each attribute has exactly one override component type, and an entity cannot hold two of the same
component, "two overrides for one attribute on one node" has no representation. The `Exclude` marker
is unnecessary by construction.

## The mechanism is attribute-agnostic

Nothing here is specific to text. `CascadeAttr`, `Override<A>`, `Resolved<A>`, the resolution pass,
and the parent-walk are generic over the attribute; `TextAlpha` and `FontUnit` are the first two
attributes. Any value that should resolve *my override, else my parent's, else a global default* plugs
in the same way — a panel background color, a line height — as a new `CascadeAttr` impl plus a field
on `CascadeDefaults`. No new plugin, trait, enum, or topology.

## Attribute value types and the override / resolved pair

The cascade is three generic pieces plus one pure value type per attribute:

```rust
// Pure value types — the cascade attributes. Wrapped in Override<A> / Resolved<A>;
// never inserted bare, so they are not Components themselves.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct TextAlpha(pub AlphaMode);
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct FontUnit(pub Unit);

/// A node's own override for attribute `A`. The type parameter says *what* is
/// overridden; the value is the data. Exactly one component type per attribute, and
/// an entity holds at most one of any component — so "two sources for one attribute"
/// cannot be written down.
#[derive(Component, Clone, Copy, Reflect)]
pub(crate) struct Override<A: CascadeAttr>(pub A);

/// The cached resolved value for `A`. Exactly one per attribute per entity.
#[derive(Component, Clone, Copy, Reflect)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);
```

`Override<A>` is the input, `Resolved<A>` is the output — a matched generic pair. `TextAlpha` /
`FontUnit` stay pure value types, with no double duty as both the override component and the resolved
payload.

## The override accessor

A node's own override is read generically — no per-kind source list, no enum, no match to keep
exhaustive:

```rust
fn override_at<A: CascadeAttr>(node: EntityRef<'_>) -> Option<A> {
    node.get::<Override<A>>().map(|o| o.0)
}
```

Every node kind — standalone, panel, label — declares an override the same way: by carrying
`Override<A>`.

## The split — overrides leave `WorldTextStyle`, `DiegeticPanel`, and `PanelText`

Today alpha/unit overrides live as fields on `WorldTextStyle` (`TextProps<ForStandalone>`), on
`DiegeticPanel`, and on `PanelText`. All three move to `Override<A>`:

- A standalone overrides alpha/unit by carrying `Override<TextAlpha>` / `Override<FontUnit>`, not by
  setting a `WorldTextStyle` field.
- A panel sets the default for the text under it by carrying `Override<TextAlpha>` (its builder
  `DiegeticPanel::screen().text_alpha_mode(x)` inserts the component); children inherit via the
  parent-walk.
- A label's per-run override is an `Override<TextAlpha>` on the label entity, not a `PanelText` field.

Because overrides no longer live on `WorldTextStyle` / `DiegeticPanel` / `PanelText`, none of those is
a cascade source — the cascade reads only `Override<A>`. That is what makes membership a property of
the tree rather than of an incidentally shared component (the original crash was `WorldTextStyle`
shared between standalone text and panel labels).

Two constraints on the split:

- **`unit` has a second consumer.** The layout engine reads `config.unit()` (`layout/element.rs`) for
  point scale, so `unit` stays on `TextProps<ForLayout>` for measurement; only the cascade-override
  role moves to `Override<FontUnit>`.
- **`world_scale` stays put.** A non-cascade field on `WorldTextStyle`, applied by the renderer as a
  post-cascade bypass; `Resolved<FontUnit>` encodes the `Unit` tier only.

## Membership — `Exclude` is gone, by construction

For any attribute `A` there is exactly one override component type, `Override<A>`, and an entity holds
at most one of any component. So "a node carrying two overrides for the same attribute" cannot be
written, and there is no shared multi-role component to mis-enroll. Node *kind* (standalone / panel /
label) is carried by the `WorldText` / `DiegeticPanel` / `PanelChild` markers and matters only for
which render system draws the entity — it is orthogonal to the cascade. `Exclude` / `ExcludeNone` are
deleted.

## Resolution — spawn observers + propagation pass

`Resolved<A>` is cached per entity. One resolver, two triggers:

- **Spawn.** An `On<Add>` observer computes a node's initial `Resolved<A>` by walking up `ChildOf`
  through ancestors' `Override<A>` to the global default — correct at any depth regardless of
  spawn order, because ancestors' `Override<A>` components are already present at command flush even
  before their own `Resolved<A>` exists. The observer inserts synchronously during flush, never as a
  deferred command, so it cannot land on an entity a HUD rebuild freed the same frame — this, together
  with the membership split, is what closes the original crash. The cascade is **add-only**: it never
  runs an `On<Remove>` handler that defers an insert.

- **Change.** A system in `CascadeSet::Propagate` re-resolves when a node's own `Override<A>` changes
  (`Changed<Override<A>>`), when its parent's `Resolved<A>` changes (flowed down roots-first through
  `Children` / `iter_descendants`, reading the parent's cached value), or when `CascadeDefaults`
  changes; it inserts `Resolved<A>` only on a changed value. Because each override is a typed
  component, the trigger is a single `Changed<Override<A>>` per attribute — an in-place `get_mut` edit
  of an override re-resolves. This is the behavior the split would otherwise drop: today's readers
  re-read `WorldTextStyle` every frame, so nothing currently caches the override.

- **Bounded walk.** The parent walk is iterative with an explicit depth cap that terminates at the
  global default on exceed, plus a debug-only visited check, so a self-parent or `ChildOf` cycle
  cannot hang a system. A parentless node, or a dangling `ChildOf` after a parent despawns (Bevy does
  not clear it), terminates at the global default, never a panic.

- **Ordering.** Observers fire during command flush, before `CascadeSet::Propagate`, so a same-batch
  spawn carries `Resolved<A>` before propagation runs. State this invariant in the module so a
  schedule reorder cannot silently break it.

Standalone world text has no parent in the cascade, so it resolves to its own `Override<A>` else the
global default; the propagation pass re-runs it only when its override or the default changes.

## Read side — entity-selection filters stay

`render_world_text` filters `Without<PanelChild>`; the panel-text systems filter `With<PanelChild>`.
These select which entities a render system draws (standalone vs panel text are drawn by different
systems) and are orthogonal to the cascade — keep them. Both read `Resolved<TextAlpha>` /
`Resolved<FontUnit>`. A panel-side override change bumps `Changed<Resolved<TextAlpha>>` and can wake
the standalone render query to a zero-entity no-op (its `Without<PanelChild>` filter) — acceptable;
revisit only if it shows on a profile.

## The `CascadeAttr` trait

```rust
pub(crate) trait CascadeAttr:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    fn global_default(defaults: &CascadeDefaults) -> Self;
}
```

`Override<A>` and `Resolved<A>` are generic over it. The reflection bounds (`FromReflect`, `TypePath`,
`Typed`, `GetTypeRegistration`) are what `register_type::<Override<A>>()` /
`register_type::<Resolved<A>>()` require; a bare `Reflect` bound is insufficient. `TextAlpha` /
`FontUnit` are pure value types (`Clone, Copy, PartialEq, Debug, Reflect`) and are not `Component` —
only `Override<A>` and `Resolved<A>` are.

## Implementation phases

Each phase compiles green and commits as a unit.

### Phase 1 — move overrides into `Override<A>`

- Add `TextAlpha(AlphaMode)` / `FontUnit(Unit)` value types and the generic `Override<A>`.
- Remove `alpha_mode` and the cascade-override `unit` from `TextProps<ForStandalone>`
  (`WorldTextStyle`), and the override fields from `DiegeticPanel` and `PanelText`. Keep `unit` on
  `TextProps<ForLayout>`; keep `world_scale`. Retire `with_alpha_mode` / `with_unit`; update
  `as_standalone()` to stop copying the removed fields; update the panel/label builders to insert
  `Override<TextAlpha>` / `Override<FontUnit>`. Sweep every caller
  (`rg "with_alpha_mode|with_unit|text_alpha_mode|as_standalone"`), including examples and doctests.
- Repoint the existing `WorldTextAlpha` / `WorldFontUnit` / `PanelTextAlpha` cascade reads to source
  from `Override<TextAlpha>` / `Override<FontUnit>` instead of the old component fields.

Commits together: the data move and every reader of the old override path. The existing two/three-tier
cascades still run, now sourcing from `Override<A>`, so behavior is preserved and
`WorldTextStyle` / `DiegeticPanel` / `PanelText` stop being override sources.

### Phase 2 — collapse the three topologies into one parent-walking cascade

- Add the `CascadeAttr` trait (with the reflection bounds above), the generic `Resolved<A>`, and one
  hierarchical cascade plugin: spawn-time `On<Add>` observers for initial resolution + a roots-first
  propagation pass in `CascadeSet::Propagate` gated on `Changed<Override<A>>`, parent
  `Changed<Resolved<A>>`, and `CascadeDefaults` changes; bounded walk (depth cap + debug-only visited
  check). Pair each attribute with `register_type::<Override<A>>()` and `register_type::<Resolved<A>>()`.
- Repoint every reader/filter from the per-role `Resolved<…>` types to `Resolved<TextAlpha>` /
  `Resolved<FontUnit>`: `world_text/mod.rs`, `rendering.rs` (`ChangedWorldTextQuery` and the tier-1
  re-resolve helpers), `panel_text/shaping.rs`, `panel_text/alpha.rs`, and the panel font-unit read.
  Run `rg "Resolved<(World|Panel)(TextAlpha|FontUnit)>"` to confirm none are missed. **Keep** the
  `With` / `Without<PanelChild>` entity-selection filters.
- Delete `WorldTextAlpha` / `PanelTextAlpha` / `WorldFontUnit` / `PanelFontUnit`, the three plugins
  (`CascadeEntityPlugin` / `CascadePanelPlugin` / `CascadePanelChildPlugin`), and `Exclude` /
  `ExcludeNone` plus their test impls.

Commit atomically: the new plugin, the reader repoint, and the old-type deletions are mutually
dependent and cannot land separately and stay green — if both old and new wrote `Resolved` in one
frame they would clobber each other. Assert exactly one cascade plugin is active per attribute.

### Phase 3 — verification + cascade example

- Verify: cross-enrollment is impossible by construction (a standalone and a panel label resolve
  independently); a same-command-batch panel+child spawn resolves in one frame; an in-place `get_mut`
  edit of an `Override<A>` re-resolves; reparenting a child re-resolves against the new parent;
  cycling all alpha modes in `text_alpha.rs` stays correct; a `ChildOf(self)` self-parent and a
  two-node cycle terminate at the global default with no hang or panic.
- Reflection sweep: `rg` for lingering `Resolved<World…>` / `Resolved<Panel…>` references; confirm
  every attribute registers `Override<A>` and `Resolved<A>`.
- Add a cascade demonstration example: one scene where a value resolves at each tier — global default
  → panel override → per-run label override — for both alpha and font unit, with on-screen labels
  showing which tier won.

## Changed names

| Was | Now | Location |
| --- | --- | --- |
| `PanelSlugTextRun` | `PanelText` | `render/panel_text/mod.rs` |
| `render/text_renderer/` module | `render/panel_text/` | — |
| `PanelTextChild` (marker) | `PanelChild` | `render/world_text/mod.rs` (next to `WorldText`) |
| alpha/unit fields on `WorldTextStyle` / `DiegeticPanel` / `PanelText` | `Override<TextAlpha>` / `Override<FontUnit>` components | the override is a generic component, not a field |
| per-role `Resolved<WorldTextAlpha>` / `Resolved<PanelTextAlpha>` / … | `Resolved<TextAlpha>` / `Resolved<FontUnit>` | one resolved type per attribute |
| `WorldTextStyle` = `TextProps<ForStandalone>` | unchanged; **loses** `alpha_mode` / `unit` as cascade overrides | `layout/text_props.rs` |

## Team review (2026-05-25)

Two-cycle team review. Posture: strengthen (intent and approach are a given). No premise-challenge
survived.

### Mechanical (auto-recorded)

- **M1 — trait rename.** Code defines `CascadeAttribute` (`cascade/resolved.rs`); the plan commits to
  `CascadeAttr`. Phase 2 renames `CascadeAttribute` → `CascadeAttr`. Add to the Changed-names table.
- **M2 — Phase 1 sweep is short two conversion methods.** Cycle 2 pinned the exact sites: both
  `TextProps::as_standalone()` (`layout/text_props.rs:598`) and `TextProps::as_layout_config()`
  (`text_props.rs:741`) copy `alpha_mode`/`unit` and will not compile after the field removal; their
  callers `render/panel_text/reconcile.rs:74`, `render/panel_text/shaping.rs:92`, and
  `render/world_text/shaping.rs:62` depend on the fixed methods. `panel/builder.rs` and
  `examples/text_alpha.rs` call `.text_alpha_mode()` on the *builder* (a builder-local field, not a
  component field) — no change there. Enumerate all of these in the Phase 1 sweep.
- **M3 — register the value types.** Phase 2 must `register_type::<TextAlpha>()` /
  `register_type::<FontUnit>()` (i.e. the inner `A`) as well as `Override<A>` / `Resolved<A>`, so
  reflection can serialize the wrapped value. Also: the reflection bounds are confirmed exactly
  sufficient — `FromReflect: Reflect` is a supertrait, so an explicit `Reflect` bound is redundant and
  no `#[reflect(where …)]` is needed on the generic derives; the plan's note stands.

### Proposed user decisions

Cycle 2 reconciliation: D1–D5, D7–D9 confirmed (D1 upgraded to critical; D6 resolved by the type
system — see below). One cycle-2 alarm dropped: a "cleared-then-refilled text same frame leaves
`Resolved<A>` stale" finding — `Resolved<TextAlpha>` / `Resolved<FontUnit>` do not depend on
`WorldText` content, so a text-only change cannot stale them; not a gap.

- **D1 — reparent re-resolve trigger (critical; 3-agent consensus, both cycles).** Propagation triggers
  are `Changed<Override<A>>`, parent `Changed<Resolved<A>>`, and `CascadeDefaults` changes — none fire
  on `Changed<ChildOf>`. A node reparented to a new parent whose `Resolved<A>` equals the old parent's
  keeps a stale value; Phase 3 asserts reparenting re-resolves, so this is a correctness gap, not an
  edge case. Recommendation: add `Changed<ChildOf>` (for entities carrying a cascade marker) as a
  propagation trigger and re-walk on it. status: proposed
- **D2 — override-removal staleness (important).** The cascade is add-only; there is no `On<Remove>`.
  Removing an `Override<A>` (or an ancestor's) leaves `Resolved<A>` at the old override-won value until
  an unrelated change. Cycle-2 recommendation converged: document removal as **unsupported** — to
  toggle an override, mutate it in place (e.g. wrap a `None`) so `Changed<Override<A>>` re-resolves,
  never `remove`; leave a `// TODO: wire On<Remove> if dynamic removal is ever needed`. (The
  alternative is paired `On<Remove>` observers now.) status: proposed
- **D3 — depth cap value + release-build cycle safety (important).** The bounded walk names no cap and
  the visited check is debug-only — a release build hitting a `ChildOf` cycle spins to the cap every
  frame, silently. Cycle-2 recommendation converged: pin `const CASCADE_DEPTH_CAP` (suggested 64–256;
  real depth is <4), keep the visited check debug-only, and **warn-log on exceed in both profiles** so
  a malformed hierarchy is visible. status: proposed
- **D4 — despawn-safety story + correct the "synchronous insert" wording (important).** Cycle 2 read
  the code: the current observers insert via `commands.entity(target).insert(Resolved(..))` — a
  *deferred* command, not the "synchronous during flush" the plan claims. The real crash-closure rests
  on the membership split (overrides now on a non-shared component) plus observer-fire-before-system
  ordering, not on synchronous insert. Recommendation: (a) correct the plan wording — inserts are
  deferred-within-flush and apply before `CascadeSet::Propagate`; (b) state the crash-closure on the
  membership split (unconditionally sound) + ordering, or switch to `world.entity_mut().insert(..)` for
  a truly synchronous insert; (c) add the invariant that the propagation pass does **not** despawn
  entities and that `iter_descendants` tolerates a dangling `ChildOf` (so a same-frame reconcile
  despawn cannot land an insert on a freed/reused id). status: proposed
- **D5 — single-plugin-per-attribute enforcement (important).** Phase 2's atomic commit exists because
  old and new both writing `Resolved` in one frame clobber; the "assert exactly one cascade plugin per
  attribute" has no mechanism, and `register_type` is idempotent so a double-add is silent.
  Recommendation (converged): at plugin build insert a `CascadeAttrMarker<A>(PhantomData)` resource and
  panic with `type_name::<A>()` if it already exists. status: proposed
- **D6 — FontUnit is two attributes, not one (important; resolved by the type system).** Cycle 2 proved
  it: `CascadeDefaults` carries two distinct fields — `world_font_unit` (Meters) and `panel_font_unit`
  (Points) — and `fn global_default(&CascadeDefaults) -> Self` has no context parameter, so a single
  `FontUnit` type cannot return both defaults. `TextAlpha` is genuinely one attribute (world and panel
  share `text_alpha`); font unit is genuinely two. Recommendation: keep `WorldFontUnit` and
  `PanelFontUnit` as separate `CascadeAttr` types (each its own default field); reconcile the plan
  prose and the Changed-names table to stop calling it one `FontUnit`. (Only-if-rejected alternative:
  add context to `global_default`, which sacrifices the clean single-resource seam.) status: proposed
- **D7 — Phase 1 stageability + `PanelText.alpha_mode` boundary (important; sharpened).** Two cycle-2
  agents found Phase 1 may not stage as written: the old `WorldTextAlpha`/`PanelTextAlpha` are
  non-generic `CascadeTarget` impls hard-wired to read *fields* via `override_value`/`panel_value`, so
  "still run, now sourcing from `Override<A>`" needs the read path to locate the new component — either
  a `CascadeTarget` refactor in Phase 1 or an explicit one-commit transient where `Override<A>` is
  inserted but the old fields are still read. And `PanelText.alpha_mode` (`render/panel_text/mod.rs:39`,
  populated at shaping time) needs a boundary: **Option A (recommended)** keep it in Phase 1 populated
  from the panel's override, remove it in Phase 2 in favor of a `Resolved<TextAlpha>` read; **Option B**
  strict split now (partial Phase-2 work in Phase 1). Recommendation: spell out the Phase 1 read-path
  mechanism and pick Option A. status: proposed
- **D8 — `unit` is dual-purpose on `TextProps<ForStandalone>` (minor).** After the override role moves
  to `Override<FontUnit>`, `unit` cannot be deleted — `as_layout_config()` reads it for measurement —
  yet it no longer controls the cascade, so a reader of `TextProps::unit()` is misled. Recommendation:
  document `unit` as layout-measurement-only (rendering-time unit now comes from `Resolved<…FontUnit>`),
  or rename to `layout_unit`. status: proposed
- **D9 — invariant docs + manifest (minor).** Key invariants live only in prose: observer→`Propagate`
  ordering, one registration per monomorphized `A`, the override-mutation timing contract (mutate
  before `Propagate` or via command, never `remove` — see D2). Recommendation: a module-header manifest
  table (attribute → default field → registration site → plugin) plus a code-level assertion/comment
  pinning the schedule-ordering invariant. status: proposed
