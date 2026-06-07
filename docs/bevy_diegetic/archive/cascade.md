# Cascade unification — one parent-walking hierarchy

> **Archived 2026-06-07 — implemented.** This is the asbuilt design of the
> current `src/cascade/` module: `CascadeAttr`, `Override<A>`, and
> `Resolved<A>` live in `cascade/resolved.rs`, attribute registrations in
> `cascade/attributes.rs`, `CascadeDefaults` in `cascade/defaults.rs`, the
> resolution pass in `cascade/plugin.rs`. The `target.rs` fixed-depth model
> this doc replaces is gone. File-path line numbers below date from the May
> 2026 tree and have drifted; the module structure is current.

Design and phased plan for replacing the cascade module's three fixed-depth topologies
(entity → global, panel → global, child → panel → global) with one parent-walking resolution,
together with the split that moves cascade overrides off `WorldTextStyle`, `DiegeticPanel`, and
`PanelText` into one generic override component per attribute — so cascade membership is a property of
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
and the parent-walk are generic over the attribute. Any value that should resolve *my override, else
my parent's, else a global default* plugs in the same way — a panel background color, a line height —
as a new `CascadeAttr` impl plus a field on `CascadeDefaults`. No new plugin, trait, enum, or topology.

The initial attributes are two (see [Registered attributes](#registered-attributes)): `TextAlpha` and
`FontUnit`.

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

`Override<A>` is the input, `Resolved<A>` is the output — a matched generic pair. The value types stay
pure values, with no double duty as both the override component and the resolved payload.

### One attribute per logical value — including font unit

Both `TextAlpha` and `FontUnit` are single attributes. Standalone world text and panel labels are
drawn by different render systems but cascade the same value type; node *kind* selects the renderer,
not the attribute.

Font unit looked like it needed two types because standalone text and panels want different *defaults*
— world text in `Meters`, panel text in `Points`. It does not. The context difference is carried by the
cascade itself: the single global default (`CascadeDefaults::font_unit`, `Meters`) is the standalone
default, and the panel builder seeds `Override<FontUnit>(Points)` on every panel so everything under a
panel inherits `Points` through the parent-walk. This is the same way a panel seeds `Override<TextAlpha>`
for its subtree.

A consequence, by design: a panel is depth-1 with no `Resolved`-carrying ancestor, so the only way it
gets `Points` is to carry its own `Override<FontUnit>` — the builder therefore seeds it on **every**
panel, unconditionally. So no panel, existing or new, ever reads the cascade global; `font_unit`
governs standalone world text only. Changing `font_unit` at runtime re-resolves standalone text and
does **not** reach panels; a caller who wants panels retuned changes them explicitly (per panel, or the
builder seed). The seed value comes from `CascadeDefaults::panel_font_unit` (`Points`), read once at
panel construction — a construction-time seed, not a cascade global, the same role `layout_unit`
already plays.

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

Today the overrides live as fields: alpha and unit on `WorldTextStyle` (`TextProps<ForStandalone>`),
`text_alpha_mode` and `font_unit` on `DiegeticPanel`, and a per-run `alpha_mode` on `PanelText`. The
cascade-source fields move to `Override<A>`:

- A standalone overrides alpha/unit by carrying `Override<TextAlpha>` / `Override<FontUnit>`, not by
  setting a `WorldTextStyle` field.
- A panel sets the default for the text under it by carrying `Override<TextAlpha>` (its builder
  `DiegeticPanel::screen().text_alpha_mode(x)` records the field; a spawn-time bridge observer inserts
  the component — see [Implementation phases](#implementation-phases) for why emission is a bridge, not a
  `build()` return) and **always** carries `Override<FontUnit>` (the bridge inserts it from
  `CascadeDefaults::panel_font_unit`, or the value the builder was given); children inherit both via the
  parent-walk.
- A label's per-run override is an `Override<TextAlpha>` on the label entity.

Because the cascade no longer reads `WorldTextStyle` / `DiegeticPanel` / `PanelText`, none of those is
a cascade source — the cascade reads only `Override<A>`. That is what makes membership a property of
the tree rather than of an incidentally shared component (the original crash was `WorldTextStyle`
shared between standalone text and panel labels).

Two constraints on the split:

- **`unit` has a second consumer.** The layout engine reads `config.unit()` (`layout/element.rs`) for
  point scale, so `unit` stays on `TextProps<ForLayout>` for measurement; on `TextProps<ForStandalone>`
  the field is documented as layout-measurement-only — its cascade-override role is now
  `Override<FontUnit>`, and render-time unit comes from `Resolved<FontUnit>`.
- **`world_scale` stays put.** A non-cascade field on `WorldTextStyle`, applied by the renderer as a
  post-cascade bypass; `Resolved<FontUnit>` encodes the `Unit` tier only.

## Membership — `Exclude` is gone, by construction

For any attribute `A` there is exactly one override component type, `Override<A>`, and an entity holds
at most one of any component. So "a node carrying two overrides for the same attribute" cannot be
written, and there is no shared multi-role component to mis-enroll. Node *kind* (standalone / panel /
label) is carried by the `WorldText` / `DiegeticPanel` / `PanelChild` markers and matters only for
which render system draws the entity — it is orthogonal to the cascade. `Exclude` / `ExcludeNone` are
deleted.

## Resolution — spawn observer + propagation pass

`Resolved<A>` is cached per entity. One resolver, with a spawn-time path and a change-time path.

- **Spawn.** An `On<Add>` observer computes a node's initial `Resolved<A>` by walking up `ChildOf`
  through ancestors' `Override<A>` to the global default — correct at any depth regardless of spawn
  order, because ancestors' `Override<A>` components are already present at command flush even before
  their own `Resolved<A>` exists. The observer inserts via a deferred command that applies within the
  same flush, before `CascadeSet::Propagate` runs. That is safe because the observer inserts onto the
  just-spawned (live) entity, and overrides now live on a non-shared component (the membership split):
  the insert cannot land on an entity a HUD rebuild freed the same frame. The split plus
  observer-before-system ordering is what closes the original crash. The cascade never runs an
  `On<Remove>` handler that defers an insert; removal is handled read-side, in the propagation pass.

- **Change.** A system in `CascadeSet::Propagate` re-resolves a node when any of these fire: its own
  `Override<A>` changes (`Changed<Override<A>>`, which an in-place `get_mut` edit triggers); its
  `Override<A>` is removed (`RemovedComponents<Override<A>>`, returning the node to inheriting); its
  `ChildOf` changes (`Changed<ChildOf>`, reparenting); its parent's `Resolved<A>` changes (flowed
  roots-first through `Children` / `iter_descendants`, reading the parent's cached value); or
  `CascadeDefaults` changes. It re-resolves through `query.get(entity)` on the live cascade query and
  inserts `Resolved<A>` only on a changed value — a despawned entity returns `Err` and is skipped, so a
  removal or reparent mid-frame can never write to a freed id. Removing an *ancestor's* override
  re-resolves the ancestor, whose `Resolved<A>` change then flows to descendants — no separate trigger
  needed. (This caching is the behavior the split would otherwise drop: today's readers re-read
  `WorldTextStyle` every frame, so nothing currently caches the override.)

  Two constraints follow from `RemovedComponents`, which is a per-read, double-buffered Bevy system
  param: the `CascadeSet::Propagate` system must run **every frame** — no run-condition that can skip
  it, or a frame's removals are cleared unread and the re-resolve is missed — and exactly **one** system
  per attribute reads `RemovedComponents<Override<A>>` (it is consumed on read); any secondary logic
  keys off the `Changed<Resolved<A>>` that system writes.

- **Bounded walk.** The parent walk is iterative with an explicit depth cap (`const CASCADE_DEPTH_CAP`,
  far above the real maximum of ~4) that terminates at the global default on exceed, plus a debug-only
  visited check; on exceed it `warn!`-logs in both debug and release so a malformed hierarchy is
  visible. A self-parent, a `ChildOf` cycle, a parentless node, or a dangling `ChildOf` after a parent
  despawns (Bevy does not clear it) all terminate at the global default — never a hang or panic.

- **No despawn in the pass.** The propagation pass never despawns entities (a module invariant tied to
  the original crash; documented, not enforced by a runtime assert) and the walk tolerates a dangling
  `ChildOf`, so a same-frame reconcile despawn cannot land an insert on a freed or reused id.

- **Ordering.** Observers fire during command flush, before `CascadeSet::Propagate`, so a same-batch
  spawn carries `Resolved<A>` before propagation runs. A code-level comment pins this invariant so a
  schedule reorder cannot silently break it.

Standalone world text and panels have no cascade parent, so they resolve to their own `Override<A>`
else the global default; the propagation pass re-runs them only when their override, their `ChildOf`,
or the default changes.

## Read side — entity-selection filters stay

`render_world_text` filters `Without<PanelChild>`; the panel-text systems filter `With<PanelChild>`.
These select which entities a render system draws (standalone vs panel text are drawn by different
systems) and are orthogonal to the cascade — keep them. Standalone text reads `Resolved<TextAlpha>` and
`Resolved<FontUnit>`; panel labels read `Resolved<TextAlpha>` — inherited from the panel, or the label's
own override when it authored one (`LayoutTextStyle::with_alpha_mode`, restored in Phase 4); panel layout
reads the panel's `Resolved<FontUnit>`. A panel-side `Override<TextAlpha>` change bumps `Changed<Resolved<TextAlpha>>` and
can wake the standalone render query to a zero-entity no-op (its `Without<PanelChild>` filter) —
acceptable; revisit only if it shows on a profile.

## The `CascadeAttr` trait

```rust
pub(crate) trait CascadeAttr:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    fn global_default(defaults: &CascadeDefaults) -> Self;
}
```

`Override<A>` and `Resolved<A>` are generic over it, and a blanket impl covers every value type meeting
the bounds. The reflection bounds (`FromReflect`, `TypePath`, `Typed`, `GetTypeRegistration`) are what
`register_type::<Override<A>>()` / `register_type::<Resolved<A>>()` require; `FromReflect: Reflect` is a
supertrait, so an explicit `Reflect` bound is redundant and the generic derives need no
`#[reflect(where …)]`. The value types are pure values (`Clone, Copy, PartialEq, Debug, Reflect`) and
are not `Component` — only `Override<A>` and `Resolved<A>` are. Reflection of the wrapped value requires
the value type itself to be registered (`register_type::<TextAlpha>()`, `register_type::<FontUnit>()`),
alongside `Override<A>` / `Resolved<A>`.

`global_default` takes only `&CascadeDefaults` and no context, by design: it keeps `CascadeDefaults` the
single resource the cascade reads. A per-context default (panels wanting `Points`) is expressed by a
seeded override on the panel, not by a second attribute — see [the font-unit
note](#one-attribute-per-logical-value--including-font-unit).

## Registered attributes

One manifest of every cascade, mirrored by a module-header table in code:

| Attribute   | Global default                | Resolution                                      | Override source today |
| ---         | ---                           | ---                                             | --- |
| `TextAlpha` | `CascadeDefaults::text_alpha` | label own → panel → global; standalone own → global  | `WorldTextStyle.alpha_mode` (standalone), `DiegeticPanel.text_alpha_mode` (panel), `LayoutTextStyle.alpha_mode` → label `Override<TextAlpha>` at reconcile |
| `FontUnit`  | `CascadeDefaults::font_unit` (standalone) | standalone own → global; panels carry a seeded override (`panel_font_unit`) that children inherit | `WorldTextStyle.unit` (cascade role), `DiegeticPanel.font_unit` |

`CascadeDefaults::panel_font_unit` is the panel builder's construction-time seed for `Override<FontUnit>`
— not a cascade global. `CascadeDefaults::layout_unit` is likewise read once at panel construction and
not cascade-propagated.

## Implementation phases

Each phase compiles green and commits as a unit. The original single "move every cascade field into
`Override<A>`" step is split in two — panel-side first, standalone-side second — for cross-fire
isolation, *not* a difference in authoring mechanism. **Both sides feed `Override<A>` the same way:
through an `On<Add>` authoring-bridge observer.** The cascade-source fields stay on their structs
(`DiegeticPanel`, `WorldTextStyle`) as authoring inputs; the bridge reads them at spawn and inserts
`Override<A>` only when the field is `Some`. The fields stop being a *cascade source* — the cascade
reads `Override<A>` — but they are not deleted (`build()` and `WorldTextStyle::new()` are unchanged).

- The **panel** move introduces `Override<A>` on panel entities only. The standalone cascades still read
  `WorldTextStyle`, so each `Override<A>` is observed by exactly one cascade — no cross-fire.
  Self-contained.
- The **standalone** move makes `Override<A>` shared by the standalone *and* panel cascades while the
  per-role plugins still run, so each is then observed by both. Splitting it into its own phase confines
  that shared-component coexistence to one phase, which the Phase-3 collapse removes.

**Why a spawn bridge and not "`build()` emits the components".** `build()` returns one `DiegeticPanel`
value (and `WorldTextStyle::new()` one `WorldTextStyle`) that callers drop into a *by-value* spawn tuple
— `commands.spawn((Marker, panel, Transform))`, sometimes `.observe(..)`. A returned value has one
compile-time type, so it cannot carry "component present on this call, absent on that call": the only
type that means *maybe a component* is `Option<Override<A>>`, and Bevy does **not** implement `Bundle`
for `Option<C>` (checked in the local `bevy_ecs` tree). Making `build()` carry the on/off decision would
therefore require an `Option` stuffed into a transient carrier component that an observer collapses to
presence/absence — pure plumbing around the value-return. The bridge avoids it: the `Option` stays where
it already lives (the struct field), the observer reads it at spawn, and `build()`,
`WorldTextStyle::new()`, and every call site stay untouched. The conditionality is required by the design
itself, not just these phases — Phase 3 has `.text_alpha_mode(x)` insert `Override<TextAlpha>` only when
called — and the bridge is doubly necessary for `FontUnit`: the Phase-3 always-on panel seed comes from
`CascadeDefaults::panel_font_unit`, a resource the pure `build()` cannot read but the observer can.

### Phase 1 — panel-side: cascade reads `Override<A>`, fed by a spawn bridge ✅ complete

- Add the value types (`TextAlpha`, `FontUnit`) and the generic `Override<A>`. Register
  `Override<TextAlpha>`, `Override<FontUnit>`, and each value type — `Override<A>` is generic, so
  registration is manual, the same as the existing `register_type::<Resolved<A>>()` (see
  [the trait section](#the-cascadeattr-trait)).
- **Keep `text_alpha_mode` / `font_unit` (and their accessors) on `DiegeticPanel`** as authoring inputs.
  The builder's `.text_alpha_mode()` / `.font_unit()` setters, the `build_panel` write, and `build()`'s
  `Result<DiegeticPanel, InvalidSize>` return are all unchanged. What changes: these fields are **no
  longer a cascade source** — the cascade reads `Override<A>` instead.
- Add a **panel authoring bridge** — an `On<Add, DiegeticPanel>` observer (with `Res<CascadeDefaults>`)
  that inserts `Override<TextAlpha>` from `panel.text_alpha_mode()` *only when `Some`* and
  `Override<FontUnit>` from `panel.font_unit()` *only when `Some`*. Field absent → component absent →
  the cascade's "inherit" signal. This is the panel twin of the Phase-2 standalone bridge.
- Repoint only the panel cascades to source from the components: `PanelTextAlpha::PanelOverride =
  Override<TextAlpha>`, `PanelFontUnit::Override = Override<FontUnit>` (`panel_value` / `override_value`
  unwrap `o.0`). The 3-tier and 2-tier topologies keep running; only the override *source* changes from a
  field to a component.
- Leave the standalone cascades (`WorldTextAlpha`, `WorldFontUnit`) untouched — they still read
  `WorldTextStyle.alpha_mode` / `.unit`, so `new(Pt(..))` standalone sizing is unchanged. `Override<A>`
  lives only on panels this phase, so each is observed by exactly one cascade — no cross-firing.
- `PanelText.alpha_mode` stays (a per-run transient set in shaping from `config.alpha_mode()`); the
  label's per-run override moves to `Override<TextAlpha>` in Phase 3.

**Behavior preservation (verified against the code).**

- *No-override panel.* With no `text_alpha_mode`, the panel gets no `Override<TextAlpha>`, so
  `on_panel_added::<PanelTextAlpha>` never fires and the panel has no `Resolved<PanelTextAlpha>`. The
  reader `apply_panel_result` (`render/panel_text/shaping.rs`) already falls back to
  `PanelTextAlpha::global_default` when the panel's `Resolved` is missing, and children resolve via
  `on_panel_child_added` reading the absent parent override → global default. Same effective value as
  today.
- *Runtime global mutation.* `examples/text_alpha.rs` mutates `CascadeDefaults::text_alpha` at runtime
  (`defaults.text_alpha = state.alpha_mode`), but every panel there calls `.text_alpha_mode(Blend)`, so
  all panels carry an override and are unaffected — the runtime mutation drives standalone `WorldText`
  only. Conditional emission does not regress it.
- *Font-unit layout.* `compute_panel_layouts` re-lays-out only on `Ref<DiegeticPanel>::is_changed()` or a
  pending tree change; it does **not** watch `Changed<Resolved<PanelFontUnit>>`. So a runtime
  `panel_font_unit` change does not re-layout panels today either way, and `Override<FontUnit>` being
  conditional (this phase) vs. always-present (Phase 3) is unobservable for layout. The reader
  (`compute_layout.rs:98`) falls back to `PanelFontUnit::global_default` when `Resolved<PanelFontUnit>` is
  absent.
- *Spawn-order race.* The bridge inserts `Override<A>` via a deferred command, one flush behind the
  panel. `PanelText` children are spawned by the shaping system in a later schedule, never co-spawned with
  the panel, so `on_panel_child_added` reading the parent's `Override<TextAlpha>` is race-free.

Commits together: the value types + `Override<A>` + registration, the panel bridge observer, and the two
repointed panel-cascade readers. `DiegeticPanel`'s fields/accessors, the builder, `build()`, and all ~30
panel spawn sites are untouched.

### Retrospective

**What worked:**
- `Override<A>` mirrors `Resolved<A>`'s exact where-clause bounds; compiled first try, with manual
  `register_type` per monomorphization (`Override<TextAlpha>`, `Override<FontUnit>`) + value types as planned.
- The cascade repoint collapses to `Some(o.0.0)` for both `PanelTextAlpha::panel_value` and
  `PanelFontUnit::override_value` — component presence *is* the override signal, so the `Option` is always
  `Some`. All 158 crate tests pass; examples and `build()` callers untouched.

**What deviated from the plan:**
- The bridge observer (`seed_panel_overrides`, `panel/diegetic_panel.rs`) omits the `Res<CascadeDefaults>`
  the Phase-1 prose parenthesized. Both inserts are "only when `Some`," which read nothing from defaults; an
  unused param would fail the no-dead-code style rule. `Res<CascadeDefaults>` is added in Phase 3 when the
  FontUnit branch becomes always-on and seeds from `panel_font_unit`. The parenthetical was a forward-reference.

**Surprises:**
- The bridge had to live in `HeadlessLayoutPlugin`, not `TextRenderPlugin`: headless `PanelFontUnit`
  resolution depends on `Override<FontUnit>` being inserted even with no text renderer present. As a
  consequence `Override<TextAlpha>` is also inserted/registered in headless mode — harmless, since its cascade
  plugin (`CascadePanelChildPlugin::<PanelTextAlpha>`, in `TextRenderPlugin`) is absent there, so the component
  just sits unread.
- Non-override panels now carry *no* `Resolved<PanelTextAlpha>` at all (previously every panel got one,
  because `on_panel_added` fired on `On<Add, DiegeticPanel>`). The reader's existing `global_default` fallback
  (`apply_panel_result`, `compute_layout.rs:98`) covers them — matches the behavior-preservation analysis.

**Implications for remaining phases:**
- Phase 2's standalone bridge (`On<Add, WorldTextStyle>`) can copy `seed_panel_overrides` almost verbatim —
  same placement logic, same `Some(o.0.0)` projection.
- Phase 3 *edits* the existing `seed_panel_overrides` (adds `Res<CascadeDefaults>`, flips FontUnit to
  always-insert), rather than creating the bridge.
- Registration of `Override<TextAlpha>`/`Override<FontUnit>` + value types currently lives in
  `HeadlessLayoutPlugin`. Phase 3's unified plugin must take ownership of (or leave) this registration so no
  dangling/duplicate `register_type` remains once the per-role plugins are deleted.

### Phase 1 Review

Architect review of the remaining phases against the Phase-1 code. All findings were determinate plan
corrections (no user-facing forks) and were folded into Phases 2–4:

- **Phase 2 cross-fire mechanism corrected** — the prose claimed "the single `Exclude` marker cannot express
  not-a-panel-and-not-a-panel-child." The real wiring is `WorldTextAlpha`/`WorldFontUnit` already set
  `Exclude = PanelChild` and `PanelTextAlpha` (3-tier) has none; rewrote the paragraph to name the two actual
  spurious resolves (`Resolved<WorldTextAlpha>` on panels, `Resolved<PanelTextAlpha>` on standalones) and why
  each is unread.
- **Phase 2 reader edit-sites named** — the "tier-1 re-resolve reads the component" bullet now names
  `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` and **adds** widening `ChangedWorldTextQuery`'s
  `Or<…>` with `Changed<Override<TextAlpha>>` / `Changed<Override<FontUnit>>` (else runtime alpha/unit edits
  stop re-rendering standalone).
- **Phase 3 registration de-duplicated** — the bullet double-counted Phase-1's `Override<A>`/value-type
  registration; reworded to "add `Resolved<A>`, relocate the existing registration," and added that the
  unified plugin must register in both `HeadlessLayoutPlugin` and the render path.
- **Phase 3 schedule/ordering pinned** — propagation stays in `CascadeSet::Propagate` (`Update`); the spawn
  observer covers `PostUpdate`-spawned labels the same frame; readers (`PostUpdate`) follow by schedule order.
- **Phase 3 standalone helpers deleted, not repointed** — keeping `re_resolve_world_*` alongside the cached
  unified pass would make two writers of standalone `Resolved<…>`; the bullet now says delete them.
- **Phase 3 font-unit made concrete** — flip `seed_panel_overrides` FontUnit branch to unconditional + add
  `Res<CascadeDefaults>`; rename `CascadeDefaults::world_font_unit` → `font_unit` (struct + docstrings); noted
  the `compute_layout.rs:98` fallback becomes unreachable for panels.
- **Phase 3 label override edit-site named** — the move must replace `apply_panel_result`'s hand-rolled
  child resolution (and the `build_panel_text` per-run set), not just delete `PanelText.alpha_mode`.
- **Phase 4 topology check added** — verify a label is a `ChildOf` descendant of its panel with no
  `Resolved`-bearing intermediate.

### Phase 2 — standalone-side field→component move ✅ complete

- Move the standalone cascade-source fields into components: `WorldTextStyle.alpha_mode` (cascade role)
  → `Override<TextAlpha>`, `WorldTextStyle.unit` (cascade role) → `Override<FontUnit>`. Keep `unit` on
  `TextProps<ForLayout>` for measurement (`layout/element.rs` reads `config.unit()`); keep `world_scale`
  as the post-cascade bypass. Retire `with_alpha_mode` / `with_unit` (no callers).
- Add a standalone authoring bridge: an `On<Add, WorldTextStyle>` observer that seeds `Override<FontUnit>`
  from `style.unit()` (when `Some`) and `Override<TextAlpha>` from `style.alpha_mode()` (when `Some`), so
  `WorldTextStyle::new(Pt(..))` keeps authoring the override and the three unit examples render
  unchanged. This bridge is permanent — the same observer-bridge pattern Phase 1 introduces for panels,
  applied to the standalone authoring struct.
- Repoint the standalone cascades to source from the components: `WorldTextAlpha::Override =
  Override<TextAlpha>`, `WorldFontUnit::Override = Override<FontUnit>` (`override_value` returns
  `Some(o.0.0)` — presence is the override). The reader-side tier-1 re-resolve lives in
  `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` (`render/world_text/rendering.rs`), which
  today read `style.unit()` / `style.alpha_mode()`; repoint both to read `Override<FontUnit>` /
  `Override<TextAlpha>` through a new query param (the fields now return `None`). **Also** widen
  `ChangedWorldTextQuery` (`rendering.rs:30`): its `Or<…>` wakes `render_world_text` on
  `Changed<WorldTextStyle>`, which no longer fires for alpha/unit edits — add `Changed<Override<TextAlpha>>`
  and `Changed<Override<FontUnit>>`, or a runtime override edit stops re-rendering standalone text.
- Sweep the standalone-side `as_standalone` / `as_layout_config` callers
  (`render/panel_text/reconcile.rs`, `render/panel_text/shaping.rs`, `render/world_text/shaping.rs`):
  `TextProps::as_standalone()` (`layout/text_props.rs:598`) and `as_layout_config()` (`:741`) keep
  copying `unit` for measurement; their cascade-override role now flows through `Override<A>`.
- Transient cross-fire (removed in Phase 3): now `Override<TextAlpha>` / `Override<FontUnit>` are shared
  by the standalone and panel cascades. The standalone 2-tier attributes already set `Exclude = PanelChild`
  (`render/world_text/mod.rs:170,194`); `PanelTextAlpha` is the 3-tier panel cascade and its
  `on_panel_added` query carries no `Exclude`. So after the repoint two spurious resolves appear: a *panel*
  (which is `Without<PanelChild>`) has `on_cascade_target_added::<WorldTextAlpha>` fire on its
  `Override<TextAlpha>`, writing a spurious `Resolved<WorldTextAlpha>` onto the panel; and a *standalone*
  has the unfiltered `on_panel_added::<PanelTextAlpha>` fire on its `Override<TextAlpha>`, writing a
  spurious `Resolved<PanelTextAlpha>` onto the standalone. Both are harmless: `render_world_text` filters
  `With<WorldText>, Without<PanelChild>` and never matches a panel, and the panel-text readers filter
  `With<PanelChild>` / `With<DiegeticPanel>` and never match a standalone, so neither spurious `Resolved`
  is ever read. It disappears when Phase 3 deletes the per-role plugins.

Commits together: the standalone data move, the authoring bridge, and the two standalone-cascade readers.

### Retrospective

**What worked:**
- The repoint mirrored Phase 1 exactly: `WorldTextAlpha::override_value` / `WorldFontUnit::override_value`
  collapse to `Some(Self(o.0.0))`, and the re-resolve helpers swap their `style: &WorldTextStyle` param for
  an `&Query<&Override<A>, Without<PanelChild>>`. Build clean first try; 158 tests pass; workspace + examples
  + `typography_overlay` all compile; clippy clean.
- `with_alpha_mode` / `with_unit` had no callers (only the definitions + one doc line), so retiring them was a
  clean delete. `unit` stays on the struct as layout-measurement input; `as_standalone` / `as_layout_config`
  still copy it unchanged.

**What deviated from the plan:**
- **Added `Without<PanelChild>` to the standalone bridge query** (`seed_world_text_overrides`). The plan's
  bridge bullet specified `On<Add, WorldTextStyle>` with no filter, but panel labels *are* `WorldText`
  entities that carry `WorldTextStyle` (the original shared-component crash). Panel labels get their
  `WorldTextStyle` from `config.as_standalone()` (`reconcile.rs:74`), whose `unit` is `Some`, so an unfiltered
  bridge would insert `Override<FontUnit>` on every label. In Phase 2 that is unread (the `WorldFontUnit`
  cascade excludes `PanelChild`; `PanelFontUnit` targets the panel, not the label), but in Phase 3 the unified
  parent-walk reads `Override<FontUnit>` on *any* node — a label carrying its own would override the panel's
  inherited value instead of inheriting it. The filter makes the bridge what its name says: the *standalone*
  bridge. Labels are seeded by the panel cascade (Phase 1 / Phase 3), never by this bridge.
- **Resolved alpha eagerly in the render loop** (next to the existing eager unit resolve) instead of lazily
  inside `spawn_run`. The helper signature had to change regardless (it can no longer read `style`); resolving
  both cascade values at the top let me drop `resolved_alphas` and `defaults` from `WorldTextRenderServices`
  (the struct shrank from 7 lifetime params to 4 — it now holds only `font_registry`, `shaping_cx`, `cache`,
  `old_meshes`, `meshes`). `render_entity` / `spawn_run` take the resolved `AlphaMode` by value.

**Surprises:**
- With `with_alpha_mode` retired, `WorldTextStyle.alpha_mode` has *no* setter — `new()` always sets it `None`,
  so the bridge's `alpha_mode` branch never fires for standalone text today. It is kept for symmetry with the
  panel bridge and for the Phase-3 authoring pattern (it is not dead code: the compiler can't prove the runtime
  `Option` is always `None`).
- Because no standalone carries `Override<TextAlpha>` (alpha always `None`), the cascade plugin's `On<Add,
  Override<TextAlpha>>` observer never seeds `Resolved<WorldTextAlpha>` at spawn for standalone text. The
  render-path `re_resolve_world_text_alpha` inserts it on first render (falling back to the global default),
  exactly the self-healing the helpers exist for — same pattern as Phase 1's no-override panels.

**Implications for remaining phases:**
- Phase 3's unified `On<Add>` observer must seed `Resolved<A>` for *every* node at spawn (including no-override
  nodes), which removes the first-render `re_resolve` insert + the one-frame settle the Phase-2 helpers rely on.
  When Phase 3 deletes `re_resolve_world_font_unit` / `re_resolve_world_text_alpha`, confirm the unified spawn
  observer covers the no-override standalone case the helpers currently cover.
- The `Without<PanelChild>` bridge filter is the standalone-side contract Phase 3 inherits: labels never carry a
  bridge-seeded `Override<A>`. Phase 3's label-override move (`Override<TextAlpha>` at reconcile time) is the
  *only* sanctioned way a label gets an `Override`, so the parent-walk inheritance for `FontUnit` stays intact.
- The eager-resolution restructure already removed `resolved_alphas`/`defaults` from the render-services struct;
  Phase 3's reader repoint (`Resolved<WorldTextAlpha>` → `Resolved<TextAlpha>`) touches only the loop's
  `re_resolve` calls and the `ChangedWorldTextQuery` `Or<…>`, not the struct.

### Phase 2 Review

Architect review of the remaining phases against the Phase-2 code + retrospective. All ten findings were
determinate plan corrections (no user-facing forks) and were folded into Phases 3–4:

- **Phase 3 spawn-observer trigger corrected** — added a dedicated bullet: the unified observer must fire per
  cascade *node*, not `On<Add, Override<A>>` (the current `target.rs` model), because no standalone carries
  `Override<TextAlpha>`, so an override-gated trigger would never seed standalone `Resolved`. It must seed every
  node at spawn, replacing the deleted render-path `re_resolve_*` self-heal.
- **Phase 3 registration ownership made concrete** — named the current split (four `register_type` + the panel
  bridge in `HeadlessLayoutPlugin`; the standalone bridge in `TextRenderPlugin`) and the exact moves: relocate
  `Override<A>`/value-type registration onto the generic unified plugin, remove the four calls from
  `HeadlessLayoutPlugin`, leave both bridge observers in place.
- **Phase 3 standalone reader-repoint scope reduced** — Phase 2's eager restructure already removed
  `resolved_alphas`/`defaults` from `WorldTextRenderServices`; the bullet now names the narrow remainder (delete
  the two helpers, drop params from **both** layers of `render_world_text`, swap two `Or<…>` terms).
- **`seed_world_text_overrides` survival made explicit** — added a "keep" bullet; its `Without<PanelChild>`
  filter is the contract that labels inherit (never override) `FontUnit` under the parent-walk.
- **Phase 3 `Exclude`/`ExcludeNone` dependents enumerated** — the three `type Exclude =` bindings, the
  `Without<A::Exclude>` query positions, and the `target.rs` test module (the only coverage of
  `propagate_global_default_to_entity` / sentinel) that the unified pass must carry forward.
- **Phase 3 label-override surface widened** — `build_panel_text`'s field set, `apply_panel_result`'s
  `panel_alpha`/`existing_child_alpha`/`defaults`/`panel_entity` args, the `child_of.parent()` threading, and
  the `shape_panel_text_children` params all come out together.
- **Phase 3 two-layer `render_world_text` flagged** — the `mod.rs` `SystemParam` wrapper and the `rendering.rs`
  impl must have params dropped in lockstep.
- **Phase 4 no-override standalone alpha check added** — verify a standalone with no `Override<TextAlpha>`
  re-resolves on a runtime `CascadeDefaults::text_alpha` edit (the path the deleted helper used to cover).
- **Phase 4 headless `FontUnit` check added** — a `HeadlessLayoutPlugin`-only app resolves the panel's seeded
  `Points`; the unified spawn observer must run headless.
- **Phase 4 cross-fire cleanup confirmation** — the two spurious Phase-2 resolves vanish with the type deletion;
  the existing zero-hit `rg` sweep is the confirmation, no separate removal step.

### Phase 3 — collapse the three topologies into one parent-walking cascade ✅ complete

- Rename the existing `CascadeAttribute` trait to `CascadeAttr` (it already carries the reflection
  bounds above). Add the generic `Resolved<A>` and one hierarchical cascade plugin: a spawn-time
  `On<Add>` observer for initial resolution + a roots-first propagation pass in `CascadeSet::Propagate`
  gated on `Changed<Override<A>>`, `RemovedComponents<Override<A>>`, `Changed<ChildOf>`, parent
  `Changed<Resolved<A>>`, and `CascadeDefaults` changes; bounded walk (`CASCADE_DEPTH_CAP` + debug-only
  visited check + warn-on-exceed).
- **Spawn-observer trigger — it must fire per cascade *node*, not per `Override<A>`.** Phase 2 confirmed
  that no standalone ever carries `Override<TextAlpha>` (alpha is always `None` after `with_alpha_mode`
  was retired), so the current 2-tier machinery — `on_cascade_target_added` firing `On<Add, A::Override>`
  and early-returning when the override query misses (`cascade/target.rs:69`) — would never seed
  `Resolved<TextAlpha>` for any standalone. That is the wrong trigger for the unified design, where
  readers query `&Resolved<A>` directly and never resolve inline, so **every** node must own a
  `Resolved<A>` after spawn. The unified observer must therefore key off node spawn (every
  cascade-participating entity, regardless of whether it carries `Override<A>`) and seed by walking up
  `ChildOf` to the global default. This replaces the deleted render-path `re_resolve_*` self-heal
  (Phase 2) for the no-override case — verify the no-override standalone gets a `Resolved` at spawn.
- Registration ownership (the bridges and `register_type` calls currently live in two plugins): today
  `seed_panel_overrides` plus all four `register_type` calls (`TextAlpha`, `FontUnit`,
  `Override<TextAlpha>`, `Override<FontUnit>`) sit in `HeadlessLayoutPlugin` (`panel/mod.rs:87-91`), while
  `seed_world_text_overrides` is registered in `TextRenderPlugin` (`panel_text/mod.rs`). Phase 3 only adds
  `register_type::<Resolved<A>>()` and **relocates** the existing `Override<A>`/value-type registration
  onto the generic unified plugin (added in **both** `HeadlessLayoutPlugin` and the render path), so the
  per-attribute monomorphization registers itself once. Concretely: remove the four `register_type` calls
  from `HeadlessLayoutPlugin`, drop the per-role `register_type::<Resolved<per-role>>()` that each deleted
  plugin emitted, and leave the two bridge observers where they are (they are attribute-/struct-specific
  authoring observers, not part of the generic cascade plugin). Nothing stranded, nothing duplicated.
- Schedule + spawn ordering (state it explicitly): keep the propagation pass in `CascadeSet::Propagate`
  (`Update`); the unified `On<Add>` observer computes a node's initial `Resolved<A>` synchronously at
  spawn, which covers panel labels spawned in `PostUpdate` by `reconcile_panel_text_children` /
  `shape_panel_text_children` — those get a correct `Resolved` the same frame from the observer (it walks
  up to the panel's already-present `Override<A>`), and any *later* parent/override/default change flows
  next frame through the `Update` pass. All readers run in `PostUpdate`, after `CascadeSet::Propagate` by
  schedule order. The unified plugin must be added in **both** `HeadlessLayoutPlugin` (headless `FontUnit`
  resolution with no text renderer) and the render path — today the panel cascade lives in
  `HeadlessLayoutPlugin`, the alpha cascades in `TextRenderPlugin`.
- Unify the two font-unit cascades into one `FontUnit` attribute: one cascade global
  (`CascadeDefaults::font_unit`, the standalone default), and the panel bridge observer **always** inserts
  `Override<FontUnit>` — from `panel.font_unit()` when set, else from `CascadeDefaults::panel_font_unit` —
  so panel subtrees inherit `Points` via the parent-walk. Concretely: flip `seed_panel_overrides`'s
  FontUnit branch (`panel/diegetic_panel.rs`) from `if let Some` to an unconditional insert and add the
  `Res<CascadeDefaults>` param Phase 1 deferred; the pure `build()` cannot read the resource. Edit the
  `CascadeDefaults` struct itself (`cascade/defaults.rs`): rename `world_font_unit` → `font_unit`, keep
  `panel_font_unit` as the construction-time seed, update both field docstrings. `panel_font_unit` becomes a
  construction-time seed (no longer a cascade global). Since every panel now carries `Override<FontUnit>`,
  no panel ever resolves `FontUnit` from the cascade global — the `global_default` fallback in the panel
  font-unit read (`compute_layout.rs:98`) becomes unreachable for panels (still correct, just dead for that
  entity kind).
- Repoint every reader/filter from the per-role `Resolved<…>` types to `Resolved<TextAlpha>` /
  `Resolved<FontUnit>`: `world_text/mod.rs`, `rendering.rs` (`ChangedWorldTextQuery`), `panel_text/shaping.rs`,
  `panel_text/alpha.rs`, and the panel font-unit read. The standalone reader surface is already narrow after
  Phase 2 (the eager-resolution restructure removed `resolved_alphas`/`defaults` from `WorldTextRenderServices`):
  what remains is to **delete** the two `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` helpers
  (`rendering.rs`), drop their query params from **both** layers of `render_world_text` — the `SystemParam`
  wrapper in `world_text/mod.rs` and the impl in `rendering.rs`, edited in lockstep or it won't compile — and
  swap the two `Changed<Resolved<World…>>` terms in `ChangedWorldTextQuery`'s `Or<…>` (and its inline copy in
  the `mod.rs` wrapper) for `Changed<Resolved<TextAlpha>>` / `Changed<Resolved<FontUnit>>`. Deleting (not
  repointing) the helpers is required: the unified pass now caches `Resolved<A>` and re-resolves on
  `Changed<Override<A>>` (an in-place `get_mut` edit triggers it), so keeping them would make two writers of
  standalone `Resolved<…>` — the exact double-writer clobber the atomic commit below makes unrepresentable. Run
  `rg "Resolved<(World|Panel)(TextAlpha|FontUnit)>"` to confirm none are missed. **Keep** the `With` /
  `Without<PanelChild>` entity-selection filters.
- **Keep `seed_world_text_overrides`** (`world_text/mod.rs`) and its `Without<PanelChild>` filter — it is the
  permanent standalone authoring bridge and is *not* part of the deletions below. The filter is the contract
  that labels never carry a bridge-seeded `Override<FontUnit>`: a label's `WorldTextStyle` comes from
  `config.as_standalone()` (`reconcile.rs`), whose `unit` is `Some`, so an unfiltered bridge would put an
  `Override<FontUnit>` on every label, and under the unified parent-walk that would shadow the panel's inherited
  `Points` instead of inheriting it. The label's only sanctioned override is the `Override<TextAlpha>` inserted
  at reconcile time (next bullet); FontUnit is always inherited from the panel.
- Move the label's per-run override: delete `PanelText.alpha_mode`, insert `Override<TextAlpha>` on the
  label at reconcile time, and read it through the parent-walk like any other override. This **replaces**
  the hand-rolled child resolution in `apply_panel_result` (`panel_text/shaping.rs`), which today computes
  `panel_text.alpha_mode.map_or(panel_fallback, …)` against `Resolved<PanelTextAlpha>` and writes the
  child's `Resolved` itself. The full surface that comes out together: the `panel_text.alpha_mode` field set
  in `build_panel_text` (`config.alpha_mode()`), the `apply_panel_result` resolution body, its `panel_alpha` /
  `existing_child_alpha` queries plus the `defaults` / `panel_entity` args, the `child_of.parent()` argument
  threading, and the two corresponding query params on `shape_panel_text_children`. Letting the generic pass
  own the label's `Resolved<TextAlpha>` — otherwise the label has two competing writers.
- Delete the per-role types (`WorldTextAlpha` / `PanelTextAlpha` / `WorldFontUnit` / `PanelFontUnit`),
  the three plugins (`CascadeEntityPlugin` / `CascadePanelPlugin` / `CascadePanelChildPlugin`), and
  `Exclude` / `ExcludeNone`. `Exclude`/`ExcludeNone` dependents that go with them: the three `type Exclude =`
  bindings (`WorldTextAlpha`/`WorldFontUnit` → `PanelChild` at `world_text/mod.rs`; `PanelFontUnit` →
  `ExcludeNone` at `diegetic_panel.rs`) and the `Without<A::Exclude>` query positions in `target.rs`. Deleting
  `target.rs` also deletes its test module (`target.rs:104-276`) — the only coverage of
  `propagate_global_default_to_entity` / `should_propagate_defaults` (global-default mutation with and without
  an override; the `Local<Option<A>>` sentinel short-circuit). The unified pass re-implements that propagation,
  so it must carry that test coverage forward, not just drop it.

Commit atomically: the new plugin, the reader repoint, and the old-type deletions are mutually
dependent — they cannot land separately and stay green, because old and new both writing `Resolved` in
one frame would clobber. That same atomic deletion ends the Phase-2 cross-fire and guarantees a single
writer: once the old per-role types and plugins are gone, two writers for one attribute is
unrepresentable — no runtime guard or plugin-count assert is needed (an accidental double-add of the
same generic plugin is caught by Bevy's built-in plugin uniqueness).

### Retrospective

**What worked:**
- The unified `CascadePlugin<A: CascadeAttr>` + a single `propagate_cascade::<A>` system replaced all
  three topology plugins and four per-role types. Full workspace builds; 157 crate tests pass (the
  generic-plugin test module carries forward the deleted `target.rs` / `panel_child.rs` coverage); clippy
  clean (`--all-targets --features typography_overlay`).
- The Phase-2 cross-fire is gone by construction: a zero-hit `rg` for `Resolved<(World|Panel)…>` /
  `WorldTextAlpha` / `PanelTextAlpha` / `…FontUnit` / `ExcludeNone` / the old plugins confirms the spurious
  resolves vanished with the type deletions — no separate removal step, exactly as the plan predicted.

**What deviated from the plan:**
- **No generic spawn observer; spawn seeding lives in the node-kind observers.** Plan finding #1 said "the
  unified observer must fire per cascade *node*." A fully attribute-generic plugin cannot identify which
  entities participate or which `Resolved<A>` each one reads without coupling to `WorldText` /
  `DiegeticPanel` / `PanelChild`. So `CascadePlugin` owns registration + propagation only, and the three
  node-kind observers seed `Resolved<A>` at spawn: `seed_world_text_overrides` (standalone — both
  attributes), `seed_panel_overrides` (panel — `Resolved<FontUnit>`), and a **new** `seed_panel_child_alpha`
  (`On<Add, PanelChild>`, label — `Resolved<TextAlpha>` via `resolve_walk`). The "fire per node" intent is
  realized as per-node-kind seeds, each calling the shared `resolve_walk`.
- **Propagation re-walks raw `Override<A>` (via `resolve_walk`) rather than reading the parent's cached
  `Resolved<A>`.** Unconditionally correct, simpler, and still writes `Resolved<A>` inequality-guarded so
  `Changed<Resolved<A>>` fan-out wakes readers. The cached-parent optimization was unnecessary at depth ~4.
- **`collect_subtree` uses a visited-guarded DFS, not Bevy's `iter_descendants`** — a `ChildOf` self-cycle
  makes `Children` self-referential and would infinite-loop `iter_descendants`; the explicit visited set
  (the `dirty` set itself) terminates the down-walk, mirroring `resolve_walk`'s cap/visited guard on the
  up-walk.
- **No label `Override<TextAlpha>` is inserted at reconcile.** `alpha_mode` has no live authoring path
  (always `None` since `with_alpha_mode` was retired in Phase 2, on both `WorldTextStyle` and
  `LayoutTextStyle`), so `PanelText.alpha_mode` was deleted and labels purely inherit the panel through the
  walk — there is no value to source a per-label override from. Behavior is identical to today's
  `apply_panel_result` (whose `panel_text.alpha_mode` was always `None`, so it always fell through to the
  panel's resolved alpha).
  **(Reversed in Phase 4, option C.)** This deviation followed from Phase 2 deleting `with_alpha_mode` as
  "no callers" — which missed that it is documented public API (the README). Phase 4 restored
  `with_alpha_mode` and reinstated the label-override path: `reconcile_panel_text_children` now inserts a
  label `Override<TextAlpha>` from `style.alpha_mode()`. Labels inherit only when they author nothing.
- **The panel carries `Resolved<FontUnit>` but not `Resolved<TextAlpha>`.** No reader reads a panel's
  alpha; only its labels render text, and they walk up to the panel's raw `Override<TextAlpha>`. The
  `compute_layout.rs` panel-font-unit fallback was repointed straight to `defaults.panel_font_unit` (not a
  `FontUnit::global_default`, which would read the standalone default `font_unit`) to preserve the exact —
  now unreachable — value.

**Surprises:**
- `shape_panel_text_children` no longer needs to watch alpha at all: it only watched
  `Changed<Resolved<PanelTextAlpha>>` to rewrite the child's `Resolved`, which the propagation pass now
  owns. The term was dropped from its `Or<…>` entirely; `build_panel_text_meshes` still watches
  `Changed<Resolved<TextAlpha>>` to rebuild the glyph material when a panel-alpha change flows to its labels.

**Implications for remaining phases:**
- Phase 4's cross-fire-cleanup confirmation is **already done** (the zero-hit sweep above).
- Phase 4's verification should exercise the real node kinds (`WorldText` / `DiegeticPanel` / labels)
  end-to-end, since the carried-forward unit tests use a throwaway `TestUnit` + a stand-in seed observer
  rather than the real bridges. The interesting same-frame path is a label's `seed_panel_child_alpha` →
  `Resolved<TextAlpha>` landing before `build_panel_text_meshes` reads it.

### Phase 3 Review

Architect review of Phase 4 against the implemented Phase-3 code + retrospective. All eleven findings were
determinate (zero sent for user approval) and were folded into Phase 4:

- **Cross-fire confirmation is redundant** — the zero-hit `rg` ran in Phase 3; Phase 4's sweep is now a
  re-run guard folded into the reflection bullet, not a fresh task.
- **Generic mechanics already covered** — the carried-forward `plugin.rs` test module proves spawn /
  inherit / override-wins / mutation-propagation / defaults-mutation / removal / reparent / self-parent /
  sentinel; Phase 4's verify list now defers those and scopes its own new tests to the real node kinds and
  the same-frame path.
- **Walk-termination gaps** — added explicit Phase-4 tests for a two-node `ChildOf` cycle and a
  cap-exceed chain (> `CASCADE_DEPTH_CAP`), which the unit module (depth-1 self-parent only) does not cover.
- **Same-frame fallback masking** — the same-frame label-seed test must assert the label's material picks up
  the panel's *overridden* alpha, because `build_panel_text_meshes`'s `defaults.text_alpha` fallback would
  silently hide an ordering regression if the panel's override equalled the global default.
- **Parent-walk cross-context behavior** — clarified as the *designed* tree-following semantics (the walk
  climbs `ChildOf` until an `Override<A>` or the root); not a leak, since `Override<A>` is `pub(crate)` and
  only the bridges insert it. Phase 4 asserts the rule both directions rather than a "stops at panel" guarantee.
- **Deleted label-override path** — Phase 4 assertions and the changed-names table were rephrased from
  "per-run label override wins" to the inheritance contract (labels carry no `Override<TextAlpha>`; their
  `Resolved` equals the panel's), matching retrospective deviation #4.
- **Example tier-3** — the demo keeps three `TextAlpha` tiers but sources the label tier via a documented
  raw `Override<TextAlpha>` insert on one label (no builder authors one), proving the parent-walk honors a
  node's own override.
- **Same-batch spawn re-scoped** — the "same-command-batch panel+child" item was replaced with the actual
  reconcile-driven label spawn (panel first, label later against the panel's already-present `Override`).
- **Headless fallback assertion** — the headless `FontUnit` check now also asserts the
  `defaults.panel_font_unit` fallback branch is unreached.
- **Stale `cascade_set.rs` doc** — its tier-2 (`Changed<A::PanelOverride>`) / tier-3 comment describes the
  deleted per-role model; folded into Phase 4's sweep (a doc-only fix, deferred to Phase 4 because
  phase-review does not touch implementation code).
- **No-override standalone alpha** — must be a headed automated test driving the propagation pass's
  `default_changed` branch (the only path that changes standalone alpha), since `text_alpha.rs` pins every
  panel to `Blend` so the runtime mutation drives only the standalones.

### Phase 4 — verification + cascade example ✅ complete

- **Generic cascade mechanics are already covered — do not re-test them here.** The `cascade/plugin.rs`
  test module (carried forward from the deleted `target.rs` / `panel_child.rs`) already proves: spawn →
  global default; spawn → own override; child inherits parent; child override wins; in-place `get_mut`
  re-resolves and propagates to children; global-default mutation updates no-override nodes and skips
  override nodes; `Override<A>` removal re-inherits; reparent re-resolves; `ChildOf(self)` self-parent
  terminates; and the `CascadeDefaults` sentinel short-circuits unrelated-field changes. Phase 4 adds the
  two cases that module deliberately stubs out: the **real node kinds** (`WorldText` / `DiegeticPanel` /
  labels via the real bridges, not `TestUnit` + a stand-in seed) and the **same-frame** reconcile path.
- **Walk-termination gaps the unit tests miss.** Add a **two-node `ChildOf` cycle** test (the module only
  covers a depth-1 self-parent) and a **cap-exceed** test — a `ChildOf` chain longer than
  `CASCADE_DEPTH_CAP` (64) resolves to the global default with the `warn!` and no hang. These exercise the
  `resolve_walk` cycle/visited and cap branches (`resolved.rs`) that have no coverage yet.
- **Same-frame label seed lands before the mesh reader, and must be asserted against an *overridden* panel
  alpha.** A label is spawned by `reconcile_panel_text_children` (`PostUpdate`, via `with_child`), not
  co-spawned with its panel; `seed_panel_child_alpha` (`On<Add, PanelChild>`) inserts its
  `Resolved<TextAlpha>` during that command flush, before `build_panel_text_meshes`
  (`.after(shape_panel_text_children)`) reads it. Verify the label's glyph material picks up the panel's
  **overridden** `text_alpha_mode` (not the global default) — `build_panel_text_meshes`'s
  `defaults.text_alpha` fallback would silently mask an ordering regression if the test used a panel whose
  override equals the global default.
- **No-override standalone alpha responds to a runtime `CascadeDefaults::text_alpha` edit — as an automated
  test, through the propagation pass.** Standalone text never carries `Override<TextAlpha>` (alpha is always
  `None`), so it resolves purely to `CascadeDefaults::text_alpha`; the deleted `re_resolve_world_text_alpha`
  used to drive this read-side, and the propagation pass's `default_changed` branch must now do it. In
  `text_alpha.rs` every panel pins `.text_alpha_mode(Blend)`, so the runtime `defaults.text_alpha` mutation
  drives **only** the two standalone `WorldText` entities — assert their `Resolved<TextAlpha>` updates in a
  headed test, not by visual inspection.
- **Headless `FontUnit` resolution.** A `HeadlessLayoutPlugin`-only app (no text renderer) resolves a
  panel's `Resolved<FontUnit>` to the seeded `Points`, and `compute_layout.rs` reads it — `CascadePlugin::<FontUnit>`
  and `seed_panel_overrides` both run in `HeadlessLayoutPlugin`. Assert the panel's seeded value resolves
  headless **and** that the `defaults.panel_font_unit` fallback branch (`compute_layout.rs`) is unreached
  (every panel carries `Resolved<FontUnit>`), since that fallback would otherwise mask a broken seed.
- **Parent-walk follows `ChildOf` through every ancestor — assert the designed semantics, both directions.**
  Positive: a panel label is a `ChildOf` descendant of the panel with no `Resolved`-bearing intermediate
  (`reconcile_panel_text_children` parents labels directly under the panel). On the cross-context question:
  `resolve_walk` does **not** stop at the panel boundary — it climbs until it finds an `Override<A>` or hits
  the root, which is the design's stated rule (*my override, else my parent's, else global*). This is not a
  leak: `Override<A>` is `pub(crate)` and inserted only by the bridges (on panels and standalones), so no
  unrelated 3D-scene ancestor carries one in practice. Add a test that documents the rule (a label under a
  panel with no `text_alpha_mode` resolves to the global default, not to some intermediate), confirming the
  walk's behavior is the intended tree-following one.
- **Reflection + stale-doc sweep** (the cross-fire cleanup is already confirmed). `rg` for lingering
  `Resolved<World…>` / `Resolved<Panel…>` / `WorldTextAlpha` / `PanelTextAlpha` / `…FontUnit` / `ExcludeNone`
  / `A::PanelOverride` — the zero-hit cross-fire confirmation ran in Phase 3, so this is a re-run guard, not
  a fresh task. Also fix the **stale `cascade_set.rs` doc comment**, which still describes the deleted
  "tier-2 (`Changed<A::PanelOverride>`) / tier-3" per-role model. Confirm every attribute registers `A`,
  `Override<A>`, and `Resolved<A>` (via `CascadePlugin::<A>`).
- Add a cascade demonstration example: one scene showing `TextAlpha` resolving at each of three tiers —
  global default (standalone) → panel override (inherited by a label) → a label's **own** override — and
  `FontUnit` resolving global default (standalone) vs. panel-seeded override inherited by a label, with
  on-screen labels showing which tier won. There is no builder that authors a label-level alpha override
  (deviation #4 — `PanelText.alpha_mode` was deleted), so the example demonstrates the third tier by
  inserting a raw `Override<TextAlpha>` directly on one label entity, with a comment that this proves the
  parent-walk honors a node's own override even though no builder authors one.

### Retrospective

**What worked:**
- All seven verification tests landed and pass (167 crate tests, +10 over Phase 3's 157): two-node `ChildOf`
  cycle + cap-exceed chain (`cascade/plugin.rs`); standalone bridge no-override / explicit-unit / explicit-alpha
  + runtime-default-change (`render/world_text/mod.rs`); panel-label same-frame inheritance, cross-context
  global-default, and label-own-override-via-real-reconcile (`render/panel_text/alpha.rs`); headless `FontUnit`
  (`panel/compute_layout.rs`).
- Stale-doc sweep done: rewrote the `cascade_set.rs` `Propagate` doc (was the deleted tier-2/tier-3 /
  `A::PanelOverride` model); the deleted-type `rg` sweep is zero-hit; registration confirmed
  (`CascadePlugin::<A>` registers `A`/`Override<A>`/`Resolved<A>`; `FontUnit` in `HeadlessLayoutPlugin`,
  `TextAlpha` in `TextRenderPlugin`).
- The same-frame label-seed test asserts against an *overridden* alpha (`Add` ≠ default `Blend`), so the
  `build_panel_text_meshes` fallback cannot mask an ordering regression — exactly the masking risk the
  Phase-3 review flagged.

**What deviated from the plan — the big one (decided with the user, option C):**
- **The planned tier-3 example was impossible as written, and uncovered a Phase-2 public-API regression.**
  Item #8 said to demonstrate a label's own alpha by inserting a raw `Override<TextAlpha>` on a label entity.
  But `Override<A>`, `TextAlpha`, `Resolved`, and `PanelChild` are all `pub(crate)` — an example is a
  separate crate and cannot touch them. Investigating the public alpha surface surfaced that the README
  (lines 54–56, 74–78) still documents `WorldTextStyle`/`LayoutTextStyle::with_alpha_mode` with runnable
  snippets, but **Phase 2 deleted that method as "no callers" — missing that it is documented public API**,
  and **Phase 3 deviation #4 then deleted the label-override path on that basis** ("labels purely inherit").
  So the documented public API and the code had silently diverged since Phase 2.
- **Resolution (user picked C — restore full per-style alpha):** re-added `with_alpha_mode` to the shared
  `TextProps<C>` impl (covers both `WorldTextStyle` and `LayoutTextStyle`), and reinstated the label-override
  path in `reconcile_panel_text_children` — it now inserts `Override<TextAlpha>` on a label from
  `style.alpha_mode()` (in the spawn bundle alongside `PanelChild`, so `seed_panel_child_alpha` seeds the
  label's own alpha with no settle frame; insert/remove on the update path for the propagation pass). The
  standalone bridge already consumed `alpha_mode()`, so standalone tier-3 alpha came for free. This **reverses
  Phase 2's deletion and Phase 3 deviation #4** and makes the README accurate again.
- The example demonstrates tier-3 alpha through the public `with_alpha_mode` (a panel label authoring
  `Multiply`, a standalone authoring `Add`) — **no internals exposed**, the cleaner path than widening the API
  with the raw `Override` component. `examples/cascade.rs` (auto-discovered, no `Cargo.toml` entry).

**Surprises:**
- The README/code drift was a latent public-API regression none of the Phase-1/2/3 reviews caught, because
  "no callers" was scanned inside the crate, not against the documented public surface.
- `with_alpha_mode` on `WorldTextStyle` was nearly free to restore — the standalone bridge
  (`seed_world_text_overrides`) already inserted `Override<TextAlpha>` from `alpha_mode()`; only the setter
  was missing.

**Implications:**
- This document now contains claims that the C reversal contradicts — they must be corrected (Phase-4 Review
  below): the changed-names table row ("labels have no alpha-authoring path… no label-level override is
  inserted"), Phase 3 retrospective deviation #4, and the design prose in
  [The split](#the-split--overrides-leave-worldtextstyle-diegeticpanel-and-paneltext) /
  [Read side](#read-side--entity-selection-filters-stay) that asserts labels purely inherit alpha.
- No phases remain after Phase 4; the review below targets the doc/code consistency the reversal exposed,
  not future phases.

### Phase 4 Review

Architect review of the Phase-4 code + the option-C reversal (no future phases remain; the review targeted
restoration completeness and doc consistency). Ten findings; four were doc edits, one closed a test gap, five
were confirmations needing no change. None went to the user — all were determinate.

- **Doc: changed-names row corrected** — the row claimed "labels have no alpha-authoring path… no label-level
  override is inserted." Rewritten to describe the restored `LayoutTextStyle::with_alpha_mode` →
  `reconcile_panel_text_children` → label `Override<TextAlpha>` path, including the spawn (no settle) vs.
  runtime-update (one-frame settle) timing.
- **Doc: registered-attributes `TextAlpha` row corrected** — "Override source today" now lists the real
  sources (`WorldTextStyle.alpha_mode`, `DiegeticPanel.text_alpha_mode`, `LayoutTextStyle.alpha_mode` →
  label override), and the resolution column reads "label own → panel → global."
- **Doc: read-side prose corrected** — notes a panel label reads its own `Resolved<TextAlpha>` from either the
  inherited panel value or its own restored override.
- **Doc: Phase-3 deviation #4 annotated as reversed** — marked "(Reversed in Phase 4, option C)" with the
  Phase-2 root cause (deleted documented public API as "no callers").
- **Test gap closed (significant finding #8)** — the reconcile *update* arm (a label's alpha changing /
  dropping to re-inherit) had no direct test; added
  `label_alpha_change_reinherits_panel_alpha_through_reconcile` driving a real `set_tree` relayout
  `Multiply → inherited Add`. 168 crate tests pass.
- **Confirmations (no change):** `build_panel_text_meshes` already wakes on `Changed<Resolved<TextAlpha>>`, so
  a label's own-alpha change re-renders; the README `with_alpha_mode` snippets compile as written; the
  standalone `with_alpha_mode` + `Without<PanelChild>` bridge filter do not double-insert; reconcile inserts
  only `Override<TextAlpha>` (never `Override<FontUnit>`) on labels, so the FontUnit-inherits-from-panel
  contract is intact and the spawn-bundle ordering (override present when `seed_panel_child_alpha` fires) holds.

## Changed names

| Was | Now | Location |
| --- | --- | --- |
| `PanelSlugTextRun` | `PanelText` | `render/panel_text/mod.rs` |
| `render/text_renderer/` module | `render/panel_text/` | — |
| `PanelTextChild` (marker) | `PanelChild` | `render/world_text/mod.rs` (next to `WorldText`) |
| `CascadeAttribute` (trait) | `CascadeAttr` | `cascade/resolved.rs` |
| alpha/unit fields as the **cascade source** | `Override<TextAlpha>` / `Override<FontUnit>` components, inserted by an `On<Add>` bridge | fields stay on `WorldTextStyle` / `DiegeticPanel` as authoring inputs the bridge reads once at spawn; the cascade reads the component. `PanelText.alpha_mode` is gone; a panel label authors its own alpha via `LayoutTextStyle::with_alpha_mode` (restored in Phase 4), which `reconcile_panel_text_children` turns into a label `Override<TextAlpha>` from `style.alpha_mode()` — present at spawn (in the `PanelChild` bundle, no settle frame) and insert/removed on update (a runtime change settles one frame later through the propagation pass). Absent → the label inherits the panel's `Override<TextAlpha>` via the walk |
| per-role `Resolved<WorldTextAlpha>` / `Resolved<PanelTextAlpha>` | `Resolved<TextAlpha>` | one resolved type per attribute |
| per-role `Resolved<WorldFontUnit>` / `Resolved<PanelFontUnit>` | `Resolved<FontUnit>` | one font-unit attribute; panel `Points` is a seeded override, not a second type |
| `CascadeDefaults::world_font_unit` | `CascadeDefaults::font_unit` | sole cascade global for `FontUnit` (standalone default) |
| `CascadeDefaults::panel_font_unit` (live global) | construction-time seed for the panel's `Override<FontUnit>` | no longer cascade-propagated |
| `WorldTextStyle` = `TextProps<ForStandalone>` | unchanged; **loses** `alpha_mode` / `unit` as cascade overrides (`unit` stays for layout measurement) | `layout/text_props.rs` |
